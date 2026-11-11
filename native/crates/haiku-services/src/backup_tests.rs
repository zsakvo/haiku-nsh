use super::*;
use crate::sync::{MemorySecrets, Secrets};
use haiku_store::{collections::Collections, settings::SettingsStore};
use std::sync::atomic::{AtomicUsize, Ordering};

fn service(root: &Path) -> Service {
    Service::new(
        root.into(),
        Arc::new(MemorySecrets::default()),
        Arc::new(NoHooks),
    )
}
fn fixture(schema: u32) -> Files {
    BTreeMap::from([
        ("backup-meta.json".into(), serde_json::to_vec(&json!({
            "schemaVersion":schema,"appVersion":"0.1.0","createdAt":"2026-11-11T00:00:00Z",
            "platform":"macos","includesToken":false,"includes":["haiku_state/settings.json","haiku_state/likes.json"]
        })).unwrap()),
        ("haiku_state/settings.json".into(), br#"{"brandColor":"blue","glassAutoInvert":true}"#.to_vec()),
        ("haiku_state/likes.json".into(), br#"{"tracks":[{"pluginId":"local","trackId":"legacy","title":"Legacy","likedAt":100}]}"#.to_vec()),
    ])
}
#[tokio::test]
async fn reference_schema_one_two_and_legacy_likes_restore_and_roundtrip() {
    for schema in [1, 2] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("reference.haikubak");
        fs::write(&source, pack(&fixture(schema)).unwrap()).unwrap();
        let service = service(root.path());
        assert_eq!(
            service.inspect(&source).unwrap().meta.schema_version,
            schema
        );
        service.import(&source).await.unwrap();
        let restored = Collections::open(root.path()).unwrap();
        assert_eq!(restored.tracks(LIKED_PLAYLIST_ID)[0].id, "legacy");
        restored.flush().unwrap();
        let settings = SettingsStore::open(root.path()).unwrap();
        assert_eq!(settings.get().extra["glassAutoInvert"], true);
        let output = root.path().join("out.haikubak");
        service.export(&output, false).await.unwrap();
        let preview = service.inspect(&output).unwrap();
        assert_eq!(preview.meta.schema_version, 2);
        assert_eq!(preview.meta.app_name, "Haiku NSH");
        assert!(!preview.meta.includes_token);
        let before = fs::read(root.path().join("playlists.loro")).unwrap();
        // Replacing the file after preview must not change the bytes restored.
        fs::write(&output, b"replaced").unwrap();
        service.import(&output).await.unwrap();
        assert_eq!(
            fs::read(root.path().join("playlists.loro")).unwrap(),
            before
        );
    }
}
#[tokio::test]
async fn backup_v2_packages_credentials_optional_token_and_safety_rotation() {
    let root = tempfile::tempdir().unwrap();
    let host = haiku_plugins::Host::open(root.path()).await.unwrap();
    let descriptor = host.package("subsonic.music").unwrap().descriptor;
    let values = BTreeMap::from([
        ("baseUrl".into(), "http://127.0.0.1:9".into()),
        ("username".into(), "listener".into()),
    ]);
    let id = haiku_plugins::source_id(&descriptor, &values);
    json_file::write(
        &root.path().join("plugins/sources.json"),
        &json!({"version":1,"sources":[{
            "sourceId":id,"pluginId":"subsonic.music","kind":"library","enabled":true,"addedAt":0
        }]}),
    )
    .unwrap();
    json_file::write(
        &root
            .path()
            .join(format!("plugins/sources/{id}/credential.json")),
        &json!({"connection":values,"credential":{"password":"private"}}),
    )
    .unwrap();
    json_file::write(
        &root
            .path()
            .join(format!("plugins/sources/{id}/storage.json")),
        &json!({"stored":"value"}),
    )
    .unwrap();
    fs::write(root.path().join("history.json"), b"must not export").unwrap();
    let secrets = Arc::new(MemorySecrets::default());
    secrets.write(Some("private-pat")).unwrap();
    let service = Service::new(root.path().into(), secrets, Arc::new(NoHooks));
    let path = root.path().join("out.haikubak");
    service.export(&path, false).await.unwrap();
    assert!(!service.inspect(&path).unwrap().meta.includes_token);
    let decoded = decode(&fs::read(&path).unwrap()).unwrap();
    assert!(!decoded
        .files
        .keys()
        .any(|p| p.contains("history") || p.contains("secrets")));
    service.export(&path, true).await.unwrap();
    assert!(service.inspect(&path).unwrap().meta.includes_token);
    assert!(service.inspect(&path).unwrap().legacy_plugins.is_empty());
    for _ in 0..4 {
        service.inspect(&path).unwrap();
        service.import(&path).await.unwrap();
    }
    assert_eq!(service.safety_backups().unwrap().len(), 3);
    let credential: Value = json_file::read(
        &root
            .path()
            .join(format!("plugins/sources/{id}/credential.json")),
    )
    .unwrap()
    .unwrap();
    assert_eq!(credential["credential"]["password"], "private");
    let kv: Value = json_file::read(
        &root
            .path()
            .join(format!("plugins/sources/{id}/storage.json")),
    )
    .unwrap()
    .unwrap();
    assert_eq!(kv["stored"], "value");
}
#[tokio::test]
async fn backup_skips_v1_code_and_migrates_subsonic_connection_without_network() {
    let root = tempfile::tempdir().unwrap();
    let _host = haiku_plugins::Host::open(root.path()).await.unwrap();
    let mut files = fixture(1);
    files.insert(
        "cjrt_plugin_pkgs/subsonic.music/manifest.json".into(),
        br#"{"id":"subsonic.music","music":{"abiVersion":1}}"#.to_vec(),
    );
    files.insert(
        "cjrt_plugin_pkgs/subsonic.music/plugin.js".into(),
        b"v1 code must not run".to_vec(),
    );
    files.insert("cjrt_plugins/subsonic.music#old/storage.json".into(), br#"{"config":"{\"baseUrl\":\"http://127.0.0.1:9\",\"username\":\"listener\",\"password\":\"secret\"}"}"#.to_vec());
    files.insert("haiku_state/sources.json".into(), br#"{"version":1,"sources":[{"sourceId":"subsonic.music#old","pluginId":"subsonic.music","kind":"library","enabled":true,"addedAt":0}]}"#.to_vec());
    let path = root.path().join("v1.haikubak");
    fs::write(&path, pack(&files).unwrap()).unwrap();
    let service = service(root.path());
    assert_eq!(
        service.inspect(&path).unwrap().legacy_plugins,
        ["subsonic.music"]
    );
    let report = service.import(&path).await.unwrap();
    assert_eq!(report.legacy_plugins, ["subsonic.music"]);
    assert!(!root
        .path()
        .join("plugins/packages/subsonic.music/plugin.js")
        .exists());
    let sources: Value = json_file::read(&root.path().join("plugins/sources.json"))
        .unwrap()
        .unwrap();
    let source = sources["sources"][0]["sourceId"].as_str().unwrap();
    assert_ne!(source, "subsonic.music#old");
    let credential: Value = json_file::read(
        &root
            .path()
            .join(format!("plugins/sources/{source}/credential.json")),
    )
    .unwrap()
    .unwrap();
    assert_eq!(credential["connection"]["username"], "listener");
}
#[test]
fn backup_rejects_unsafe_paths_symlinks_corruption_duplicates_and_future_schema() {
    for name in [
        "/absolute",
        "../escape",
        "cjrt_plugins/a/../escape",
        "cjrt_plugins/a\\escape",
        "cjrt_plugins/a:C/file",
        "haiku_state/history.json",
        "unlisted",
        "cjrt_plugins//a",
    ] {
        let mut files = fixture(2);
        files.insert(name.into(), vec![]);
        assert!(decode(&pack(&files).unwrap()).is_err(), "{name}");
    }
    assert!(decode(&pack(&fixture(3)).unwrap()).is_err());
    assert!(decode(b"invalid zip").is_err());
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.add_symlink("cjrt_plugins/link", "/tmp", SimpleFileOptions::default())
        .unwrap();
    assert!(decode(&zip.finish().unwrap().into_inner()).is_err());
    let mut bytes = pack(&fixture(2)).unwrap();
    bytes.truncate(bytes.len() - 10);
    assert!(decode(&bytes).is_err());
    // ZIP writer itself rejects duplicate filenames; the decoder also checks the directory.
}
struct ReloadHooks {
    root: PathBuf,
    collections: Collections,
    calls: AtomicUsize,
    observed: Mutex<Vec<u8>>,
}
impl Hooks for ReloadHooks {
    fn flush(&self) -> BoxFuture<'_, Result<()>> {
        Box::pin(async { Ok(()) })
    }
    fn before_restore(&self) -> BoxFuture<'_, Result<()>> {
        Box::pin(async { Ok(()) })
    }
    fn after_restore(&self) -> BoxFuture<'_, Result<()>> {
        Box::pin(async {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.collections.reload()?;
            *self.observed.lock().unwrap() =
                fs::read(self.root.join("playlists.loro")).map_err(err)?;
            Ok(())
        })
    }
}
#[tokio::test]
async fn backup_failed_write_reloads_disk_state_and_rejects_on_disk_symlink() {
    let root = tempfile::tempdir().unwrap();
    let core = PlaylistCore::new();
    core.ensure_liked_playlist("Old");
    let old = core.export_snapshot();
    fs::write(root.path().join("playlists.loro"), &old).unwrap();
    let hooks = Arc::new(ReloadHooks {
        root: root.path().into(),
        collections: Collections::open(root.path()).unwrap(),
        calls: AtomicUsize::new(0),
        observed: Default::default(),
    });
    let service = Service::new(
        root.path().into(),
        Arc::new(MemorySecrets::default()),
        hooks.clone(),
    );
    let mut files = fixture(2);
    files.remove("haiku_state/likes.json");
    let next = PlaylistCore::new();
    next.ensure_liked_playlist("Restored");
    files.insert("haiku_state/playlists.loro".into(), next.export_snapshot());
    // Sorted writes reach playlists.loro before failing to replace a directory with settings.json.
    fs::create_dir(root.path().join("settings.json")).unwrap();
    let path = root.path().join("failure.haikubak");
    fs::write(&path, pack(&files).unwrap()).unwrap();
    // Safety export reads existing settings; create the failing target only after flush/build.
    fs::remove_dir(root.path().join("settings.json")).unwrap();
    service.inspect(&path).unwrap();
    struct FailBefore {
        inner: Arc<ReloadHooks>,
    }
    impl Hooks for FailBefore {
        fn flush(&self) -> BoxFuture<'_, Result<()>> {
            Box::pin(async { Ok(()) })
        }
        fn before_restore(&self) -> BoxFuture<'_, Result<()>> {
            Box::pin(async { fs::create_dir(self.inner.root.join("settings.json")).map_err(err) })
        }
        fn after_restore(&self) -> BoxFuture<'_, Result<()>> {
            self.inner.after_restore()
        }
    }
    let failing = Service::new(
        root.path().into(),
        Arc::new(MemorySecrets::default()),
        Arc::new(FailBefore {
            inner: hooks.clone(),
        }),
    );
    failing.inspect(&path).unwrap();
    assert!(failing.import(&path).await.is_err());
    assert_eq!(hooks.calls.load(Ordering::SeqCst), 1);
    assert!(hooks.collections.list().iter().any(|p| p.title == "Restored"));
    assert_eq!(
        *hooks.observed.lock().unwrap(),
        fs::read(root.path().join("playlists.loro")).unwrap()
    );
    assert_ne!(*hooks.observed.lock().unwrap(), old);
    #[cfg(unix)]
    {
        fs::remove_dir(root.path().join("settings.json")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("plugins")).unwrap();
        assert!(service
            .export(&root.path().join("unsafe.haikubak"), false)
            .await
            .is_err());
    }
}
