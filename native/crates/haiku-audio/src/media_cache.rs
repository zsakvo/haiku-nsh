//! 媒体文件磁盘缓存：落盘路径、按文件扫用量、LRU 淘汰、清空。
//!
//! 键为 `pluginId + "\x1f" + trackId`；磁盘文件名为其稳定 128-bit 十六进制 id。
//! **不做 JSON 索引**——状态以 `objects/` 下文件为准（完整无后缀 / 半成品 `.part`），
//! 用量与淘汰扫盘 + mtime；元数据/歌词等由 Dart MMKV 持有。
//! 根目录由 Dart 注入（`<AppSupport>/haiku_media_cache`）。

use std::fs::{self, File, OpenOptions};
#[cfg(test)]
use std::io::Write;
use std::io::{self, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 默认上限 2 GiB。
pub const DEFAULT_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// 滑杆下限 0.5 GiB。
pub const MIN_MAX_BYTES: u64 = 512 * 1024 * 1024;
/// 滑杆上限 20 GiB。
pub const MAX_MAX_BYTES: u64 = 20 * 1024 * 1024 * 1024;

struct State {
    root: Option<PathBuf>,
    enabled: bool,
    max_bytes: u64,
    /// 当前正在播放与已预取下一首、禁止淘汰的 key。
    pinned: [Option<String>; 2],
}

impl Default for State {
    fn default() -> Self {
        Self {
            root: None,
            enabled: true,
            max_bytes: DEFAULT_MAX_BYTES,
            pinned: [None, None],
        }
    }
}

fn cache() -> &'static Mutex<State> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Mutex<State>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(State::default()))
}

/// 稳定文件 id（双 FNV-1a 拼 32 hex），跨进程/版本一致。
pub fn file_id(key: &str) -> String {
    let mut h1: u64 = 0xcbf29ce484222325;
    let mut h2: u64 = 0x100000001b3;
    for &b in key.as_bytes() {
        h1 ^= b as u64;
        h1 = h1.wrapping_mul(0x100000001b3);
        h2 ^= (b as u64).wrapping_mul(0x9e3779b97f4a7c15);
        h2 = h2.wrapping_mul(0x100000001b3);
    }
    format!("{h1:016x}{h2:016x}")
}

fn clamp_max(bytes: u64) -> u64 {
    bytes.clamp(MIN_MAX_BYTES, MAX_MAX_BYTES)
}

fn objects_dir(root: &Path) -> PathBuf {
    root.join("objects")
}

fn ensure_layout(root: &Path) -> io::Result<()> {
    fs::create_dir_all(objects_dir(root))?;
    // 清理遗留 JSON 索引（旧版）；失败忽略。
    let _ = fs::remove_file(root.join("index.json"));
    let _ = fs::remove_file(root.join("index.json.tmp"));
    Ok(())
}

/// 设置缓存根目录。可重复调用。
pub fn set_root(path: String) -> Result<(), String> {
    let root = PathBuf::from(path);
    ensure_layout(&root).map_err(|e| format!("创建缓存目录失败: {e}"))?;
    let mut g = cache().lock().map_err(|e| e.to_string())?;
    g.root = Some(root);
    Ok(())
}

pub fn set_enabled(enabled: bool) {
    if let Ok(mut g) = cache().lock() {
        g.enabled = enabled;
    }
}

pub fn set_max_bytes(max: u64) {
    if let Ok(mut g) = cache().lock() {
        g.max_bytes = clamp_max(max);
        if let Some(root) = g.root.clone() {
            let pinned = g.pinned.clone();
            let max = g.max_bytes;
            drop(g);
            let _ = evict_to(&root, max, &pinned);
        }
    }
}

pub fn set_pinned(current: Option<String>, next: Option<String>) {
    if let Ok(mut g) = cache().lock() {
        g.pinned = [current, next];
    }
}

pub fn set_current_pinned(key: Option<String>) {
    if let Ok(mut g) = cache().lock() {
        g.pinned[0] = key;
    }
}

pub fn set_next_pinned(key: Option<String>) {
    if let Ok(mut g) = cache().lock() {
        g.pinned[1] = key;
    }
}

pub fn promote_next_pinned() {
    if let Ok(mut g) = cache().lock() {
        g.pinned[0] = g.pinned[1].take();
    }
}

pub fn is_enabled() -> bool {
    cache().lock().map(|g| g.enabled).unwrap_or(false)
}

pub fn root_path() -> Option<String> {
    cache()
        .lock()
        .ok()
        .and_then(|g| g.root.as_ref().map(|p| p.display().to_string()))
}

#[derive(Debug, Clone)]
pub enum CacheStatus {
    None,
    Partial {
        path: PathBuf,
        size: u64,
        total_len: Option<u64>,
    },
    Complete {
        path: PathBuf,
        size: u64,
    },
}

/// 查询条目：以磁盘文件为准（完整优先于 `.part`）。
pub fn status(key: &str) -> CacheStatus {
    let Ok(g) = cache().lock() else {
        return CacheStatus::None;
    };
    if !g.enabled {
        return CacheStatus::None;
    }
    let Some(root) = g.root.as_ref() else {
        return CacheStatus::None;
    };
    let id = file_id(key);
    let complete = objects_dir(root).join(&id);
    if complete.is_file() {
        let size = fs::metadata(&complete).map(|m| m.len()).unwrap_or(0);
        if size > 0 {
            return CacheStatus::Complete {
                path: complete,
                size,
            };
        }
    }
    let part = objects_dir(root).join(format!("{id}.part"));
    if part.is_file() {
        let size = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if size > 0 {
            return CacheStatus::Partial {
                path: part,
                size,
                total_len: None,
            };
        }
    }
    CacheStatus::None
}

/// 标记访问：touch 完整文件 mtime，供 LRU。
pub fn touch(key: &str) {
    let Ok(g) = cache().lock() else {
        return;
    };
    let Some(root) = g.root.as_ref() else {
        return;
    };
    let path = objects_dir(root).join(file_id(key));
    if path.is_file() {
        let _ = filetime_touch(&path);
    }
}

/// 尽量更新 atime/mtime 为现在（跨平台：rewrite open truncate no — 用 set_times if available）。
fn filetime_touch(path: &Path) -> io::Result<()> {
    // 打开读写不截断即可更新 mtime（多数平台）。
    OpenOptions::new()
        .write(true)
        .open(path)?
        .set_times(std::fs::FileTimes::new().set_modified(SystemTime::now()))?;
    Ok(())
}

pub struct Usage {
    pub used_bytes: u64,
    pub max_bytes: u64,
    pub entry_count: u64,
}

/// 扫 `objects/` 统计用量（真实磁盘占用，不依赖索引）。
pub fn usage() -> Usage {
    let g = cache().lock().unwrap_or_else(|e| e.into_inner());
    let max_bytes = g.max_bytes;
    let (used_bytes, entry_count) = g.root.as_ref().map(|r| scan_usage(r)).unwrap_or((0, 0));
    Usage {
        used_bytes,
        max_bytes,
        entry_count,
    }
}

fn scan_usage(root: &Path) -> (u64, u64) {
    let dir = objects_dir(root);
    let Ok(rd) = fs::read_dir(&dir) else {
        return (0, 0);
    };
    let mut used = 0u64;
    let mut count = 0u64;
    for ent in rd.flatten() {
        let path = ent.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        // 完整文件（无 .part）计一首；.part 计入字节但不算完整 entry。
        if let Ok(meta) = ent.metadata() {
            used = used.saturating_add(meta.len());
            if !name.ends_with(".part") {
                count += 1;
            }
        }
    }
    (used, count)
}

/// 清空 objects。
pub fn clear() -> Result<(), String> {
    let g = cache().lock().map_err(|e| e.to_string())?;
    let Some(root) = g.root.clone() else {
        return Err("缓存目录未初始化".into());
    };
    drop(g);
    let objects = objects_dir(&root);
    if objects.is_dir() {
        fs::remove_dir_all(&objects).map_err(|e| e.to_string())?;
    }
    ensure_layout(&root).map_err(|e| e.to_string())?;
    Ok(())
}

/// 打开/创建 `.part` 供边下边写。返回 (文件, 已有字节数, 路径)。
pub fn open_part_writer(key: &str) -> Result<(File, u64, PathBuf), String> {
    let g = cache().lock().map_err(|e| e.to_string())?;
    if !g.enabled {
        return Err("缓存已关闭".into());
    }
    let Some(root) = g.root.clone() else {
        return Err("缓存目录未初始化".into());
    };
    drop(g);
    ensure_layout(&root).map_err(|e| e.to_string())?;

    let id = file_id(key);
    let complete = objects_dir(&root).join(&id);
    if complete.is_file() {
        let size = fs::metadata(&complete).map(|m| m.len()).unwrap_or(0);
        if size > 0 {
            return Err("已有完整缓存".into());
        }
    }

    let path = objects_dir(&root).join(format!("{id}.part"));
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| format!("打开缓存写入失败: {e}"))?;
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    file.seek(SeekFrom::End(0))
        .map_err(|e| format!("定位缓存文件末尾失败: {e}"))?;
    Ok((file, size, path))
}

/// 半成品推进：仅 touch mtime（无索引可写）。
pub fn update_part(_key: &str, _size: u64, _total_len: Option<u64>) {
    // no-op：状态以文件长度为准；保留 API 兼容 player 调用。
}

/// 只有已知总长且字节完整时才可晋升。未知总长无法区分正常 chunked EOF 与连接截断，
/// 保守保留为 `.part`，避免把坏文件永久当作完整缓存。
pub(crate) fn is_complete_download(size: u64, total_len: Option<u64>) -> bool {
    matches!(total_len, Some(total) if total > 0 && size >= total)
}

/// 下载完整：`.part` → 正式文件，跑 LRU。
pub fn finalize(key: &str, size: u64, total_len: Option<u64>, _content_type: Option<String>) {
    if !is_complete_download(size, total_len) {
        return;
    }
    let Ok(g) = cache().lock() else {
        return;
    };
    let Some(root) = g.root.clone() else {
        return;
    };
    let max = g.max_bytes;
    let pinned = g.pinned.clone();
    drop(g);

    let id = file_id(key);
    let part_path = objects_dir(&root).join(format!("{id}.part"));
    let final_path = objects_dir(&root).join(&id);
    if part_path.metadata().map(|m| m.len()).ok() != Some(size) {
        return;
    }
    if part_path.is_file() {
        let _ = fs::rename(&part_path, &final_path);
    }
    let _ = filetime_touch(&final_path);
    let _ = evict_to(&root, max, &pinned);
}

/// 删除某 key 的完整与残片。
pub fn remove(key: &str) {
    let Ok(g) = cache().lock() else {
        return;
    };
    let Some(root) = g.root.clone() else {
        return;
    };
    drop(g);
    let id = file_id(key);
    let dir = objects_dir(&root);
    let _ = fs::remove_file(dir.join(&id));
    let _ = fs::remove_file(dir.join(format!("{id}.part")));
}

struct DiskEntry {
    path: PathBuf,
    /// 文件名 stem（hash），用于 pinned 比对时反查困难——pinned 是逻辑 key。
    /// 淘汰时：若 path 对应 pinned key 的 file_id 则跳过。
    file_id: String,
    is_part: bool,
    size: u64,
    mtime: SystemTime,
}

fn list_entries(root: &Path) -> Vec<DiskEntry> {
    let dir = objects_dir(root);
    let Ok(rd) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for ent in rd.flatten() {
        let path = ent.path();
        if !path.is_file() {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let is_part = name.ends_with(".part");
        let file_id = if is_part {
            name.trim_end_matches(".part").to_string()
        } else {
            name
        };
        let meta = match ent.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
        out.push(DiskEntry {
            path,
            file_id,
            is_part,
            size: meta.len(),
            mtime,
        });
    }
    out
}

fn evict_to(root: &Path, max: u64, pinned_keys: &[Option<String>; 2]) -> io::Result<()> {
    let pinned_ids = pinned_keys
        .iter()
        .filter_map(|key| key.as_deref().map(file_id))
        .collect::<Vec<_>>();
    loop {
        let mut entries = list_entries(root);
        let used: u64 = entries.iter().map(|e| e.size).sum();
        if used <= max {
            break;
        }
        // 残片优先，再按 mtime 最旧。
        entries.sort_by(|a, b| {
            b.is_part
                .cmp(&a.is_part) // true 残片在前
                .then_with(|| a.mtime.cmp(&b.mtime))
        });
        let victim = entries
            .into_iter()
            .find(|e| !pinned_ids.contains(&e.file_id));
        let Some(v) = victim else {
            break;
        };
        fs::remove_file(&v.path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    #[test]
    fn file_id_stable() {
        assert_eq!(file_id("a\x1fb"), file_id("a\x1fb"));
        assert_ne!(file_id("a\x1fb"), file_id("a\x1fc"));
    }

    static TEST_LOCK: Mutex<()> = Mutex::new(());
    #[test]
    fn finalize_status_and_usage() {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = env::temp_dir().join(format!("haiku_mcache_{}", now_ms()));
        let _ = fs::remove_dir_all(&dir);
        set_root(dir.display().to_string()).unwrap();
        set_enabled(true);
        set_max_bytes(DEFAULT_MAX_BYTES);
        let key = "netease\x1fnetease:song:1";
        let (mut f, size, _) = open_part_writer(key).unwrap();
        assert_eq!(size, 0);
        f.write_all(b"hello-audio").unwrap();
        f.sync_all().unwrap();
        finalize(key, 11, Some(11), None);
        match status(key) {
            CacheStatus::Complete { size, .. } => assert_eq!(size, 11),
            o => panic!("expected complete, got {o:?}"),
        }
        let u = usage();
        assert_eq!(u.used_bytes, 11);
        assert_eq!(u.entry_count, 1);
        clear().unwrap();
        assert!(matches!(status(key), CacheStatus::None));
        assert_eq!(usage().used_bytes, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn capacity_lru_pins_integrity_and_local_safety() {
        let _guard = TEST_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let local = root.path().join("original.flac");
        fs::write(&local, b"local original").unwrap();
        let cache_root = root.path().join("media-cache");
        set_root(cache_root.display().to_string()).unwrap();
        set_enabled(true);
        set_max_bytes(1);
        assert_eq!(usage().max_bytes, MIN_MAX_BYTES);
        set_max_bytes(u64::MAX);
        assert_eq!(usage().max_bytes, MAX_MAX_BYTES);
        for (i, key) in ["old", "current", "next", "recent"].iter().enumerate() {
            let (mut file, _, _) = open_part_writer(key).unwrap();
            file.write_all(b"abcd").unwrap();
            drop(file);
            finalize(key, 4, Some(4), None);
            File::options()
                .write(true)
                .open(objects_dir(&cache_root).join(file_id(key)))
                .unwrap()
                .set_times(
                    fs::FileTimes::new()
                        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(i as u64 + 1)),
                )
                .unwrap();
        }
        touch("recent");
        evict_to(
            &cache_root,
            12,
            &[Some("current".into()), Some("next".into())],
        )
        .unwrap();
        assert!(matches!(status("old"), CacheStatus::None));
        assert!(matches!(status("current"), CacheStatus::Complete { .. }));
        assert!(matches!(status("next"), CacheStatus::Complete { .. }));
        let (mut f, _, _) = open_part_writer("broken").unwrap();
        f.write_all(b"bad").unwrap();
        drop(f);
        finalize("broken", 3, None, None);
        assert!(matches!(status("broken"), CacheStatus::Partial { .. }));
        remove("broken");
        assert!(matches!(status("broken"), CacheStatus::None));
        clear().unwrap();
        assert_eq!(fs::read(local).unwrap(), b"local original");
        assert_eq!(usage().used_bytes, 0);
    }
    #[test]
    fn unknown_length_download_is_not_complete() {
        assert!(!is_complete_download(1024, None));
        assert!(!is_complete_download(99, Some(100)));
        assert!(is_complete_download(100, Some(100)));
    }
}
