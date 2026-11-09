//! 本地歌单权威源：Loro CRDT 文档之上的歌单语义层。
//!
//! FRB 暴露面见 [`crate::collections`]；本模块不含 FRB 类型，可独立单测。
//!
//! # 文档结构（全部用**根容器**，避免嵌套容器并发创建时的 LWW 丢失）
//!
//! ```text
//! LoroDoc
//! ├── "playlists": MovableList<String>       # 歌单 id 顺序（并发插入取并集，读侧去重）
//! ├── "playlist:<id>": Map                   # 歌单元数据（LWW）
//! │     title / description / kind / createdAt / deletedAt（墓碑，软删除）
//! └── "tracks:<id>": MovableList<String>     # 曲目（每项为 TrackEntry 的 JSON 字符串）
//! ```
//!
//! 与设计稿（docs/spark/2026-07-09-gist-playlist-sync-design.md §2）的差异：
//! `tracks` 不作为嵌套容器挂在歌单 Map 下，而是根容器 `tracks:<id>`——两台设备并发
//! "首次创建同一歌单"（尤其 `__liked__`）时，嵌套容器的创建是 Map 值的 LWW 竞争，
//! 输掉的一方容器里已写入的曲目会整体丢失；根容器按名字寻址、天然合并，无此问题。
//! 语义不变（曲目级合并、取并集、MovableList 支持拖拽排序）。
//!
//! TODO(v2): Loro 操作历史长期增长后做 shallow snapshot 裁剪（v1 明确不做，见设计稿 §11）。

use std::collections::{HashMap, HashSet};

use loro::{LoroDoc, LoroValue, ValueOrContainer};
use serde::{Deserialize, Serialize};

/// 内置「喜欢」歌单的固定 id（跨设备同名根容器天然合并）。
pub const LIKED_PLAYLIST_ID: &str = "__liked__";

/// 插件歌单收藏在本地歌单里的 kind。
pub const PLUGIN_FAVORITE_KIND: &str = "plugin";

/// 插件歌单收藏的本地 id 前缀：`favorite:<pluginId>:<playlistId>`。
const PLUGIN_FAVORITE_PREFIX: &str = "favorite:";

const ORDER_LIST: &str = "playlists";

fn meta_key(id: &str) -> String {
    format!("playlist:{id}")
}
fn tracks_key(id: &str) -> String {
    format!("tracks:{id}")
}
fn mirror_tracks_key(id: &str, version: &str) -> String {
    format!("mirror-tracks:{id}:{version}")
}
fn mirror_snapshots_key(id: &str) -> String {
    format!("mirror-snapshots:{id}")
}

fn plugin_favorite_id(plugin_id: &str, playlist_id: &str) -> String {
    format!("{PLUGIN_FAVORITE_PREFIX}{plugin_id}:{playlist_id}")
}

/// 跨插件曲目引用（字段约定与 Dart 侧 `LikedTrack` 一致）。
/// 作为**普通 JSON 值**整条写入 MovableList，不做字段级合并。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackEntry {
    #[serde(rename = "pluginId")]
    pub plugin_id: String,
    #[serde(rename = "trackId")]
    pub track_id: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artists: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// 时长（秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
    /// 加入时间（epoch 秒）。
    #[serde(rename = "addedAt", default)]
    pub added_at: i64,
}

/// 镜像歌单的一次完整远端版本。记录放在根 MovableList 中，并发写入天然取并集；
/// 读侧按 `(refreshed_at, version)` 选最大值，因此不会依赖 Map 的 actor LWW 顺序。
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PluginSnapshotRecord {
    version: String,
    #[serde(rename = "refreshedAt")]
    refreshed_at: i64,
    title: String,
    description: String,
    #[serde(rename = "sourceLabel")]
    source_label: String,
    #[serde(rename = "coverUrl")]
    cover_url: String,
}

/// 歌单元数据物化结果（供 Dart 侧写 SQLite）。
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistMeta {
    pub id: String,
    pub title: String,
    pub description: String,
    /// `user` | `liked` | `plugin`（插件歌单收藏）。
    pub kind: String,
    /// 创建时间（epoch 秒）。
    pub created_at: i64,
    /// 展示顺序（0 起，已过滤墓碑）。
    pub position: u32,
    pub track_count: u32,
    /// 插件歌单收藏元数据；普通本地歌单为 `None`。
    pub source_plugin_id: Option<String>,
    pub source_playlist_id: Option<String>,
    pub source_label: Option<String>,
    pub cover_url: Option<String>,
    /// 收藏时间（epoch 秒；普通歌单回退为创建时间）。
    pub favorited_at: i64,
    /// 最近一次成功取得完整远端快照的时间（epoch 毫秒）。
    pub refreshed_at: i64,
}

/// 歌单指纹：用于 import / reorder 前后对比，找出受影响歌单。
#[derive(PartialEq)]
struct Fingerprint {
    position: Option<usize>,
    title: String,
    description: String,
    kind: String,
    created_at: i64,
    deleted_at: i64,
    source_plugin_id: Option<String>,
    source_playlist_id: Option<String>,
    source_label: Option<String>,
    cover_url: Option<String>,
    favorited_at: i64,
    refreshed_at: i64,
    snapshot_version: Option<String>,
    tracks: Vec<String>,
}

pub struct PlaylistCore {
    doc: LoroDoc,
}

impl Default for PlaylistCore {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaylistCore {
    pub fn new() -> Self {
        PlaylistCore {
            doc: LoroDoc::new(),
        }
    }

    /// 从快照恢复；解析失败返回 Err（调用方按「空文档」处理，见设计稿 §7）。
    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, String> {
        let core = PlaylistCore::new();
        core.doc.import(bytes).map_err(|e| e.to_string())?;
        Ok(core)
    }

    // ───────────────────────────── 读 ─────────────────────────────

    /// 全部存活歌单（按顺序表去重、过滤墓碑与未初始化 id）。
    pub fn playlists(&self) -> Vec<PlaylistMeta> {
        let mut out = Vec::new();
        for id in self.visible_ids() {
            let map = self.doc.get_map(meta_key(&id).as_str());
            let kind = map_str(&map, "kind").unwrap_or_else(|| "user".to_string());
            let latest = if kind == PLUGIN_FAVORITE_KIND {
                self.latest_plugin_snapshot(&id)
            } else {
                None
            };
            let title = latest
                .as_ref()
                .map(|v| v.title.clone())
                .or_else(|| map_str(&map, "title"))
                .unwrap_or_default();
            let description = latest
                .as_ref()
                .map(|v| v.description.clone())
                .or_else(|| map_str(&map, "description"))
                .unwrap_or_default();
            let created_at = map_i64(&map, "createdAt").unwrap_or(0);
            let favorited_at = map_i64(&map, "favoritedAt").unwrap_or(created_at);
            let track_count = self.playlist_tracks(&id).len() as u32;
            out.push(PlaylistMeta {
                id,
                title,
                description,
                kind,
                created_at,
                position: out.len() as u32,
                track_count,
                source_plugin_id: map_str(&map, "sourcePluginId").filter(|v| !v.is_empty()),
                source_playlist_id: map_str(&map, "sourcePlaylistId").filter(|v| !v.is_empty()),
                source_label: latest
                    .as_ref()
                    .map(|v| v.source_label.clone())
                    .or_else(|| map_str(&map, "sourceLabel"))
                    .filter(|v| !v.is_empty()),
                cover_url: latest
                    .as_ref()
                    .map(|v| v.cover_url.clone())
                    .or_else(|| map_str(&map, "coverUrl"))
                    .filter(|v| !v.is_empty()),
                favorited_at,
                refreshed_at: latest
                    .as_ref()
                    .map(|v| v.refreshed_at)
                    .unwrap_or_else(|| map_i64(&map, "refreshedAt").unwrap_or(0)),
            });
        }
        out
    }

    /// 歌单曲目（物理顺序；按 (pluginId, trackId) 去重，保留首次出现）。
    pub fn playlist_tracks(&self, id: &str) -> Vec<TrackEntry> {
        let mut seen = HashSet::new();
        self.raw_tracks(id)
            .into_iter()
            .filter_map(|s| serde_json::from_str::<TrackEntry>(&s).ok())
            .filter(|t| seen.insert((t.plugin_id.clone(), t.track_id.clone())))
            .collect()
    }

    // ───────────────────────────── 写 ─────────────────────────────

    /// 新建用户歌单，返回新 id。
    pub fn create_playlist(&self, title: &str) -> String {
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.init_playlist(&id, title, "user");
        id
    }

    /// 确保内置「喜欢」歌单存在（幂等；置顶）。返回是否本次新建。
    pub fn ensure_liked_playlist(&self, title: &str) -> bool {
        let map = self.doc.get_map(meta_key(LIKED_PLAYLIST_ID).as_str());
        if map_i64(&map, "createdAt").is_some() {
            return false;
        }
        self.init_playlist(LIKED_PLAYLIST_ID, title, "liked");
        true
    }

    /// 收藏插件歌单：以确定性 id 落入本地歌单顺序表，并记录来源/封面/简介等
    /// 元数据。重复收藏（含删除后重新收藏）是幂等的：元数据与完整曲目快照原子换版。
    pub fn favorite_plugin_playlist(
        &self,
        plugin_id: &str,
        playlist_id: &str,
        source_label: &str,
        title: &str,
        description: &str,
        cover_url: &str,
        tracks: &[TrackEntry],
    ) -> String {
        let id = plugin_favorite_id(plugin_id, playlist_id);
        let map = self.doc.get_map(meta_key(&id).as_str());
        let latest_refresh = self
            .latest_plugin_snapshot(&id)
            .map(|snapshot| snapshot.refreshed_at)
            .unwrap_or_else(|| map_i64(&map, "refreshedAt").unwrap_or(0));
        // 连续两次本地刷新可能落在同一毫秒；时间戳相同时，快照读侧只能按随机
        // UUID 兜底排序，旧版本便有机会胜出。这里让本地刷新时间严格单调递增。
        let refreshed_at = now_millis().max(latest_refresh.saturating_add(1));
        self.write_plugin_playlist_snapshot(
            plugin_id,
            playlist_id,
            source_label,
            title,
            description,
            cover_url,
            tracks,
            refreshed_at,
        )
    }

    fn write_plugin_playlist_snapshot(
        &self,
        plugin_id: &str,
        playlist_id: &str,
        source_label: &str,
        title: &str,
        description: &str,
        cover_url: &str,
        tracks: &[TrackEntry],
        refreshed_at: i64,
    ) -> String {
        let id = plugin_favorite_id(plugin_id, playlist_id);
        let map = self.doc.get_map(meta_key(&id).as_str());
        let now = now_secs();
        // 网络迟到的旧请求不得覆盖已成功写入的新快照。
        let latest_refresh = self
            .latest_plugin_snapshot(&id)
            .map(|v| v.refreshed_at)
            .unwrap_or_else(|| map_i64(&map, "refreshedAt").unwrap_or(0));
        if latest_refresh > refreshed_at {
            return id;
        }
        let is_new = map_i64(&map, "createdAt").is_none();
        let was_deleted = map_i64(&map, "deletedAt").unwrap_or(0) > 0;
        if is_new {
            let _ = map.insert("kind", PLUGIN_FAVORITE_KIND);
            let _ = map.insert("createdAt", now);
        }
        if was_deleted {
            // 取消收藏后又收藏：清墓碑而不是新开一条，避免远端看到重复收藏。
            let _ = map.insert("deletedAt", 0);
        }
        let _ = map.insert("title", title);
        let _ = map.insert("description", description);
        let _ = map.insert("sourcePluginId", plugin_id);
        let _ = map.insert("sourcePlaylistId", playlist_id);
        let _ = map.insert("sourceLabel", source_label);
        let _ = map.insert("coverUrl", cover_url);
        if is_new || was_deleted {
            let _ = map.insert("favoritedAt", now);
        }
        let version = format!("{refreshed_at}-{}", uuid::Uuid::new_v4().simple());
        self.write_track_snapshot(&id, &version, tracks);
        let record = PluginSnapshotRecord {
            version: version.clone(),
            refreshed_at,
            title: title.to_string(),
            description: description.to_string(),
            source_label: source_label.to_string(),
            cover_url: cover_url.to_string(),
        };
        let snapshots = self
            .doc
            .get_movable_list(mirror_snapshots_key(&id).as_str());
        let json = serde_json::to_string(&record).unwrap_or_default();
        let _ = snapshots.insert(snapshots.len(), json.as_str());
        let _ = map.insert("snapshotVersion", version);
        let _ = map.insert("refreshedAt", refreshed_at);
        self.ensure_in_order(&id);
        self.doc.commit();
        id
    }

    fn init_playlist(&self, id: &str, title: &str, kind: &str) {
        let map = self.doc.get_map(meta_key(id).as_str());
        let _ = map.insert("title", title);
        let _ = map.insert("description", "");
        let _ = map.insert("kind", kind);
        let _ = map.insert("createdAt", now_secs());
        let order = self.doc.get_movable_list(ORDER_LIST);
        let pos = if kind == "liked" { 0 } else { order.len() };
        let _ = order.insert(pos.min(order.len()), id);
        self.doc.commit();
    }

    /// 改名 / 改描述。返回受影响歌单 id。
    pub fn rename_playlist(&self, id: &str, title: &str, description: &str) -> Vec<String> {
        let map = self.doc.get_map(meta_key(id).as_str());
        let _ = map.insert("title", title);
        let _ = map.insert("description", description);
        self.doc.commit();
        vec![id.to_string()]
    }

    /// 软删除（墓碑 `deletedAt`），不物理清除，避免与并发改名互相丢数据。
    pub fn delete_playlist(&self, id: &str) -> Vec<String> {
        let map = self.doc.get_map(meta_key(id).as_str());
        let _ = map.insert("deletedAt", now_secs());
        self.doc.commit();
        vec![id.to_string()]
    }

    /// 加曲（已存在同 (pluginId, trackId) 则 no-op）。返回受影响歌单 id。
    pub fn add_track(&self, playlist_id: &str, track: &TrackEntry) -> Vec<String> {
        let exists = self
            .playlist_tracks(playlist_id)
            .iter()
            .any(|t| t.plugin_id == track.plugin_id && t.track_id == track.track_id);
        if exists {
            return Vec::new();
        }
        let list = self.doc.get_movable_list(tracks_key(playlist_id).as_str());
        let json = serde_json::to_string(track).unwrap_or_default();
        let _ = list.insert(list.len(), json.as_str());
        self.doc.commit();
        vec![playlist_id.to_string()]
    }

    /// 删曲：移除全部匹配 (pluginId, trackId) 的条目（并发重复也一并清理）。
    pub fn remove_track(&self, playlist_id: &str, plugin_id: &str, track_id: &str) -> Vec<String> {
        let list = self.doc.get_movable_list(tracks_key(playlist_id).as_str());
        let raw = self.raw_tracks(playlist_id);
        let mut removed = false;
        for i in (0..raw.len()).rev() {
            if let Ok(t) = serde_json::from_str::<TrackEntry>(&raw[i]) {
                if t.plugin_id == plugin_id && t.track_id == track_id {
                    let _ = list.delete(i, 1);
                    removed = true;
                }
            }
        }
        if !removed {
            return Vec::new();
        }
        self.doc.commit();
        vec![playlist_id.to_string()]
    }

    /// 拖拽排序：把**去重后可见列表**里 `from` 位置的曲目移到 `to`。
    pub fn move_track(&self, playlist_id: &str, from: usize, to: usize) -> Vec<String> {
        let visible = self.visible_track_indices(playlist_id);
        if from >= visible.len() || to >= visible.len() || from == to {
            return Vec::new();
        }
        let list = self.doc.get_movable_list(tracks_key(playlist_id).as_str());
        if list.mov(visible[from], visible[to]).is_err() {
            return Vec::new();
        }
        self.doc.commit();
        vec![playlist_id.to_string()]
    }

    /// 歌单整体重排：物理重写顺序表为给定 id 序列（并发新建的歌单因是并发插入，
    /// 合并后仍保留）。返回位置发生变化的歌单 id。
    pub fn reorder_playlists(&self, order: &[String]) -> Vec<String> {
        let before: HashMap<String, usize> = self
            .visible_ids()
            .into_iter()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect();

        let list = self.doc.get_movable_list(ORDER_LIST);
        let len = list.len();
        if len > 0 {
            let _ = list.delete(0, len);
        }
        // 只接受当前存在的 id；漏掉的存活歌单补到末尾（防御调用方给的不全）。
        let mut written = HashSet::new();
        let mut pos = 0usize;
        for id in order {
            if before.contains_key(id) && written.insert(id.clone()) {
                let _ = list.insert(pos, id.as_str());
                pos += 1;
            }
        }
        let mut missing: Vec<&String> = before.keys().filter(|id| !written.contains(*id)).collect();
        missing.sort_by_key(|id| before[*id]);
        for id in missing {
            let _ = list.insert(pos, id.as_str());
            pos += 1;
        }
        self.doc.commit();

        self.visible_ids()
            .into_iter()
            .enumerate()
            .filter(|(i, id)| before.get(id) != Some(i))
            .map(|(_, id)| id)
            .collect()
    }

    // ───────────────────────────── 同步 ─────────────────────────────

    /// 导出全量快照（含历史，供 import 合并）。
    pub fn export_snapshot(&self) -> Vec<u8> {
        self.doc
            .export(loro::ExportMode::Snapshot)
            .unwrap_or_default()
    }

    /// 合并远端快照，返回本次实际变化的歌单 id（含新增/删除/内容/位置变化）。
    /// Loro import 幂等且可交换：重复导入返回空列表。
    pub fn import(&self, bytes: &[u8]) -> Result<Vec<String>, String> {
        let before = self.fingerprints();
        self.doc.import(bytes).map_err(|e| e.to_string())?;
        self.doc.commit();
        let after = self.fingerprints();

        let mut ids: HashSet<String> = HashSet::new();
        for (id, fp) in &after {
            if before.get(id) != Some(fp) {
                ids.insert(id.clone());
            }
        }
        for id in before.keys() {
            if !after.contains_key(id) {
                ids.insert(id.clone());
            }
        }
        let mut out: Vec<String> = ids.into_iter().collect();
        out.sort();
        Ok(out)
    }

    // ───────────────────────────── 内部 ─────────────────────────────

    /// 若 id 尚不在顺序表里则追加到末尾（不提交）。
    fn ensure_in_order(&self, id: &str) {
        let list = self.doc.get_movable_list(ORDER_LIST);
        for i in 0..list.len() {
            if let Some(ValueOrContainer::Value(LoroValue::String(s))) = list.get(i) {
                if s.to_string() == id {
                    return;
                }
            }
        }
        let _ = list.insert(list.len(), id);
    }

    /// 写入一个不可变、按版本命名的完整镜像曲目列表（不提交）。元数据只需切换
    /// `snapshotVersion`，读侧便不会把不同设备的并发快照合并成一张歌单。
    fn write_track_snapshot(&self, id: &str, version: &str, tracks: &[TrackEntry]) {
        let list = self
            .doc
            .get_movable_list(mirror_tracks_key(id, version).as_str());
        let mut seen = HashSet::new();
        for track in tracks {
            if !seen.insert((track.plugin_id.clone(), track.track_id.clone())) {
                continue;
            }
            let json = serde_json::to_string(track).unwrap_or_default();
            let _ = list.insert(list.len(), json.as_str());
        }
    }

    /// 顺序表 id：去重（并发插入并集）+ 过滤墓碑 / 未初始化。
    fn visible_ids(&self) -> Vec<String> {
        let list = self.doc.get_movable_list(ORDER_LIST);
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for i in 0..list.len() {
            let Some(ValueOrContainer::Value(LoroValue::String(s))) = list.get(i) else {
                continue;
            };
            let id = s.to_string();
            if !seen.insert(id.clone()) {
                continue;
            }
            let map = self.doc.get_map(meta_key(&id).as_str());
            if map_i64(&map, "createdAt").is_none() {
                continue; // 未初始化（异常数据）
            }
            if map_i64(&map, "deletedAt").unwrap_or(0) > 0 {
                continue; // 墓碑
            }
            out.push(id);
        }
        out
    }

    /// 曲目列表原始 JSON 串（物理顺序，不去重）。
    fn raw_tracks(&self, id: &str) -> Vec<String> {
        let map = self.doc.get_map(meta_key(id).as_str());
        let key = self
            .latest_plugin_snapshot(id)
            .map(|v| v.version)
            .or_else(|| map_str(&map, "snapshotVersion"))
            .map(|v| mirror_tracks_key(id, &v))
            .unwrap_or_else(|| tracks_key(id));
        let list = self.doc.get_movable_list(key.as_str());
        let mut out = Vec::new();
        for i in 0..list.len() {
            if let Some(ValueOrContainer::Value(LoroValue::String(s))) = list.get(i) {
                out.push(s.to_string());
            }
        }
        out
    }

    fn latest_plugin_snapshot(&self, id: &str) -> Option<PluginSnapshotRecord> {
        let list = self.doc.get_movable_list(mirror_snapshots_key(id).as_str());
        let mut latest: Option<PluginSnapshotRecord> = None;
        for i in 0..list.len() {
            let Some(ValueOrContainer::Value(LoroValue::String(raw))) = list.get(i) else {
                continue;
            };
            let Ok(candidate) = serde_json::from_str::<PluginSnapshotRecord>(&raw) else {
                continue;
            };
            let replace = latest.as_ref().is_none_or(|current| {
                (candidate.refreshed_at, candidate.version.as_str())
                    > (current.refreshed_at, current.version.as_str())
            });
            if replace {
                latest = Some(candidate);
            }
        }
        latest
    }

    /// 去重后可见曲目 → 物理下标映射。
    fn visible_track_indices(&self, id: &str) -> Vec<usize> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for (i, s) in self.raw_tracks(id).iter().enumerate() {
            let Ok(t) = serde_json::from_str::<TrackEntry>(s) else {
                continue;
            };
            if seen.insert((t.plugin_id, t.track_id)) {
                out.push(i);
            }
        }
        out
    }

    /// 全部（含墓碑）歌单指纹，import 前后 diff 用。
    fn fingerprints(&self) -> HashMap<String, Fingerprint> {
        let list = self.doc.get_movable_list(ORDER_LIST);
        let mut all_ids = Vec::new();
        let mut seen = HashSet::new();
        for i in 0..list.len() {
            if let Some(ValueOrContainer::Value(LoroValue::String(s))) = list.get(i) {
                let id = s.to_string();
                if seen.insert(id.clone()) {
                    all_ids.push(id);
                }
            }
        }
        let positions: HashMap<String, usize> = self
            .visible_ids()
            .into_iter()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect();
        let mut out = HashMap::new();
        for id in all_ids {
            let map = self.doc.get_map(meta_key(&id).as_str());
            out.insert(
                id.clone(),
                Fingerprint {
                    position: positions.get(&id).copied(),
                    title: map_str(&map, "title").unwrap_or_default(),
                    description: map_str(&map, "description").unwrap_or_default(),
                    kind: map_str(&map, "kind").unwrap_or_default(),
                    created_at: map_i64(&map, "createdAt").unwrap_or(0),
                    deleted_at: map_i64(&map, "deletedAt").unwrap_or(0),
                    source_plugin_id: map_str(&map, "sourcePluginId").filter(|v| !v.is_empty()),
                    source_playlist_id: map_str(&map, "sourcePlaylistId").filter(|v| !v.is_empty()),
                    source_label: map_str(&map, "sourceLabel").filter(|v| !v.is_empty()),
                    cover_url: map_str(&map, "coverUrl").filter(|v| !v.is_empty()),
                    favorited_at: map_i64(&map, "favoritedAt").unwrap_or(0),
                    refreshed_at: map_i64(&map, "refreshedAt").unwrap_or(0),
                    snapshot_version: map_str(&map, "snapshotVersion"),
                    tracks: self.raw_tracks(&id),
                },
            );
        }
        out
    }
}

fn map_str(map: &loro::LoroMap, key: &str) -> Option<String> {
    match map.get(key)? {
        ValueOrContainer::Value(LoroValue::String(s)) => Some(s.to_string()),
        _ => None,
    }
}

fn map_i64(map: &loro::LoroMap, key: &str) -> Option<i64> {
    match map.get(key)? {
        ValueOrContainer::Value(LoroValue::I64(v)) => Some(v),
        _ => None,
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ═══════════════════════════════ 单测 ═══════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn track(n: u32) -> TrackEntry {
        TrackEntry {
            plugin_id: "netease".into(),
            track_id: format!("t{n}"),
            provider: "netease".into(),
            title: format!("歌 {n}"),
            artists: vec!["歌手A".into()],
            album: Some("专辑".into()),
            cover: None,
            duration: Some(200),
            added_at: 1000 + n as i64,
        }
    }

    fn track_ids(core: &PlaylistCore, pl: &str) -> Vec<String> {
        core.playlist_tracks(pl)
            .into_iter()
            .map(|t| t.track_id)
            .collect()
    }

    /// 互相导入对方快照（一轮交叉同步）。
    fn cross_sync(a: &PlaylistCore, b: &PlaylistCore) {
        a.import(&b.export_snapshot()).unwrap();
        b.import(&a.export_snapshot()).unwrap();
        // 再回灌一次，确保双方看到完全一致的操作集。
        a.import(&b.export_snapshot()).unwrap();
    }

    #[test]
    fn crud_basics() {
        let core = PlaylistCore::new();
        let id = core.create_playlist("我的歌单");
        assert_eq!(core.playlists().len(), 1);
        assert_eq!(core.playlists()[0].title, "我的歌单");
        assert_eq!(core.playlists()[0].kind, "user");

        // 加曲 / 重复加曲 no-op
        assert_eq!(core.add_track(&id, &track(1)), vec![id.clone()]);
        assert_eq!(core.add_track(&id, &track(2)), vec![id.clone()]);
        assert!(core.add_track(&id, &track(1)).is_empty());
        assert_eq!(track_ids(&core, &id), vec!["t1", "t2"]);
        assert_eq!(core.playlists()[0].track_count, 2);

        // 改名
        core.rename_playlist(&id, "新名字", "描述");
        assert_eq!(core.playlists()[0].title, "新名字");
        assert_eq!(core.playlists()[0].description, "描述");

        // 移动
        core.add_track(&id, &track(3));
        core.move_track(&id, 2, 0);
        assert_eq!(track_ids(&core, &id), vec!["t3", "t1", "t2"]);

        // 删曲
        core.remove_track(&id, "netease", "t1");
        assert_eq!(track_ids(&core, &id), vec!["t3", "t2"]);
        assert!(core.remove_track(&id, "netease", "t1").is_empty());

        // 删歌单（墓碑）
        core.delete_playlist(&id);
        assert!(core.playlists().is_empty());
    }

    #[test]
    fn liked_playlist_fixed_id_and_pinned() {
        let core = PlaylistCore::new();
        core.create_playlist("普通");
        assert!(core.ensure_liked_playlist("喜欢"));
        assert!(!core.ensure_liked_playlist("喜欢")); // 幂等
        let pls = core.playlists();
        assert_eq!(pls[0].id, LIKED_PLAYLIST_ID);
        assert_eq!(pls[0].kind, "liked");
        assert_eq!(pls.len(), 2);
    }

    #[test]
    fn favorite_plugin_playlist_metadata_and_snapshot_replacement() {
        let core = PlaylistCore::new();
        let id = core.favorite_plugin_playlist(
            "netease.music",
            "pl:10086",
            "网易云音乐",
            "深夜电台",
            "适合凌晨听的歌",
            "https://cdn/cover.jpg",
            &[track(1), track(2)],
        );
        assert_eq!(id, "favorite:netease.music:pl:10086");

        let pl = core.playlists().into_iter().find(|p| p.id == id).unwrap();
        assert_eq!(pl.kind, PLUGIN_FAVORITE_KIND);
        assert_eq!(pl.title, "深夜电台");
        assert_eq!(pl.description, "适合凌晨听的歌");
        assert_eq!(pl.source_plugin_id.as_deref(), Some("netease.music"));
        assert_eq!(pl.source_playlist_id.as_deref(), Some("pl:10086"));
        assert_eq!(pl.source_label.as_deref(), Some("网易云音乐"));
        assert_eq!(pl.cover_url.as_deref(), Some("https://cdn/cover.jpg"));
        assert!(pl.favorited_at > 0);
        assert_eq!(pl.track_count, 2);
        assert_eq!(track_ids(&core, &id), vec!["t1", "t2"]);

        // 重复收藏：元数据更新、完整快照替换，不保留远端已删除曲目。
        core.favorite_plugin_playlist(
            "netease.music",
            "pl:10086",
            "网易云音乐",
            "深夜电台（更新）",
            "新的简介",
            "https://cdn/cover2.jpg",
            &[track(2), track(3)],
        );
        let pls = core.playlists();
        assert_eq!(pls.len(), 1);
        let pl = pls.into_iter().find(|p| p.id == id).unwrap();
        assert_eq!(pl.title, "深夜电台（更新）");
        assert_eq!(pl.cover_url.as_deref(), Some("https://cdn/cover2.jpg"));
        assert!(pl.refreshed_at > 0);
        assert_eq!(pl.track_count, 2);
        assert_eq!(track_ids(&core, &id), vec!["t2", "t3"]);
    }

    #[test]
    fn favorite_plugin_playlist_refavorite_after_delete() {
        let core = PlaylistCore::new();
        let id =
            core.favorite_plugin_playlist("qq", "pl:1", "QQ音乐", "流行榜", "", "", &[track(1)]);
        core.delete_playlist(&id);
        assert!(core.playlists().iter().all(|p| p.id != id));

        let id2 = core.favorite_plugin_playlist(
            "qq",
            "pl:1",
            "QQ音乐",
            "流行榜",
            "简介",
            "https://cover",
            &[track(1), track(2)],
        );
        assert_eq!(id2, id, "删除后重新收藏沿用同一个本地 id");
        let pl = core.playlists().into_iter().find(|p| p.id == id).unwrap();
        assert_eq!(pl.description, "简介");
        assert_eq!(pl.track_count, 2);
    }

    #[test]
    fn favorite_plugin_playlist_refresh_is_monotonic_past_clock_skew() {
        let core = PlaylistCore::new();
        let future_refresh = now_millis().saturating_add(10_000);
        let id = core.write_plugin_playlist_snapshot(
            "netease.music",
            "pl:clock-skew",
            "网易云音乐",
            "旧标题",
            "旧简介",
            "",
            &[track(1)],
            future_refresh,
        );

        core.favorite_plugin_playlist(
            "netease.music",
            "pl:clock-skew",
            "网易云音乐",
            "新标题",
            "新简介",
            "",
            &[track(2)],
        );

        let pl = core.playlists().into_iter().find(|p| p.id == id).unwrap();
        assert_eq!(pl.title, "新标题");
        assert_eq!(pl.description, "新简介");
        assert!(pl.refreshed_at > future_refresh);
        assert_eq!(track_ids(&core, &id), vec!["t2"]);
    }

    #[test]
    fn newer_plugin_snapshot_wins_after_sync() {
        let a = PlaylistCore::new();
        let b = PlaylistCore::new();
        a.write_plugin_playlist_snapshot(
            "netease.music",
            "pl:1",
            "网易云音乐",
            "收藏歌单",
            "",
            "",
            &[track(1)],
            100,
        );
        b.write_plugin_playlist_snapshot(
            "netease.music",
            "pl:1",
            "网易云音乐",
            "收藏歌单",
            "",
            "",
            &[track(2)],
            200,
        );

        cross_sync(&a, &b);

        let fav: Vec<_> = a
            .playlists()
            .into_iter()
            .filter(|p| p.kind == PLUGIN_FAVORITE_KIND)
            .collect();
        assert_eq!(fav.len(), 1, "并发首藏同一歌单只出现一次");
        let ids = track_ids(&a, &fav[0].id);
        assert_eq!(ids, vec!["t2"], "较新的完整快照获胜，不合并曲目");
        assert_eq!(track_ids(&a, &fav[0].id), track_ids(&b, &fav[0].id));
    }

    #[test]
    fn stale_plugin_snapshot_does_not_overwrite_newer() {
        let core = PlaylistCore::new();
        core.write_plugin_playlist_snapshot(
            "qq",
            "pl:1",
            "QQ音乐",
            "新版",
            "",
            "",
            &[track(2)],
            200,
        );
        core.write_plugin_playlist_snapshot(
            "qq",
            "pl:1",
            "QQ音乐",
            "旧版",
            "",
            "",
            &[track(1)],
            100,
        );
        let pl = core.playlists().into_iter().next().unwrap();
        assert_eq!(pl.title, "新版");
        assert_eq!(pl.refreshed_at, 200);
        assert_eq!(track_ids(&core, &pl.id), vec!["t2"]);
    }

    #[test]
    fn reorder_playlists_reports_changed() {
        let core = PlaylistCore::new();
        let a = core.create_playlist("A");
        let b = core.create_playlist("B");
        let c = core.create_playlist("C");
        let changed = core.reorder_playlists(&[c.clone(), a.clone(), b.clone()]);
        assert_eq!(
            core.playlists()
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<_>>(),
            vec![c.clone(), a.clone(), b.clone()]
        );
        // 三个位置都变了
        assert_eq!(changed.len(), 3);
        // 再排一次同序 → 无变化
        assert!(core.reorder_playlists(&[c, a, b]).is_empty());
    }

    /// A 加曲同时 B 删另一曲：互相 import 后收敛一致且两个改动都保留。
    #[test]
    fn concurrent_add_and_remove_converge() {
        let a = PlaylistCore::new();
        let pl = a.create_playlist("共享");
        a.add_track(&pl, &track(1));
        a.add_track(&pl, &track(2));

        let b = PlaylistCore::from_snapshot(&a.export_snapshot()).unwrap();

        // 离线分叉：A 加 t3，B 删 t1。
        a.add_track(&pl, &track(3));
        b.remove_track(&pl, "netease", "t1");

        cross_sync(&a, &b);

        let ids_a = track_ids(&a, &pl);
        let ids_b = track_ids(&b, &pl);
        assert_eq!(ids_a, ids_b, "双方必须收敛到同一状态");
        assert!(ids_a.contains(&"t3".to_string()), "A 的加曲保留");
        assert!(!ids_a.contains(&"t1".to_string()), "B 的删曲保留");
        assert!(ids_a.contains(&"t2".to_string()));
    }

    /// 两边并发重排同一歌单：收敛一致（曲目集合不变）。
    #[test]
    fn concurrent_reorder_same_playlist_converges() {
        let a = PlaylistCore::new();
        let pl = a.create_playlist("重排");
        for n in 1..=4 {
            a.add_track(&pl, &track(n));
        }
        let b = PlaylistCore::from_snapshot(&a.export_snapshot()).unwrap();

        // A: t4 移到最前；B: t1 移到最后。
        a.move_track(&pl, 3, 0);
        b.move_track(&pl, 0, 3);

        cross_sync(&a, &b);

        let ids_a = track_ids(&a, &pl);
        let ids_b = track_ids(&b, &pl);
        assert_eq!(ids_a, ids_b, "并发重排必须收敛到同一顺序");
        let mut sorted = ids_a.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["t1", "t2", "t3", "t4"], "重排不得增删曲目");
    }

    /// 并发改名 vs 删除：收敛一致（墓碑保留，删除生效）。
    #[test]
    fn concurrent_rename_vs_delete_converge() {
        let a = PlaylistCore::new();
        let pl = a.create_playlist("原名");
        let b = PlaylistCore::from_snapshot(&a.export_snapshot()).unwrap();

        a.rename_playlist(&pl, "A 改的名", "");
        b.delete_playlist(&pl);

        cross_sync(&a, &b);

        assert!(a.playlists().is_empty(), "删除（墓碑）生效");
        assert!(b.playlists().is_empty());
    }

    /// 两台设备并发首建「喜欢」歌单并各自点赞：合并后取并集且列表只出现一次。
    #[test]
    fn concurrent_liked_bootstrap_union() {
        let a = PlaylistCore::new();
        let b = PlaylistCore::new();
        a.ensure_liked_playlist("喜欢");
        b.ensure_liked_playlist("喜欢");
        a.add_track(LIKED_PLAYLIST_ID, &track(1));
        b.add_track(LIKED_PLAYLIST_ID, &track(2));

        cross_sync(&a, &b);

        let liked: Vec<_> = a
            .playlists()
            .into_iter()
            .filter(|p| p.id == LIKED_PLAYLIST_ID)
            .collect();
        assert_eq!(liked.len(), 1, "顺序表去重后喜欢歌单只出现一次");
        let mut ids = track_ids(&a, LIKED_PLAYLIST_ID);
        ids.sort();
        assert_eq!(ids, vec!["t1", "t2"], "两端点赞取并集");
        assert_eq!(
            track_ids(&a, LIKED_PLAYLIST_ID),
            track_ids(&b, LIKED_PLAYLIST_ID)
        );
    }

    /// import 幂等：重复导入同一快照，第二次返回空且状态不变。
    #[test]
    fn import_is_idempotent() {
        let a = PlaylistCore::new();
        let pl = a.create_playlist("幂等");
        a.add_track(&pl, &track(1));
        let snapshot = a.export_snapshot();

        let b = PlaylistCore::new();
        let first = b.import(&snapshot).unwrap();
        assert!(!first.is_empty(), "首次导入报告受影响歌单");
        assert!(first.contains(&pl));

        let before = track_ids(&b, &pl);
        let second = b.import(&snapshot).unwrap();
        assert!(second.is_empty(), "重复导入无受影响歌单");
        assert_eq!(track_ids(&b, &pl), before);
    }

    /// import 返回的受影响 id 精确：只动了一个歌单时不误报其他歌单。
    #[test]
    fn import_reports_only_affected() {
        let a = PlaylistCore::new();
        let p1 = a.create_playlist("一");
        let p2 = a.create_playlist("二");
        let b = PlaylistCore::from_snapshot(&a.export_snapshot()).unwrap();

        a.add_track(&p1, &track(1));
        let affected = b.import(&a.export_snapshot()).unwrap();
        assert_eq!(affected, vec![p1.clone()]);
        let _ = p2;
    }

    /// 损坏快照：from_snapshot 报错（上层按空文档处理）。
    #[test]
    fn corrupt_snapshot_errors() {
        assert!(PlaylistCore::from_snapshot(b"not a loro snapshot").is_err());
    }
}
