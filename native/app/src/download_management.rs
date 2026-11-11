//! Collection reconciliation runs independently of UI effect lifetimes.
use super::*;
use haiku_store::collections::Collections;
use std::collections::BTreeSet;

impl Service {
    pub fn directory_snapshot(&self) -> DirectoryProgress {
        self.directory_events.borrow().clone()
    }

    pub fn directory_subscribe(&self) -> watch::Receiver<DirectoryProgress> {
        self.directory_events.subscribe()
    }

    /// Drain active workers, migrate owned files without overwrite, then resume scheduling.
    pub async fn change_directory(
        self: &Arc<Self>,
        next: String,
        mode: String,
    ) -> Result<DirectoryProgress> {
        if mode == "cancel" {
            return Ok(DirectoryProgress::default());
        }
        if !["move", "keep"].contains(&mode.as_str()) {
            return Err("目录操作必须为 move、keep 或 cancel".into());
        }
        let next = if next.is_empty() {
            PathBuf::from(std::env::var_os("HOME").ok_or("未找到用户目录")?).join("Music/Haiku")
        } else {
            PathBuf::from(next)
        };
        if !next.is_absolute() {
            return Err("下载目录必须是绝对路径".into());
        }
        let _serial = self.management.lock().await;
        // Validate/create before interrupting downloads; default stays unchanged on failure.
        fs::create_dir_all(&next).map_err(|e| e.to_string())?;
        let next = next.canonicalize().map_err(|e| e.to_string())?;
        let current = self.settings.get().download_dir;
        let old = if current.is_empty() {
            PathBuf::from(std::env::var_os("HOME").ok_or("未找到用户目录")?).join("Music/Haiku")
        } else {
            PathBuf::from(current)
        };
        let old = old.canonicalize().unwrap_or(old);
        let mut events = self.subscribe();
        {
            let mut state = self.state.lock().unwrap();
            if state.stopped {
                return Err("下载服务已关闭".into());
            }
            state.migrating = true;
            for active in state.active.values() {
                active.cancel.cancel();
            }
            // User-paused jobs stay paused. Interrupted running jobs resume after migration.
            for item in &mut state.items {
                if active_status(&item.status) {
                    item.status = "queued".into();
                }
            }
        }
        let _resume = ResumeScheduler(self.clone());
        self.directory_events.send_replace(DirectoryProgress {
            running: true,
            ..Default::default()
        });
        while !self.state.lock().unwrap().active.is_empty() {
            events.changed().await.map_err(|e| e.to_string())?;
        }
        let mut report = DirectoryProgress {
            running: true,
            ..Default::default()
        };
        {
            let mut state = self.state.lock().unwrap();
            if state.stopped {
                return Err("下载服务已关闭".into());
            }
            if mode == "move" && old != next {
                report.total = state
                    .items
                    .iter()
                    .map(|i| {
                        i.path
                            .iter()
                            .chain(i.temp_path.iter())
                            .chain(i.sidecars.iter())
                            .count()
                    })
                    .sum();
                self.directory_events.send_replace(report.clone());
                for item in &mut state.items {
                    for path in item
                        .path
                        .iter_mut()
                        .chain(item.temp_path.iter_mut())
                        .chain(item.sidecars.iter_mut())
                    {
                        match move_owned_file(path, &old, &next) {
                            Ok(Some(target)) => {
                                *path = target.to_string_lossy().into();
                                report.moved += 1;
                            }
                            Ok(None) => {}
                            Err(error) => report
                                .errors
                                .push(format!("{} · {path}：{error}", item.track.title)),
                        }
                        report.completed += 1;
                        self.directory_events.send_replace(report.clone());
                    }
                }
            }
            // Failed moves retain their original paths; records commit before the new default.
            self.save(&state)?;
            self.settings
                .update(json!({"downloadDir": next.to_string_lossy()}))?;
        }
        report.running = false;
        self.directory_events.send_replace(report.clone());
        Ok(report)
    }

    /// Watch committed local/favorite snapshots and relevant settings, including startup.
    pub fn watch_collections(self: &Arc<Self>, collections: Collections) {
        let weak = Arc::downgrade(self);
        let events = collections.subscribe();
        downloader::runtime().spawn(async move {
            let mut previous = None;
            loop {
                let Some(service) = weak.upgrade() else { break };
                if service.state.lock().unwrap().stopped {
                    break;
                }
                let settings = service.settings.get();
                let input = (
                    collections.revision(),
                    settings.auto_download_playlists,
                    settings.download_follow_playlist_removal,
                );
                // Drain committed change notifications; compare only reconciliation inputs.
                while events.try_recv().is_ok() {}
                if previous.as_ref() != Some(&input) {
                    match service.reconcile_auto(&collections).await {
                        Ok(()) => previous = Some(input),
                        Err(error) => log::error!("Automatic downloads: {error}"),
                    }
                }
                drop(service);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
    }

    /// Reconcile complete durable snapshots; remote refresh already commits atomically.
    pub(super) async fn reconcile_auto(self: &Arc<Self>, collections: &Collections) -> Result<()> {
        let _serial = self.management.lock().await;
        if self.state.lock().unwrap().restoring { return Err("正在恢复备份".into()); }
        let settings = self.settings.get();
        let revision = collections.revision();
        let metadata = collections.list();
        let playlists: Vec<_> = settings
            .auto_download_playlists
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    metadata.iter().find(|p| p.id == *id),
                    collections.tracks(id),
                )
            })
            .collect();
        if collections.revision() != revision {
            return Err("歌单已变化，等待下次同步".into());
        }
        let mut membership: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (id, meta, tracks) in playlists {
            // Likes and absent/deleted playlists cannot create automatic jobs.
            if !meta.is_some_and(|m| ["user", "plugin"].contains(&m.kind.as_str())) {
                continue;
            }
            membership.insert(id.clone(), tracks.iter().map(key).collect());
            self.enqueue_tracks(
                tracks,
                meta.map(|m| m.title.clone()),
                Some(id.clone()),
                &format!("auto:{id}"),
            )?;
        }
        let removed = {
            let mut state = self.state.lock().unwrap();
            let mut removed = vec![];
            for item in &mut state.items {
                let before = item.playlist_ids.len();
                item.playlist_ids.retain(|id| {
                    !settings.auto_download_playlists.contains(id)
                        || membership
                            .get(id)
                            .is_some_and(|keys| keys.contains(&item.key))
                });
                if (before != item.playlist_ids.len()
                    || item.extra.get("autoRemovalPending") == Some(&json!(true)))
                    && settings.download_follow_playlist_removal
                    && item.origin != "manual"
                    && !item
                        .playlist_ids
                        .iter()
                        .any(|id| settings.auto_download_playlists.contains(id))
                {
                    item.extra.insert("autoRemovalPending".into(), json!(true));
                    removed.push(item.key.clone());
                }
            }
            self.save(&state)?;
            removed
        };
        for key in removed {
            self.remove_auto(&key, collections).await?;
        }
        Ok(())
    }

    fn auto_protected(&self, item: &Item, collections: &Collections) -> bool {
        let settings = self.settings.get();
        !settings.download_follow_playlist_removal
            || item.origin == "manual"
            || settings.auto_download_playlists.iter().any(|id| {
                collections
                    .tracks(id)
                    .iter()
                    .any(|track| key(track) == item.key)
            })
    }

    async fn remove_auto(self: &Arc<Self>, key: &str, collections: &Collections) -> Result<()> {
        let mut events = self.subscribe();
        {
            let mut state = self.state.lock().unwrap();
            let Some(item) = state.items.iter_mut().find(|i| i.key == key) else {
                return Ok(());
            };
            if self.auto_protected(item, collections) {
                return Ok(());
            }
            if active_status(&item.status) || item.status == "queued" {
                item.status = "canceled".into();
            }
            if let Some(active) = state.active.get(key) {
                active.cancel.cancel();
            }
            self.save(&state)?;
        }
        while self.state.lock().unwrap().active.contains_key(key) {
            events.changed().await.map_err(|e| e.to_string())?;
        }
        let mut state = self.state.lock().unwrap();
        let Some(item) = state.items.iter().find(|i| i.key == key) else {
            return Ok(());
        };
        // A manual enqueue or playlist update during cancellation wins over follow removal.
        if self.auto_protected(item, collections) {
            return Ok(());
        }
        for path in item
            .path
            .iter()
            .chain(item.temp_path.iter())
            .chain(item.sidecars.iter())
        {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("跟随移除文件失败 {path}：{e}")),
            }
        }
        state.items.retain(|i| i.key != key);
        self.save(&state)
    }
}

struct ResumeScheduler(Arc<Service>);
impl Drop for ResumeScheduler {
    fn drop(&mut self) {
        self.0.state.lock().unwrap().migrating = false;
        self.0
            .directory_events
            .send_modify(|progress| progress.running = false);
        self.0.pump();
    }
}

fn move_owned_file(source: &str, old: &Path, next: &Path) -> Result<Option<PathBuf>> {
    let path = Path::new(source);
    // Completed jobs normally retain a now-absent tempPath.
    let source = match path.canonicalize() {
        Ok(path) => path,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let relative = source
        .strip_prefix(old)
        .map_err(|_| "文件不在原下载目录，保留原路径")?;
    let target = next.join(relative);
    if target == source {
        return Ok(None);
    }
    let parent = target.parent().ok_or("无效目标路径")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    // Canonicalize the parent to reject symlinks escaping the chosen directory.
    if !parent
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(next)
    {
        return Err("目标目录链接超出下载目录".into());
    }
    // Copy into a temporary file, then publish without overwrite; works across volumes.
    let temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    fs::copy(&source, temp.path()).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist_noclobber(&target).map_err(|e| e.to_string())?;
    if let Err(error) = fs::remove_file(&source) {
        let _ = fs::remove_file(&target);
        return Err(error.to_string());
    }
    Ok(Some(target))
}
