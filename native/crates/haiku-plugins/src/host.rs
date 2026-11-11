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
    errors: Mutex<BTreeMap<String, PluginError>>,
    replay_base: Option<String>,
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
#[derive(Serialize, Deserialize)]
struct InstallJournal {
    id: String,
    existed: bool,
    storage: Vec<(String, bool)>,
    committed: bool,
}
fn recover_installs(root: &Path) -> Result<()> {
    for entry in std::fs::read_dir(root.join("packages"))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.file_type()?.is_dir() {
            continue;
        }
        if name.starts_with(".backup-") {
            if let Some(journal) =
                json_file::read::<InstallJournal>(&entry.path().join("journal.json"))?
            {
                valid_path(&journal.id)?;
                if !journal.committed {
                    let target = root.join("packages").join(&journal.id);
                    let old = entry.path().join("package");
                    if old.exists() {
                        if target.exists() {
                            std::fs::remove_dir_all(&target)?;
                        }
                        std::fs::rename(old, &target)?;
                    } else if !journal.existed && target.exists() {
                        std::fs::remove_dir_all(&target)?;
                    }
                    for (index, (id, existed)) in journal.storage.iter().enumerate() {
                        valid_path(id)?;
                        let target = root.join("sources").join(id).join("storage.json");
                        let saved = entry.path().join(format!("storage-{index}"));
                        // Copy leaves the journal restartable if recovery itself is interrupted.
                        if *existed && saved.exists() {
                            std::fs::copy(saved, target)?;
                        } else if !existed && target.exists() {
                            std::fs::remove_file(target)?;
                        }
                    }
                }
            }
            std::fs::remove_dir_all(entry.path())?;
        } else if name.starts_with(".stage-") {
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    for entry in std::fs::read_dir(root.join("sources"))? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.file_name().to_string_lossy().starts_with(".login-")
        {
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}
impl Host {
    /// Open an independent data root, installing the embedded builtin on first use.
    pub async fn open(data: &Path) -> Result<Arc<Self>> {
        Self::open_with_replay(data, None).await
    }
    pub(crate) async fn open_with_replay(
        data: &Path,
        replay_base: Option<String>,
    ) -> Result<Arc<Self>> {
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
        recover_installs(&root)?;
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
            errors: Mutex::default(),
            replay_base,
        });
        for (id, source, fixture, logo) in [
            (
                "subsonic.music",
                include_bytes!("../../../../plugins/subsonic/plugin.ts").as_slice(),
                include_bytes!("../../../../plugins/subsonic/fixtures/contract.json").as_slice(),
                include_bytes!("../../../../plugins/subsonic/logo.png").as_slice(),
            ),
            (
                "feiniu.music",
                include_bytes!("../../../../plugins/feiniu/plugin.ts").as_slice(),
                include_bytes!("../../../../plugins/feiniu/fixtures/contract.json").as_slice(),
                include_bytes!("../../../../plugins/feiniu/logo.png").as_slice(),
            ),
            (
                "qq.scraper",
                include_bytes!("../../../../plugins/qq-scraper/plugin.ts").as_slice(),
                include_bytes!("../../../../plugins/qq-scraper/fixtures/contract.json").as_slice(),
                include_bytes!("../../../../plugins/qq-scraper/logo.png").as_slice(),
            ),
            (
                "kugou.scraper",
                include_bytes!("../../../../plugins/kugou-scraper/plugin.ts").as_slice(),
                include_bytes!("../../../../plugins/kugou-scraper/fixtures/contract.json")
                    .as_slice(),
                include_bytes!("../../../../plugins/kugou-scraper/logo.png").as_slice(),
            ),
        ] {
            if !host
                .root
                .join("packages")
                .join(id)
                .join("descriptor.json")
                .exists()
            {
                host.install_files(
                    BTreeMap::from([
                        ("plugin.ts".into(), source.to_vec()),
                        ("logo.png".into(), logo.to_vec()),
                        ("fixtures/contract.json".into(), fixture.to_vec()),
                    ]),
                    true,
                )
                .await?;
                log::info!("Installed builtin plugin {id} into {}", host.root.display());
            }
        }
        let records = host.records();
        for record in records {
            if !record.enabled {
                continue;
            }
            let result = async {
                valid_path(&record.source_id)?;
                let descriptor = host.package(&record.plugin_id)?.descriptor;
                Source::open(&host.root, &record, descriptor, host.replay_base.as_deref()).await
            }
            .await;
            match result {
                Ok(instance) => {
                    host.sources
                        .lock()
                        .unwrap()
                        .insert(record.source_id, Arc::new(instance));
                }
                Err(error) => {
                    log::warn!(
                        "Source {} isolated during startup: {}",
                        record.source_id,
                        error.code
                    );
                    host.errors.lock().unwrap().insert(record.source_id, error);
                }
            }
        }
        hosts.lock().unwrap().insert(data, Arc::downgrade(&host));
        Ok(host)
    }
    /// Observe this host's redacted plugin log and HTTP events; receiver lag drops old entries.
    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<Value> {
        crate::events::sender(&self.root).subscribe()
    }
    pub fn source_errors(&self) -> BTreeMap<String, PluginError> {
        self.errors.lock().unwrap().clone()
    }
    pub fn packages(&self) -> Result<Vec<Package>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(self.root.join("packages"))? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with('.') || !entry.file_type()?.is_dir()
            {
                continue;
            }
            let descriptor =
                match json_file::read::<Descriptor>(&entry.path().join("descriptor.json")) {
                    Ok(Some(descriptor)) => descriptor,
                    _ => {
                        log::warn!(
                            "Skipping invalid plugin descriptor: {}",
                            entry.file_name().to_string_lossy()
                        );
                        continue;
                    }
                };
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
    /// KV writes are immediate; persist each current credential snapshot before export.
    pub fn flush(&self) -> Result<()> {
        for source in self.sources.lock().unwrap().values() { source.persist_credentials()?; }
        Ok(())
    }
    /// Cancel calls and drain their runtime guards before backup replacement.
    pub async fn quiesce(&self) -> Result<()> {
        let _transaction = self.transaction.lock().await;
        let sources = self.sources.lock().unwrap().values().cloned().collect::<Vec<_>>();
        for source in &sources { source.cancel_all(); }
        for source in sources {
            let _runtime = source.runtime.write().await;
            source.persist_credentials()?;
        }
        Ok(())
    }
    /// Reload packages and instances in place, so resolvers and UI retain the same host.
    pub async fn reload(&self) -> Result<()> {
        let _transaction = self.transaction.lock().await;
        self.sources.lock().unwrap().clear();
        self.errors.lock().unwrap().clear();
        let registry: Registry = json_file::read(&self.root.join("sources.json"))?.unwrap_or_default();
        *self.registry.lock().unwrap() = registry;
        for record in self.records().into_iter().filter(|r| r.enabled) {
            let result = async {
                valid_path(&record.source_id)?;
                let descriptor = self.package(&record.plugin_id)?.descriptor;
                Source::open(&self.root, &record, descriptor, self.replay_base.as_deref()).await
            }.await;
            match result {
                Ok(source) => { self.sources.lock().unwrap().insert(record.source_id, Arc::new(source)); }
                Err(error) => { self.errors.lock().unwrap().insert(record.source_id, error); }
            }
        }
        Ok(())
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
        let mut journal = InstallJournal {
            id: descriptor.id.clone(),
            existed,
            storage: affected
                .iter()
                .zip(&storage_backups)
                .map(|(s, (_, _, existed))| (s.record.source_id.clone(), *existed))
                .collect(),
            committed: false,
        };
        json_file::write(&backup.path().join("journal.json"), &journal)?;
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
                replacements.push(
                    source::open_runtime(
                        &self.root,
                        &source.record,
                        &descriptor,
                        self.replay_base.as_deref(),
                    )
                    .await?,
                );
            }
            Ok::<_, PluginError>(replacements)
        }
        .await;
        match activate {
            Ok(replacements) => {
                journal.committed = true;
                if let Err(error) = json_file::write(&backup.path().join("journal.json"), &journal)
                {
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
                    return Err(error.into());
                }
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
    /// Import a local package or bounded HTTPS ZIP/TypeScript download.
    pub async fn import_location(&self, location: &str) -> Result<Descriptor> {
        let path = Path::new(location);
        if path.is_dir() {
            return self.install_directory(path, false).await;
        }
        let (bytes, typescript) = if location.starts_with("https://") {
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|e| PluginError::new("UPSTREAM_ERROR", e.to_string()))?;
            let mut response = client
                .get(location)
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await
                .map_err(|e| PluginError::new("UPSTREAM_ERROR", e.to_string()))?
                .error_for_status()
                .map_err(|e| PluginError::new("UPSTREAM_ERROR", e.to_string()))?;
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| PluginError::new("UPSTREAM_ERROR", e.to_string()))?
            {
                if bytes.len() + chunk.len() > 10 * 1024 * 1024 {
                    return Err(PluginError::new(
                        "INVALID_PACKAGE",
                        "Download exceeds 10 MiB",
                    ));
                }
                bytes.extend_from_slice(&chunk);
            }
            let url = url::Url::parse(location)
                .map_err(|e| PluginError::new("INVALID_ARGUMENT", e.to_string()))?;
            (bytes, url.path().ends_with(".ts"))
        } else {
            if location.contains("://") {
                return Err(PluginError::new("INVALID_ARGUMENT", "Use HTTPS"));
            }
            let metadata = std::fs::metadata(path)?;
            if metadata.len() > 10 * 1024 * 1024 {
                return Err(PluginError::new("INVALID_PACKAGE", "File exceeds 10 MiB"));
            }
            (
                std::fs::read(path)?,
                path.extension().is_some_and(|v| v == "ts"),
            )
        };
        if typescript {
            self.install_files(BTreeMap::from([("plugin.ts".into(), bytes)]), false)
                .await
        } else {
            self.install_zip(&bytes).await
        }
    }
    /// Inspect KV without reading stale files while the VM is active.
    pub async fn debug_storage(&self, id: &str) -> Result<Value> {
        let source = self.source(id)?;
        let runtime = source.runtime.read().await;
        Ok(runtime
            .call("debugStorage", "null", std::time::Duration::from_secs(5))
            .await?)
    }
    /// Clear KV in the VM, atomically persisting through the storage binding.
    pub async fn clear_storage(&self, id: &str) -> Result<()> {
        let source = self.source(id)?;
        let runtime = source.runtime.read().await;
        runtime
            .call("clearStorage", "null", std::time::Duration::from_secs(5))
            .await?;
        Ok(())
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
        values: BTreeMap<String, String>,
        name: Option<String>,
    ) -> Result<Arc<Source>> {
        self.connect_cancellable(plugin, values, name, None).await
    }
    /// Connect with an optional UI cancellation token, rolling back failed/closed drafts.
    pub async fn connect_cancellable(
        self: &Arc<Self>,
        plugin: &str,
        values: BTreeMap<String, String>,
        name: Option<String>,
        cancel: Option<&crate::Cancellation>,
    ) -> Result<Arc<Source>> {
        let _transaction = self.transaction.lock().await;
        if cancel.is_some_and(|c| c.is_cancelled()) {
            return Err(PluginError::new("CANCELLED", "Connection cancelled"));
        }
        let descriptor = self.package(plugin)?.descriptor;
        if !self.package_enabled(plugin) {
            return Err(PluginError::new(
                "PLUGIN_DISABLED",
                "Enable the package before connecting",
            ));
        }
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
                self.replay_base.as_deref(),
            )
            .await?;
            source
                .call_raw("connection.verify", vec![json!(values)], json!({}), cancel)
                .await?;
            if cancel.is_some_and(|c| c.is_cancelled()) {
                return Err(PluginError::new("CANCELLED", "Connection cancelled"));
            }
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
    /// Activate a public plugin whose handlers do not require a login.
    pub async fn activate(self: &Arc<Self>, plugin: &str) -> Result<Arc<Source>> {
        let mut draft = self.begin_login(plugin).await?;
        if !draft.source.descriptor().auth_methods.is_empty() {
            return Err(PluginError::new("AUTH_REQUIRED", "Use a login draft"));
        }
        draft.authenticated = true;
        draft.commit().await
    }
    pub async fn begin_login(self: &Arc<Self>, plugin: &str) -> Result<LoginDraft> {
        if !self.package_enabled(plugin) {
            return Err(PluginError::new(
                "PLUGIN_DISABLED",
                "Enable the package before login",
            ));
        }
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
        let source = Arc::new(
            Source::open(&self.root, &record, descriptor, self.replay_base.as_deref()).await?,
        );
        Ok(LoginDraft {
            host: self.clone(),
            source,
            dir: Some(dir),
            authenticated: false,
            sms: Default::default(),
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
            let source = Arc::new(
                Source::open(&self.root, &record, descriptor, self.replay_base.as_deref()).await?,
            );
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
    /// Package enablement is independent of whether an account has been added.
    pub fn package_enabled(&self, id: &str) -> bool {
        !self
            .registry
            .lock()
            .unwrap()
            .extra
            .get("disabledPlugins")
            .and_then(Value::as_array)
            .is_some_and(|ids| ids.iter().any(|v| v == id))
    }
    /// Atomically change a package and its instances, preparing all VMs before committing.
    pub async fn set_package_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let _transaction = self.transaction.lock().await;
        let descriptor = self.package(id)?.descriptor;
        let mut next = self.registry.lock().unwrap().clone();
        let mut disabled = next
            .extra
            .get("disabledPlugins")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        disabled.retain(|v| v != id);
        if !enabled {
            disabled.push(json!(id));
        }
        next.extra.insert("disabledPlugins".into(), json!(disabled));
        let mut replacements = Vec::new();
        for record in next.sources.iter_mut().filter(|r| r.plugin_id == id) {
            record.enabled = enabled;
            let source = if enabled {
                Some(Arc::new(
                    Source::open(
                        &self.root,
                        record,
                        descriptor.clone(),
                        self.replay_base.as_deref(),
                    )
                    .await?,
                ))
            } else {
                None
            };
            replacements.push((record.source_id.clone(), source));
        }
        self.save(next)?;
        let mut sources = self.sources.lock().unwrap();
        for (id, replacement) in replacements {
            if let Some(old) = sources.remove(&id) {
                old.cancel_all();
            }
            if let Some(source) = replacement {
                sources.insert(id, source);
            }
        }
        Ok(())
    }
    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let _transaction = self.transaction.lock().await;
        let mut next = self.registry.lock().unwrap().clone();
        let record = next
            .sources
            .iter_mut()
            .find(|r| r.source_id == id)
            .ok_or_else(|| PluginError::new("NOT_FOUND", "Unknown source"))?;
        if enabled && !self.package_enabled(&record.plugin_id) {
            return Err(PluginError::new(
                "PLUGIN_DISABLED",
                "Enable the package first",
            ));
        }
        record.enabled = enabled;
        let replacement = if enabled {
            Some(Arc::new(
                Source::open(
                    &self.root,
                    record,
                    self.package(&record.plugin_id)?.descriptor,
                    self.replay_base.as_deref(),
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
