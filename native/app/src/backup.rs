//! Hot restoration preserves the identity of services already held by TSX and native views.
use futures::future::BoxFuture;
use haiku_services::backup::{Hooks, Service};
use haiku_store::{collections::Collections, settings::SettingsStore};
use nsh_core::{JsonValue, prelude::*};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};

pub(crate) struct Runtime {
    pub root: PathBuf,
    pub secrets: Arc<dyn haiku_services::sync::Secrets>,
    pub controller: haiku_playback::Controller,
    pub host: Arc<haiku_plugins::Host>,
    pub settings: Arc<SettingsStore>,
    pub collections: Collections,
    pub history: haiku_store::history::History,
    pub stats: haiku_store::stats::Stats,
    pub sync: Arc<haiku_services::sync::Service>,
    pub downloads: Arc<crate::download_service::Service>,
}
impl Hooks for Runtime {
    fn flush(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async {
            self.controller
                .execute(haiku_playback::Command::Flush)
                .await
                .map_err(|e| e.message)?;
            self.collections.flush()?;
            self.history.flush()?;
            self.stats.flush()?;
            self.host.flush().map_err(|e| e.to_string())?;
            self.downloads.flush()
        })
    }
    fn before_restore(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async {
            self.controller
                .execute(haiku_playback::Command::Clear)
                .await
                .map_err(|e| e.message)?;
            self.sync.shutdown().await?;
            self.downloads.suspend_restore().await?;
            self.host.quiesce().await.map_err(|e| e.to_string())
        })
    }
    fn after_restore(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async {
            // Run every reload/resume, even if an earlier step failed.
            let mut errors = vec![];
            if let Err(e) = self.host.reload().await {
                errors.push(e.to_string());
            }
            if let Err(e) = self.settings.reload() {
                errors.push(e);
            }
            if let Err(e) = self.collections.reload() {
                errors.push(e);
            }
            if let Err(e) = self.sync.resume_after_restore() {
                errors.push(e);
            }
            self.downloads.resume_restore();
            if errors.is_empty() {
                Ok(())
            } else {
                Err(errors.join("; "))
            }
        })
    }
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ExportOptions {
    include_github_token: bool,
}

/// Reference-compatible .haikubak files and ordered live-service restoration.
#[class]
pub struct BackupService {
    service: Arc<Service>,
    settings: Instance<crate::settings::SettingsService>,
    plugins: Instance<crate::plugins::PluginService>,
}
impl BackupService {
    pub(crate) fn new(
        runtime: Runtime,
        settings: Instance<crate::settings::SettingsService>,
        plugins: Instance<crate::plugins::PluginService>,
    ) -> Instance<Self> {
        let service = Arc::new(Service::new(
            runtime.root.clone(),
            runtime.secrets.clone(),
            Arc::new(runtime),
        ));
        Instance::new(Self {
            service,
            settings,
            plugins,
        })
    }
}
#[class]
impl BackupService {
    /// Flush services and export schema 2; GitHub PAT is excluded unless explicitly requested.
    pub async fn export(
        this: Instance<Self>,
        path: String,
        options: JsonValue,
    ) -> Result<(), String> {
        let options: ExportOptions = serde_json::from_value(options).map_err(|e| e.to_string())?;
        let service = this.with(|s| s.service.clone());
        background(async move {
            service
                .export(&PathBuf::from(path), options.include_github_token)
                .await
        })
        .await
        .map_err(|e| e.to_string())?
    }
    /// Validate and retain the exact selected bytes for a later import confirmation.
    pub async fn inspect(this: Instance<Self>, path: String) -> Result<JsonValue, String> {
        let service = this.with(|s| s.service.clone());
        blocking(move || {
            service
                .inspect(&PathBuf::from(path))
                .map(|p| serde_json::to_value(p).unwrap())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    /// Safety-back up, restore inspected bytes, and reload mirrors even when restoration fails.
    pub async fn import(this: Instance<Self>, path: String) -> Result<JsonValue, String> {
        let service = this.with(|s| s.service.clone());
        let result = background(async move { service.import(&PathBuf::from(path)).await })
            .await
            .map_err(|e| e.to_string())?;
        let (settings, plugins) = this.with(|s| (s.settings.clone(), s.plugins.clone()));
        let refreshed = crate::settings::SettingsService::refresh(settings).await;
        let sources = crate::plugins::PluginService::changed(&plugins).await;
        let report = result?;
        refreshed?;
        sources?;
        Ok(serde_json::to_value(report).unwrap())
    }
    /// List up to three retained pre-restore safety backups, newest first.
    pub async fn safety_backups(this: Instance<Self>) -> Result<Vec<String>, String> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.safety_backups())
            .await
            .map_err(|e| e.to_string())?
    }
    /// Show the native save dialog; cancellation returns null.
    pub async fn pick_export() -> Result<Option<String>, String> {
        use nsh_core::file_dialog::{FileFilter, SaveDialog};
        pick_save_path(
            SaveDialog::new()
                .file_name("haiku-backup.haikubak")
                .filter(FileFilter::new("Haiku 备份", ["haikubak"])),
        )
        .await
        .map(|p| p.map(|p| p.to_string_lossy().into_owned()))
        .map_err(|e| e.to_string())
    }
    /// Show the native .haikubak open dialog; cancellation returns null.
    pub async fn pick_import() -> Result<Option<String>, String> {
        use nsh_core::file_dialog::{FileFilter, OpenDialog};
        pick_paths(OpenDialog::files().filter(FileFilter::new("Haiku 备份", ["haikubak"])))
            .await
            .map(|paths| paths.and_then(|p| p.first().map(|p| p.to_string_lossy().into_owned())))
            .map_err(|e| e.to_string())
    }
}
