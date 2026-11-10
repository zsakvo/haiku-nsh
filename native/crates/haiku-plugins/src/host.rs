use crate::{Descriptor, LoginDraft, PluginError, Result, Source, descriptor, source};
use haiku_store::json_file;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRecord {
    pub source_id: String,
    pub plugin_id: String,
    pub kind: String,
    pub display_name: Option<String>,
    pub added_at: u64,
    pub enabled: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Default, Clone, Serialize, Deserialize)]
struct Registry {
    #[serde(default = "version")]
    version: u32,
    #[serde(default)]
    sources: Vec<SourceRecord>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}
fn version() -> u32 {
    1
}
#[derive(Clone, Debug)]
pub struct Package {
    pub descriptor: Descriptor,
    pub path: PathBuf,
}
pub struct Host {
    pub(crate) root: PathBuf,
    pub(crate) sources: Mutex<BTreeMap<String, Arc<Source>>>,
    registry: Mutex<Registry>,
    transaction: tokio::sync::Mutex<()>,
}
pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub(crate) fn valid_path(name: &str) -> Result<()> {
    if name.is_empty()
        || name.contains(['\\', ':'])
        || Path::new(name).is_absolute()
        || Path::new(name)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(PluginError::new("INVALID_PACKAGE", "Unsafe package path"));
    }
    Ok(())
}
impl Host {
    /// Open an independent data root, installing the embedded builtin on first use.
    pub async fn open(data: &Path) -> Result<Arc<Self>> {
        static OPEN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        static HOSTS: std::sync::OnceLock<Mutex<BTreeMap<PathBuf, std::sync::Weak<Host>>>> =
            std::sync::OnceLock::new();
        let _opening = OPEN.lock().await;
        std::fs::create_dir_all(data)?;
        let data = data.canonicalize()?;
        let hosts = HOSTS.get_or_init(Mutex::default);
        if let Some(host) = hosts
            .lock()
            .unwrap()
            .get(&data)
            .and_then(std::sync::Weak::upgrade)
        {
            return Ok(host);
        }
        let root = data.join("plugins");
        std::fs::create_dir_all(root.join("packages"))?;
        std::fs::create_dir_all(root.join("sources"))?;
        let mut registry: Registry =
            json_file::read(&root.join("sources.json"))?.unwrap_or_default();
        if registry.version == 0 {
            registry.version = 1;
        }
        let host = Arc::new(Self {
            root,
            sources: Mutex::new(BTreeMap::new()),
            registry: Mutex::new(registry),
            transaction: tokio::sync::Mutex::new(()),
        });
        if !host
            .root
            .join("packages/subsonic.music/descriptor.json")
            .exists()
        {
            let files = BTreeMap::from([(
                "plugin.ts".into(),
                include_bytes!("../../../../plugins/subsonic/plugin.ts").to_vec(),
            )]);
            host.install_files(files, true).await?;
        }
        let records = host.records();
        for record in records {
            valid_path(&record.source_id)?;
            if record.enabled {
                let d = host.package(&record.plugin_id)?.descriptor;
                let instance = Source::open(&host.root, &record, d).await?;
                host.sources
                    .lock()
                    .unwrap()
                    .insert(record.source_id, Arc::new(instance));
            }
        }
        hosts.lock().unwrap().insert(data, Arc::downgrade(&host));
        Ok(host)
    }
    pub fn packages(&self) -> Result<Vec<Package>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(self.root.join("packages"))? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with('.') || !entry.file_type()?.is_dir()
            {
                continue;
            }
            let descriptor: Descriptor = json_file::read(&entry.path().join("descriptor.json"))?
                .ok_or_else(|| PluginError::new("INVALID_PLUGIN", "Missing descriptor"))?;
            out.push(Package {
                descriptor,
                path: entry.path(),
            });
        }
        Ok(out)
    }
    pub fn package(&self, id: &str) -> Result<Package> {
        valid_path(id)?;
        let path = self.root.join("packages").join(id);
        let descriptor = json_file::read(&path.join("descriptor.json"))?
            .ok_or_else(|| PluginError::new("NOT_FOUND", "Package not installed"))?;
        Ok(Package { descriptor, path })
    }
    pub fn records(&self) -> Vec<SourceRecord> {
        self.registry.lock().unwrap().sources.clone()
    }
    pub fn source(&self, id: &str) -> Result<Arc<Source>> {
        self.sources
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| PluginError::new("PLUGIN_UNAVAILABLE", "Source disabled or missing"))
    }
    fn save(&self, next: Registry) -> Result<()> {
        json_file::write(&self.root.join("sources.json"), &next)?;
        *self.registry.lock().unwrap() = next;
        Ok(())
    }
    pub async fn install_directory(&self, path: &Path, builtin: bool) -> Result<Descriptor> {
        fn collect(
            root: &Path,
            dir: &Path,
            out: &mut BTreeMap<String, Vec<u8>>,
            total: &mut usize,
        ) -> Result<()> {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let ty = entry.file_type()?;
                if ty.is_symlink() {
                    return Err(PluginError::new(
                        "INVALID_PACKAGE",
                        "Symlinks are forbidden",
                    ));
                }
                if ty.is_dir() {
                    collect(root, &entry.path(), out, total)?;
                } else if ty.is_file() {
                    *total += entry.metadata()?.len() as usize;
                    if *total > 50 * 1024 * 1024 {
                        return Err(PluginError::new(
                            "PACKAGE_TOO_LARGE",
                            "Expanded package exceeds 50MiB",
                        ));
                    }
                    out.insert(
                        entry
                            .path()
                            .strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                        std::fs::read(entry.path())?,
                    );
                }
            }
            Ok(())
        }
        let mut files = BTreeMap::new();
        collect(path, path, &mut files, &mut 0)?;
        self.install_files(files, builtin).await
    }
    pub async fn install_zip(&self, bytes: &[u8]) -> Result<Descriptor> {
        if bytes.len() > 10 * 1024 * 1024 {
            return Err(PluginError::new("PACKAGE_TOO_LARGE", "ZIP exceeds 10MiB"));
        }
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| PluginError::new("INVALID_PACKAGE", e.to_string()))?;
        let mut files = BTreeMap::new();
        let mut total = 0u64;
        for index in 0..archive.len() {
            let mut file = archive
                .by_index(index)
                .map_err(|e| PluginError::new("INVALID_PACKAGE", e.to_string()))?;
            let name = file.name().trim_end_matches('/').to_owned();
            valid_path(&name)?;
            if file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
                return Err(PluginError::new("INVALID_PACKAGE", "ZIP symlink"));
            }
            if file.is_dir() {
                continue;
            }
            total = total
                .checked_add(file.size())
                .ok_or_else(|| PluginError::new("PACKAGE_TOO_LARGE", "ZIP size overflow"))?;
            if total > 50 * 1024 * 1024 {
                return Err(PluginError::new(
                    "PACKAGE_TOO_LARGE",
                    "Expanded ZIP exceeds 50MiB",
                ));
            }
            let mut data = Vec::new();
            file.by_ref()
                .take(50 * 1024 * 1024 + 1)
                .read_to_end(&mut data)?;
            if data.len() as u64 != file.size() {
                return Err(PluginError::new("INVALID_PACKAGE", "Invalid expanded size"));
            }
            if files.insert(name, data).is_some() {
                return Err(PluginError::new("INVALID_PACKAGE", "Duplicate entry"));
            }
        }
        self.install_files(files, false).await
    }
    pub async fn install_files(
        &self,
        files: BTreeMap<String, Vec<u8>>,
        builtin: bool,
    ) -> Result<Descriptor> {
        let _transaction = self.transaction.lock().await;
        let mut total = 0usize;
        for (name, data) in &files {
            valid_path(name)?;
            total = total
                .checked_add(data.len())
                .ok_or_else(|| PluginError::new("PACKAGE_TOO_LARGE", "Size overflow"))?;
        }
        if total > 50 * 1024 * 1024 {
            return Err(PluginError::new(
                "PACKAGE_TOO_LARGE",
                "Package exceeds 50MiB",
            ));
        }
        let stage = tempfile::Builder::new()
            .prefix(".stage-")
            .tempdir_in(self.root.join("packages"))?;
        for (name, data) in files {
            let file = stage.path().join(name);
            std::fs::create_dir_all(file.parent().unwrap())?;
            std::fs::write(file, data)?;
        }
        let mut descriptor = descriptor::describe_package(stage.path()).await?;
        let target = self.root.join("packages").join(&descriptor.id);
        descriptor.builtin = builtin
            || self
                .package(&descriptor.id)
                .is_ok_and(|p| p.descriptor.builtin);
        json_file::write(&stage.path().join("descriptor.json"), &descriptor)?;
        let affected = self
            .sources
            .lock()
            .unwrap()
            .values()
            .filter(|s| s.record.plugin_id == descriptor.id)
            .cloned()
            .collect::<Vec<_>>();
        let mut guards = Vec::new();
        for source in &affected {
            guards.push(source.runtime.write().await);
        }
        let backup = tempfile::Builder::new()
            .prefix(".backup-")
            .tempdir_in(self.root.join("packages"))?;
        // Candidate top-level code can write KV before activation fails. Keep the
        // old VMs alive and restore their files as part of the package rollback.
        let mut storage_backups = Vec::new();
        for (index, source) in affected.iter().enumerate() {
            let path = self
                .root
                .join("sources")
                .join(&source.record.source_id)
                .join("storage.json");
            let saved = backup.path().join(format!("storage-{index}"));
            let existed = path.exists();
            if existed {
                std::fs::copy(&path, &saved)?;
            }
            storage_backups.push((path, saved, existed));
        }
        let old = backup.path().join("package");
        let existed = target.exists();
        if existed {
            std::fs::rename(&target, &old)?;
        }
        if let Err(error) = std::fs::rename(stage.path(), &target) {
            if existed {
                std::fs::rename(&old, &target)?;
            }
            return Err(error.into());
        }
        let activate = async {
            let mut replacements = Vec::new();
            for source in &affected {
                replacements
                    .push(source::open_runtime(&self.root, &source.record, &descriptor).await?);
            }
            Ok::<_, PluginError>(replacements)
        }
        .await;
        match activate {
            Ok(replacements) => {
                for ((guard, source), vm) in guards.iter_mut().zip(&affected).zip(replacements) {
                    **guard = vm;
                    *source.descriptor.lock().unwrap() = descriptor.clone();
                }
                Ok(descriptor)
            }
            Err(error) => {
                std::fs::remove_dir_all(&target)?;
                if existed {
                    std::fs::rename(&old, &target)?;
                }
                for (path, saved, existed) in storage_backups {
                    if existed {
                        std::fs::rename(saved, path)?;
                    } else if path.exists() {
                        std::fs::remove_file(path)?;
                    }
                }
                Err(error)
            }
        }
    }
    pub async fn remove_package(&self, id: &str) -> Result<()> {
        let _transaction = self.transaction.lock().await;
        let package = self.package(id)?;
        if package.descriptor.builtin || self.records().iter().any(|r| r.plugin_id == id) {
            return Err(PluginError::new(
                "PACKAGE_IN_USE",
                "Builtin or referenced package cannot be removed",
            ));
        }
        std::fs::remove_dir_all(package.path)?;
        Ok(())
    }
    pub async fn connect(
        self: &Arc<Self>,
        plugin: &str,
        mut values: BTreeMap<String, String>,
        name: Option<String>,
    ) -> Result<Arc<Source>> {
        let _transaction = self.transaction.lock().await;
        let descriptor = self.package(plugin)?.descriptor;
        if descriptor.kind != "library" {
            return Err(PluginError::new(
                "INVALID_ARGUMENT",
                "Expected a library connection",
            ));
        }
        if let Some(connection) = &descriptor.connection {
            for (key, field) in connection["fields"].as_object().into_iter().flatten() {
                if field["required"] != false && values.get(key).is_none_or(|s| s.trim().is_empty())
                {
                    return Err(PluginError::new(
                        "INVALID_ARGUMENT",
                        format!("Missing {key}"),
                    ));
                }
                if field["type"] == "url" {
                    if let Some(v) = values.get_mut(key) {
                        *v = descriptor::normalize_source_url(v);
                    }
                }
            }
        }
        let id = descriptor::source_id(&descriptor, &values);
        let dir = self.root.join("sources").join(&id);
        let existed = dir.exists();
        let previous = self.sources.lock().unwrap().get(&id).cloned();
        let _guard = match &previous {
            Some(source) => Some(source.runtime.write().await),
            None => None,
        };
        let backup = tempfile::tempdir_in(self.root.join("sources"))?;
        for file in ["credential.json", "storage.json"] {
            if dir.join(file).exists() {
                std::fs::copy(dir.join(file), backup.path().join(file))?;
            }
        }
        let mut record = SourceRecord {
            source_id: id.clone(),
            plugin_id: plugin.into(),
            kind: "library".into(),
            display_name: name,
            added_at: now(),
            enabled: true,
            extra: BTreeMap::new(),
        };
        if let Some(old) = self.records().into_iter().find(|r| r.source_id == id) {
            record.added_at = old.added_at;
            record.extra = old.extra;
            if record.display_name.is_none() {
                record.display_name = old.display_name;
            }
        }
        let result = async {
            let source = Source::open_with(
                &self.root,
                &record,
                descriptor,
                Value::Null,
                Some(values.clone()),
            )
            .await?;
            source
                .call_raw("connection.verify", vec![json!(values)], json!({}), None)
                .await?;
            source.persist_credentials()?;
            let mut next = self.registry.lock().unwrap().clone();
            if let Some(previous) = next.sources.iter_mut().find(|s| s.source_id == id) {
                *previous = record;
            } else {
                next.sources.push(record);
            }
            self.save(next)?;
            Ok::<_, PluginError>(Arc::new(source))
        }
        .await;
        let source = match result {
            Ok(source) => source,
            Err(error) => {
                if !existed {
                    if dir.exists() {
                        std::fs::remove_dir_all(&dir)?;
                    }
                } else {
                    for file in ["credential.json", "storage.json"] {
                        let saved = backup.path().join(file);
                        let target = dir.join(file);
                        if saved.exists() {
                            std::fs::rename(saved, target)?;
                        } else if target.exists() {
                            std::fs::remove_file(target)?;
                        }
                    }
                }
                return Err(error);
            }
        };
        self.sources.lock().unwrap().insert(id, source.clone());
        Ok(source)
    }
    pub async fn begin_login(self: &Arc<Self>, plugin: &str) -> Result<LoginDraft> {
        let descriptor = self.package(plugin)?.descriptor;
        if descriptor.kind == "library" {
            return Err(PluginError::new(
                "INVALID_ARGUMENT",
                "Use connect for libraries",
            ));
        }
        let dir = tempfile::Builder::new()
            .prefix(".login-")
            .tempdir_in(self.root.join("sources"))?;
        let record = SourceRecord {
            source_id: dir
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            plugin_id: plugin.into(),
            kind: descriptor.kind.clone(),
            display_name: None,
            added_at: now(),
            enabled: true,
            extra: BTreeMap::new(),
        };
        let source = Arc::new(Source::open(&self.root, &record, descriptor).await?);
        Ok(LoginDraft {
            host: self.clone(),
            source,
            dir: Some(dir),
            authenticated: false,
        })
    }
    pub(crate) async fn commit_login(&self, draft: &mut LoginDraft) -> Result<Arc<Source>> {
        let _transaction = self.transaction.lock().await;
        if !draft.authenticated {
            return Err(PluginError::new("AUTH_REQUIRED", "Login has not succeeded"));
        }
        let id = draft.source.record.plugin_id.clone();
        let target = self.root.join("sources").join(&id);
        let backup = tempfile::tempdir_in(self.root.join("sources"))?;
        let existed = target.exists();
        // A replacement waits for all old runtime calls before moving its credentials.
        let previous = self.sources.lock().unwrap().get(&id).cloned();
        let _guard = match &previous {
            Some(s) => Some(s.runtime.write().await),
            None => None,
        };
        if existed {
            std::fs::rename(&target, backup.path().join("old"))?;
        }
        let temp = draft.dir.as_ref().unwrap().path();
        if let Err(e) = std::fs::rename(temp, &target) {
            if existed {
                std::fs::rename(backup.path().join("old"), &target)?;
            }
            return Err(e.into());
        }
        let mut record = draft.source.record.clone();
        record.source_id = id.clone();
        let descriptor = draft.source.descriptor.lock().unwrap().clone();
        let result = async {
            let source = Arc::new(Source::open(&self.root, &record, descriptor).await?);
            let mut next = self.registry.lock().unwrap().clone();
            if let Some(old) = next.sources.iter_mut().find(|s| s.source_id == id) {
                record.added_at = old.added_at;
                record.display_name = old.display_name.clone();
                *old = record.clone();
            } else {
                next.sources.push(record.clone());
            }
            self.save(next)?;
            Ok::<_, PluginError>(source)
        }
        .await;
        match result {
            Ok(source) => {
                draft.dir.take();
                self.sources.lock().unwrap().insert(id, source.clone());
                Ok(source)
            }
            Err(e) => {
                std::fs::rename(&target, temp)?;
                if existed {
                    std::fs::rename(backup.path().join("old"), &target)?;
                }
                Err(e)
            }
        }
    }
    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let _transaction = self.transaction.lock().await;
        let mut next = self.registry.lock().unwrap().clone();
        let record = next
            .sources
            .iter_mut()
            .find(|r| r.source_id == id)
            .ok_or_else(|| PluginError::new("NOT_FOUND", "Unknown source"))?;
        record.enabled = enabled;
        let replacement = if enabled {
            Some(Arc::new(
                Source::open(
                    &self.root,
                    record,
                    self.package(&record.plugin_id)?.descriptor,
                )
                .await?,
            ))
        } else {
            None
        };
        self.save(next)?;
        if let Some(s) = replacement {
            self.sources.lock().unwrap().insert(id.into(), s);
        } else {
            if let Some(s) = self.sources.lock().unwrap().remove(id) {
                s.cancel_all();
            }
        }
        Ok(())
    }
    pub async fn reorder(&self, ids: &[String]) -> Result<()> {
        let _transaction = self.transaction.lock().await;
        let mut next = self.registry.lock().unwrap().clone();
        let order = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect::<BTreeMap<_, _>>();
        next.sources
            .sort_by_key(|r| order.get(&r.source_id).copied().unwrap_or(usize::MAX));
        self.save(next)
    }
    pub async fn rename(&self, id: &str, name: Option<String>) -> Result<()> {
        let _transaction = self.transaction.lock().await;
        let mut next = self.registry.lock().unwrap().clone();
        let record = next
            .sources
            .iter_mut()
            .find(|r| r.source_id == id)
            .ok_or_else(|| PluginError::new("NOT_FOUND", "Unknown source"))?;
        record.display_name = name;
        self.save(next)
    }
    pub async fn remove_source(&self, id: &str) -> Result<()> {
        let _transaction = self.transaction.lock().await;
        valid_path(id)?;
        let mut next = self.registry.lock().unwrap().clone();
        next.sources.retain(|r| r.source_id != id);
        self.save(next)?;
        if let Some(s) = self.sources.lock().unwrap().remove(id) {
            s.cancel_all();
        }
        let path = self.root.join("sources").join(id);
        if path.exists() {
            std::fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}
