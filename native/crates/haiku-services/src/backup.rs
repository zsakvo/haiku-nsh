//! Reference-compatible ZIP backups. Decode and validate everything before touching live files.
use crate::sync::Secrets;
use futures::future::BoxFuture;
use haiku_store::{
    json_file,
    playlist_core::{PlaylistCore, TrackEntry, LIKED_PLAYLIST_ID},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

type Result<T> = std::result::Result<T, String>;
const PACKED_LIMIT: u64 = 256 * 1024 * 1024;
const EXPANDED_LIMIT: u64 = 512 * 1024 * 1024;
type Files = BTreeMap<String, Vec<u8>>;

/// Service owners implement the restoration barrier; `after_restore` runs even on failure.
pub trait Hooks: Send + Sync {
    fn flush(&self) -> BoxFuture<'_, Result<()>>;
    fn before_restore(&self) -> BoxFuture<'_, Result<()>>;
    fn after_restore(&self) -> BoxFuture<'_, Result<()>>;
}
#[derive(Default)]
pub struct NoHooks;
impl Hooks for NoHooks {
    fn flush(&self) -> BoxFuture<'_, Result<()>> {
        Box::pin(async { Ok(()) })
    }
    fn before_restore(&self) -> BoxFuture<'_, Result<()>> {
        Box::pin(async { Ok(()) })
    }
    fn after_restore(&self) -> BoxFuture<'_, Result<()>> {
        Box::pin(async { Ok(()) })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub schema_version: u32,
    #[serde(default)]
    pub app_name: String,
    pub app_version: String,
    pub created_at: String,
    pub platform: String,
    pub includes_token: bool,
    pub includes: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub meta: Meta,
    pub legacy_plugins: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub safety_backup: String,
    pub legacy_plugins: Vec<String>,
    pub warnings: Vec<String>,
}
#[derive(Clone)]
struct Decoded {
    preview: Preview,
    files: Files,
}
pub struct Service {
    root: PathBuf,
    secrets: Arc<dyn Secrets>,
    hooks: Arc<dyn Hooks>,
    pending: Mutex<Option<(PathBuf, Decoded)>>,
    serial: tokio::sync::Mutex<()>,
}
impl Service {
    pub fn new(root: PathBuf, secrets: Arc<dyn Secrets>, hooks: Arc<dyn Hooks>) -> Self {
        Self {
            root,
            secrets,
            hooks,
            pending: Mutex::new(None),
            serial: Default::default(),
        }
    }
    pub async fn export(&self, path: &Path, include_token: bool) -> Result<()> {
        let _serial = self.serial.lock().await;
        self.hooks.flush().await?;
        atomic_write(path, &self.build(include_token)?)
    }
    pub fn inspect(&self, path: &Path) -> Result<Preview> {
        *self.pending.lock().unwrap() = None;
        if fs::metadata(path).map_err(err)?.len() > PACKED_LIMIT {
            return Err("备份包过大".into());
        }
        let decoded = decode(&fs::read(path).map_err(err)?)?;
        let preview = decoded.preview.clone();
        // Confirmation restores the inspected bytes, even if the selected file is replaced.
        *self.pending.lock().unwrap() = Some((path.into(), decoded));
        Ok(preview)
    }
    pub async fn import(&self, path: &Path) -> Result<Report> {
        let _serial = self.serial.lock().await;
        let decoded = self
            .pending
            .lock()
            .unwrap()
            .take()
            .filter(|(pending, _)| pending == path)
            .map(|(_, decoded)| decoded)
            .ok_or("请先预览所选备份，再确认恢复")?;
        self.hooks.flush().await?;
        let dir = self.root.join("backups");
        fs::create_dir_all(&dir).map_err(err)?;
        let safety = dir.join(format!(
            "pre-restore-{}.haikubak",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        atomic_write(&safety, &self.build(false)?)?;
        for old in self.safety_backups()?.into_iter().skip(3) {
            fs::remove_file(old).map_err(err)?;
        }
        let result = async {
            self.hooks.before_restore().await?;
            self.apply(decoded, safety)
        }
        .await;
        let reload = self.hooks.after_restore().await;
        match (result, reload) {
            (Ok(report), Ok(())) => Ok(report),
            (Err(error), Ok(())) => Err(error),
            (Ok(_), Err(error)) => Err(format!("数据已写入，但重新加载失败：{error}")),
            (Err(error), Err(reload)) => Err(format!("{error}；重新加载失败：{reload}")),
        }
    }
    pub fn safety_backups(&self) -> Result<Vec<String>> {
        let dir = self.root.join("backups");
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut paths = fs::read_dir(dir)
            .map_err(err)?
            .map(|entry| {
                let entry = entry.map_err(err)?;
                let name = entry.file_name().to_string_lossy().into_owned();
                Ok((name.starts_with("pre-restore-")
                    && name.ends_with(".haikubak")
                    && entry.file_type().map_err(err)?.is_file())
                .then(|| entry.path().to_string_lossy().into_owned()))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        paths.sort_by(|a, b| b.cmp(a));
        Ok(paths)
    }
    fn build(&self, include_token: bool) -> Result<Vec<u8>> {
        let mut files = Files::new();
        for (archive, local) in [
            ("haiku_state/settings.json", "settings.json"),
            ("haiku_state/likes.json", "likes.json"),
            ("haiku_state/playlists.loro", "playlists.loro"),
            ("haiku_state/sources.json", "plugins/sources.json"),
        ] {
            let path = self.root.join(local);
            reject_symlinks(&self.root, Path::new(local))?;
            if path.exists() {
                files.insert(archive.into(), fs::read(path).map_err(err)?);
            }
        }
        for (archive, local) in [
            ("cjrt_plugin_pkgs", "plugins/packages"),
            ("cjrt_plugins", "plugins/sources"),
        ] {
            collect(&self.root, Path::new(local), archive, &mut files)?;
        }
        let mut included = false;
        if include_token {
            if let Some(token) = self.secrets.read()? {
                files.insert(
                    "secrets.json".into(),
                    serde_json::to_vec(&json!({"github_pat":token})).map_err(err)?,
                );
                included = true;
            }
        }
        let meta = Meta {
            schema_version: 2,
            app_name: "Haiku NSH".into(),
            app_version: "1.0.0".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            platform: platform().into(),
            includes_token: included,
            includes: files.keys().cloned().collect(),
        };
        files.insert(
            "backup-meta.json".into(),
            serde_json::to_vec(&meta).map_err(err)?,
        );
        pack(&files)
    }
    fn apply(&self, decoded: Decoded, safety: PathBuf) -> Result<Report> {
        let mut files = decoded.files;
        let mut warnings = vec![];
        let mut targets = Files::new();
        for (archive, local) in [
            ("haiku_state/settings.json", "settings.json"),
            ("haiku_state/likes.json", "likes.json"),
            ("haiku_state/playlists.loro", "playlists.loro"),
        ] {
            if let Some(bytes) = files.remove(archive) {
                targets.insert(local.into(), bytes);
            }
        }
        if normalize_platform(&decoded.preview.meta.platform) != platform() {
            if let Some(bytes) = targets.get_mut("settings.json") {
                let mut settings: Value = serde_json::from_slice(bytes).map_err(err)?;
                let current: Value = json_file::read(&self.root.join("settings.json"))
                    .map_err(err)?
                    .unwrap_or(json!({}));
                settings["downloadDir"] = current.get("downloadDir").cloned().unwrap_or(json!(""));
                settings["localScanRoots"] =
                    current.get("localScanRoots").cloned().unwrap_or(json!([]));
                *bytes = serde_json::to_vec(&settings).map_err(err)?;
                warnings.push("跨平台恢复：下载目录与本地扫描目录保留本机设置。".into());
            }
        }
        let mut registry: Value = files
            .get("haiku_state/sources.json")
            .map(|b| serde_json::from_slice(b).map_err(err))
            .transpose()?
            .unwrap_or(json!({"version":1,"sources":[]}));
        let records = registry["sources"]
            .as_array_mut()
            .ok_or("来源列表格式无效")?;
        records.retain(|r| {
            !decoded
                .preview
                .legacy_plugins
                .iter()
                .any(|id| r["pluginId"] == *id)
        });
        for (name, bytes) in &files {
            let parts: Vec<_> = name.split('/').collect();
            if parts.len() < 3 {
                continue;
            }
            if parts[0] == "cjrt_plugin_pkgs"
                && !decoded.preview.legacy_plugins.contains(&parts[1].into())
            {
                targets.insert(
                    format!("plugins/packages/{}", parts[1..].join("/")),
                    bytes.clone(),
                );
            } else if parts[0] == "cjrt_plugins" {
                let plugin = parts[1].split('#').next().unwrap_or(parts[1]);
                if !decoded.preview.legacy_plugins.iter().any(|id| id == plugin)
                    && matches!(parts.get(2), Some(&"storage.json" | &"credential.json"))
                    && parts.len() == 3
                {
                    targets.insert(
                        format!("plugins/sources/{}", parts[1..].join("/")),
                        bytes.clone(),
                    );
                }
            }
        }
        // Old v1 code and sessions are never installed. Only connection fields can migrate.
        for id in &decoded.preview.legacy_plugins {
            if id != "subsonic.music" {
                continue;
            }
            let descriptor_path = self
                .root
                .join("plugins/packages/subsonic.music/descriptor.json");
            let Some(descriptor) =
                json_file::read::<haiku_plugins::Descriptor>(&descriptor_path).map_err(err)?
            else {
                continue;
            };
            let mut instances = BTreeMap::<String, BTreeMap<String, String>>::new();
            for (name, bytes) in &files {
                let parts: Vec<_> = name.split('/').collect();
                if parts.len() == 3
                    && parts[0] == "cjrt_plugins"
                    && parts[1].split('#').next() == Some(id)
                {
                    if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
                        connection_fields(&value, instances.entry(parts[1].into()).or_default(), 0);
                    }
                }
            }
            for (_, connection) in instances {
                if !connection.contains_key("baseUrl") || !connection.contains_key("username") {
                    continue;
                }
                let source = haiku_plugins::source_id(&descriptor, &connection);
                records.retain(|r| r["sourceId"] != source);
                records.push(json!({"sourceId":source,"pluginId":id,"kind":"library","enabled":true,"addedAt":0}));
                targets.insert(
                    format!("plugins/sources/{source}/credential.json"),
                    serde_json::to_vec(
                        &json!({"connection":connection,"credential":null,"user":null}),
                    )
                    .map_err(err)?,
                );
                warnings.push(format!("{id} 连接字段已迁移；请重新验证连接。"));
            }
        }
        if files.contains_key("haiku_state/sources.json") || !records.is_empty() {
            targets.insert(
                "plugins/sources.json".into(),
                serde_json::to_vec(&registry).map_err(err)?,
            );
        }
        migrate_likes(&mut targets)?;
        // Check every existing ancestor before writing the first file.
        for name in targets.keys() {
            reject_symlinks(&self.root, Path::new(name))?;
        }
        for (name, bytes) in targets {
            atomic_write(&self.root.join(name), &bytes)?;
        }
        if let Some(bytes) = files.get("secrets.json") {
            let value: Value = serde_json::from_slice(bytes).map_err(err)?;
            if let Some(token) = value["github_pat"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
            {
                if self.secrets.write(Some(token.trim())).is_err() {
                    warnings.push(
                        "数据已恢复，但 GitHub PAT 未能写入钥匙串，请重新连接 GitHub。".into(),
                    );
                }
            }
        }
        Ok(Report {
            safety_backup: safety.to_string_lossy().into(),
            legacy_plugins: decoded.preview.legacy_plugins,
            warnings,
        })
    }
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn platform() -> &'static str {
    normalize_platform(std::env::consts::OS)
}
fn normalize_platform(value: &str) -> &str {
    match value {
        "darwin" => "macos",
        "win32" => "windows",
        other => other,
    }
}
fn validate_path(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('/')
        || name.contains(['\\', ':', '\0'])
        || name
            .trim_end_matches('/')
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err("备份含非法路径".into());
    }
    let path = name.trim_end_matches('/');
    let allowed = matches!(
        path,
        "backup-meta.json" | "secrets.json" | "haiku_state" | "cjrt_plugins" | "cjrt_plugin_pkgs"
    ) || matches!(
        path,
        "haiku_state/settings.json"
            | "haiku_state/likes.json"
            | "haiku_state/playlists.loro"
            | "haiku_state/sources.json"
    ) || ["cjrt_plugins/", "cjrt_plugin_pkgs/"]
        .iter()
        .any(|p| path.starts_with(p));
    if !allowed {
        return Err("备份含白名单外条目".into());
    }
    Ok(())
}
fn decode(bytes: &[u8]) -> Result<Decoded> {
    if bytes.len() as u64 > PACKED_LIMIT {
        return Err("备份包过大".into());
    }
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|_| "无法解析备份 ZIP")?;
    if zip.len() > 50000 {
        return Err("备份条目过多".into());
    }
    let mut total = 0u64;
    let mut names = BTreeSet::new();
    // Validate the central directory and sizes before allocating any expanded content.
    for i in 0..zip.len() {
        let file = zip.by_index(i).map_err(err)?;
        validate_path(file.name())?;
        if !names.insert(file.name().to_owned()) {
            return Err("备份含重复条目".into());
        }
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("备份含符号链接".into());
        }
        total = total.checked_add(file.size()).ok_or("备份容量溢出")?;
        if total > EXPANDED_LIMIT {
            return Err("备份解压容量过大".into());
        }
    }
    let mut files = Files::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(err)?;
        if file.is_dir() {
            continue;
        }
        let mut data = vec![];
        let expected = file.size();
        (&mut file)
            .take(expected + 1)
            .read_to_end(&mut data)
            .map_err(|_| "备份文件损坏")?;
        if data.len() as u64 != expected {
            return Err("备份文件长度异常".into());
        }
        files.insert(file.name().to_owned(), data);
    }
    let meta: Meta = serde_json::from_slice(files.get("backup-meta.json").ok_or("备份缺少元信息")?)
        .map_err(err)?;
    if !(1..=2).contains(&meta.schema_version) {
        return Err("不支持的备份格式版本，请升级 Haiku".into());
    }
    if let Some(settings) = files.get("haiku_state/settings.json") {
        serde_json::from_slice::<haiku_store::settings::Settings>(settings).map_err(err)?;
    }
    if let Some(snapshot) = files.get("haiku_state/playlists.loro") {
        PlaylistCore::from_snapshot(snapshot)?;
    }
    for name in [
        "haiku_state/likes.json",
        "haiku_state/sources.json",
        "secrets.json",
    ] {
        if let Some(bytes) = files.get(name) {
            serde_json::from_slice::<Value>(bytes).map_err(err)?;
        }
    }
    let mut legacy = BTreeSet::new();
    let is_v2 = |id: &str| {
        files.get(&format!("cjrt_plugin_pkgs/{id}/descriptor.json"))
            .is_some_and(|bytes| serde_json::from_slice::<haiku_plugins::Descriptor>(bytes).is_ok())
            && files.contains_key(&format!("cjrt_plugin_pkgs/{id}/plugin.ts"))
    };
    for name in files.keys() {
        let parts: Vec<_> = name.split('/').collect();
        if parts.len() < 3 || !["cjrt_plugins", "cjrt_plugin_pkgs"].contains(&parts[0]) {
            continue;
        }
        let id = parts[1].split('#').next().unwrap();
        if !is_v2(id) {
            legacy.insert(id.to_owned());
        }
    }
    if let Some(bytes) = files.get("haiku_state/sources.json") {
        let registry: Value = serde_json::from_slice(bytes).map_err(err)?;
        for record in registry["sources"].as_array().ok_or("来源列表格式无效")? {
            let id = record["pluginId"].as_str().ok_or("来源缺少插件 ID")?;
            if !is_v2(id) { legacy.insert(id.to_owned()); }
        }
    }
    Ok(Decoded {
        preview: Preview {
            meta,
            legacy_plugins: legacy.into_iter().collect(),
        },
        files,
    })
}
fn pack(files: &Files) -> Result<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        zip.start_file(
            name,
            SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o600),
        )
        .map_err(err)?;
        zip.write_all(bytes).map_err(err)?;
    }
    Ok(zip.finish().map_err(err)?.into_inner())
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("无效文件路径")?;
    fs::create_dir_all(parent).map_err(err)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(err)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(err)?;
    }
    temp.write_all(bytes).map_err(err)?;
    temp.as_file().sync_all().map_err(err)?;
    temp.persist(path).map_err(|e| err(e.error))?;
    fs::File::open(parent).map_err(err)?.sync_all().map_err(err)
}
fn reject_symlinks(root: &Path, relative: &Path) -> Result<()> {
    let mut path = root.to_owned();
    for part in relative.components() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("备份目标或来源包含符号链接".into())
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(err(e)),
        }
    }
    Ok(())
}
fn collect(root: &Path, local: &Path, archive: &str, files: &mut Files) -> Result<()> {
    reject_symlinks(root, local)?;
    let path = root.join(local);
    if !path.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(path).map_err(err)? {
        let entry = entry.map_err(err)?;
        let name = entry
            .file_name()
            .to_str()
            .ok_or("文件名编码无效")?
            .to_owned();
        if name.starts_with('.') {
            continue;
        }
        let archive = format!("{archive}/{name}");
        validate_path(&archive)?;
        let local = local.join(name);
        if entry.file_type().map_err(err)?.is_dir() {
            collect(root, &local, &archive, files)?;
        } else {
            reject_symlinks(root, &local)?;
            files.insert(archive, fs::read(entry.path()).map_err(err)?);
        }
    }
    Ok(())
}
fn connection_fields(value: &Value, out: &mut BTreeMap<String, String>, depth: usize) {
    if depth > 8 {
        return;
    }
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            if let Some(text) = value.as_str() {
                let key = match key.as_str() {
                    "baseUrl" | "serverUrl" | "url" => Some("baseUrl"),
                    "username" | "password" => Some(key.as_str()),
                    _ => None,
                };
                if let Some(key) = key {
                    out.insert(key.into(), text.into());
                }
                if let Ok(nested) = serde_json::from_str::<Value>(text) {
                    connection_fields(&nested, out, depth + 1);
                }
            } else {
                connection_fields(value, out, depth + 1);
            }
        }
    }
}
fn migrate_likes(targets: &mut Files) -> Result<()> {
    let mut settings: Value = targets
        .get("settings.json")
        .map(|b| serde_json::from_slice(b).map_err(err))
        .transpose()?
        .unwrap_or(json!({}));
    if settings["migratedLikesToPlaylist"] == true {
        return Ok(());
    }
    let Some(bytes) = targets.get("likes.json") else {
        return Ok(());
    };
    let likes: Value = serde_json::from_slice(bytes).map_err(err)?;
    let core = targets
        .get("playlists.loro")
        .map(|b| PlaylistCore::from_snapshot(b))
        .transpose()?
        .unwrap_or_default();
    core.ensure_liked_playlist("喜欢");
    for track in likes["tracks"].as_array().into_iter().flatten() {
        let mut value = track.clone();
        value["addedAt"] = track.get("likedAt").cloned().unwrap_or(json!(0));
        if let Ok(entry) = serde_json::from_value::<TrackEntry>(value) {
            if !entry.plugin_id.is_empty() && !entry.track_id.is_empty() {
                core.add_track(LIKED_PLAYLIST_ID, &entry);
            }
        }
    }
    targets.insert("playlists.loro".into(), core.export_snapshot());
    settings["migratedLikesToPlaylist"] = json!(true);
    targets.insert(
        "settings.json".into(),
        serde_json::to_vec(&settings).map_err(err)?,
    );
    Ok(())
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
