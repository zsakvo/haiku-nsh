use super::*;
use haiku_store::json_file;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU16, Ordering},
    },
    time::{Duration, Instant},
};

async fn install(host: &Host, id: &str, body: &str) -> Descriptor {
    let source = format!(
        "import {{definePlugin,AuthExpired,Expired}} from 'haiku';export default definePlugin({{id:'{id}',name:'Fixture',version:'1.0.0',{body}}});"
    );
    host.install_files(
        BTreeMap::from([("plugin.ts".into(), source.into_bytes())]),
        false,
    )
    .await
    .unwrap()
}
#[tokio::test]
async fn subsonic_source_id_matches_reference_derive_source_id() {
    let dir = tempfile::tempdir().unwrap();
    let host = Host::open(dir.path()).await.unwrap();
    let descriptor = host.package("subsonic.music").unwrap().descriptor;
    let values = BTreeMap::from([
        ("baseUrl".into(), " EXAMPLE.COM:80/rest/ ".into()),
        ("username".into(), " User ".into()),
    ]);
    // Reference sources.ts deriveSourceId uses URL normalization, trimmed username,
    // identity key order baseUrl|username, and the first eight SHA-1 hex digits.
    // normalizeSourceUrl yields http://example.com, so the hash input is
    // "http://example.com|User". This golden value includes the reference plugin id.
    assert_eq!(source_id(&descriptor, &values), "subsonic.music#eeb7a0de");
}

#[tokio::test]
async fn descriptor_categories_identity_and_registry_compatibility() {
    let dir = tempfile::tempdir().unwrap();
    let host = Host::open(dir.path()).await.unwrap();
    let descriptor = host.package("subsonic.music").unwrap().descriptor;
    assert_eq!(descriptor.kind, "library");
    assert!(descriptor.capabilities.contains(&"track.streams".into()));
    assert_eq!(descriptor.library_cards.len(), 3);
    for input in [
        " EXAMPLE.COM:80/rest/ ",
        "http://example.com/app",
        "HTTP://EXAMPLE.COM/",
    ] {
        assert_eq!(normalize_source_url(input), "http://example.com");
    }
    let values = BTreeMap::from([
        ("baseUrl".into(), " EXAMPLE.COM:80/rest/ ".into()),
        ("username".into(), " User ".into()),
    ]);
    let normalized = BTreeMap::from([
        ("baseUrl".into(), "http://example.com".into()),
        ("username".into(), "User".into()),
    ]);
    assert_eq!(
        source_id(&descriptor, &values),
        source_id(&descriptor, &normalized)
    );
    assert_ne!(
        source_id(&descriptor, &values),
        source_id(
            &descriptor,
            &BTreeMap::from([
                ("baseUrl".into(), "http://example.com".into()),
                ("username".into(), "user".into())
            ])
        )
    );
    assert_eq!(source_id(&descriptor, &values).len(), 23);
    let provider = install(
        &host,
        "provider.test",
        "track:{streams:async()=>[]},scrape:{search:async()=>[]}",
    )
    .await;
    assert_eq!(provider.kind, "provider");
    assert!(provider.scraper);
    let scraper = install(&host, "scraper.test", "scrape:{search:async()=>[]}").await;
    assert_eq!(scraper.kind, "scraper");
    let file = dir.path().join("plugins/sources.json");
    json_file::write(&file,&json!({"version":1,"future":"keep","sources":[{"sourceId":"provider.test","pluginId":"provider.test","kind":"provider","enabled":false,"addedAt":123,"displayName":"Name","futureField":7}]})).unwrap();
    drop(host);
    let host = Host::open(dir.path()).await.unwrap();
    host.rename("provider.test", Some("Renamed".into()))
        .await
        .unwrap();
    let saved: Value = json_file::read(&file).unwrap().unwrap();
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["future"], "keep");
    assert_eq!(saved["sources"][0]["futureField"], 7);
    assert_eq!(saved["sources"][0]["addedAt"], 123);
}
fn zip_files(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        for (name, data) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }
    cursor.into_inner()
}
#[tokio::test]
async fn install_limits_traversal_descriptor_and_activation_rollback() {
    let root = tempfile::tempdir().unwrap();
    let host = Host::open(root.path()).await.unwrap();
    assert!(
        host.install_zip(&vec![0; 10 * 1024 * 1024 + 1])
            .await
            .is_err()
    );
    assert!(
        host.install_files(
            BTreeMap::from([("huge".into(), vec![0; 50 * 1024 * 1024 + 1])]),
            false
        )
        .await
        .is_err()
    );
    for path in ["../escape", "/absolute", "bad\\name", "C:escape"] {
        assert!(
            host.install_files(BTreeMap::from([(path.into(), vec![0])]), false)
                .await
                .is_err()
        );
    }
    assert!(
        host.install_zip(&zip_files(&[("../escape", b"bad")]))
            .await
            .is_err()
    );
    assert!(
        host.install_files(
            BTreeMap::from([("plugin.ts".into(), b"export default {}".to_vec())]),
            false
        )
        .await
        .is_err()
    );
    assert!(host.remove_package("subsonic.music").await.is_err());
    let body = "connection:{fields:{baseUrl:{type:'url',required:true}},identity:['baseUrl'],verify:async(conn,ctx)=>{if(conn.reject){ctx.storage.set('keep',-2);throw new Error('verify failed');}return {};}},track:{streams:async()=>[{url:'old'}]}";
    install(&host, "upgrade.test", body).await;
    let source = host
        .connect(
            "upgrade.test",
            BTreeMap::from([("baseUrl".into(), "http://example.com".into())]),
            None,
        )
        .await
        .unwrap();
    source
        .call_raw("connection.verify", vec![json!({})], json!({}), None)
        .await
        .unwrap();
    let id = &source.record.source_id;
    let kv = root
        .path()
        .join("plugins/sources")
        .join(id)
        .join("storage.json");
    json_file::write(&kv, &json!({"keep":42,"activationFail":true})).unwrap();
    let creds = root
        .path()
        .join("plugins/sources")
        .join(id)
        .join("credential.json");
    let before = std::fs::read(&creds).unwrap();
    let storage_before = std::fs::read(&kv).unwrap();
    assert!(
        host.connect(
            "upgrade.test",
            BTreeMap::from([
                ("baseUrl".into(), "http://example.com".into()),
                ("reject".into(), "true".into()),
            ]),
            None
        )
        .await
        .is_err()
    );
    assert_eq!(std::fs::read(&kv).unwrap(), storage_before);
    assert_eq!(std::fs::read(&creds).unwrap(), before);
    let broken = format!(
        "import {{definePlugin}} from 'haiku';if(localStorage.get('activationFail')){{localStorage.set('keep',-1);throw new Error('activation');}}export default definePlugin({{id:'upgrade.test',name:'New',version:'2.0.0',{body}}});"
    );
    assert!(
        host.install_zip(&zip_files(&[("plugin.ts", broken.as_bytes())]))
            .await
            .is_err()
    );
    assert_eq!(
        host.package("upgrade.test").unwrap().descriptor.version,
        "1.0.0"
    );
    assert_eq!(std::fs::read(&kv).unwrap(), storage_before);
    assert_eq!(
        source
            .call_raw("track.streams", vec![json!("x")], json!({}), None)
            .await
            .unwrap(),
        json!([{"url":"old"}])
    );
    let valid = format!(
        "import {{definePlugin}} from 'haiku';export default definePlugin({{id:'upgrade.test',name:'New',version:'2.0.0',{body}}});"
    );
    host.install_zip(&zip_files(&[("plugin.ts", valid.as_bytes())]))
        .await
        .unwrap();
    assert_eq!(std::fs::read(creds).unwrap(), before);
    assert_eq!(json_file::read::<Value>(&kv).unwrap().unwrap()["keep"], 42);
    assert!(host.remove_package("upgrade.test").await.is_err());
}
#[tokio::test]
async fn auth_single_flight_refresh_replay_and_validation() {
    let dir = tempfile::tempdir().unwrap();
    let host = Host::open(dir.path()).await.unwrap();
    install(&host,"auth.test",r#"auth:{password:{submit:async()=>({credential:{token:'old',refresh:'keep'}})},user:async()=>null,refresh:async(_,ctx)=>{ctx.storage.set('refreshes',(ctx.storage.get('refreshes')||0)+1);return {token:'fresh'};}},track:{streams:async(id,ctx)=>{if(ctx.credential?.token!=='fresh')throw new AuthExpired();return [{url:'https://example.invalid/'+id}];}},search:{track:async()=>({items:[{id:'good',title:'Good',artists:[],cover:'https://cover/{size}'},{id:'bad'}]})}"#.replace("refresh:async(_,ctx)","refresh:async(ctx)").as_str()).await;
    let mut draft = host.begin_login("auth.test").await.unwrap();
    draft.password(BTreeMap::new(), None).await.unwrap();
    let source = draft.commit().await.unwrap();
    let (a, b, c) = tokio::join!(
        source.invoke("track.streams", vec![json!("a")], json!({}), None),
        source.invoke("track.streams", vec![json!("b")], json!({}), None),
        source.invoke("track.streams", vec![json!("c")], json!({}), None)
    );
    for result in [a, b, c] {
        assert!(result.is_ok(), "{result:?}");
    }
    let storage: Value =
        json_file::read(&dir.path().join("plugins/sources/auth.test/storage.json"))
            .unwrap()
            .unwrap();
    assert_eq!(storage["refreshes"], 1);
    let creds: Value =
        json_file::read(&dir.path().join("plugins/sources/auth.test/credential.json"))
            .unwrap()
            .unwrap();
    assert_eq!(creds["credential"]["refresh"], "keep");
    assert_eq!(creds["credential"]["token"], "fresh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.path().join("plugins/sources/auth.test/credential.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let api = SourceApi::new(source);
    let page = api
        .search(
            SearchKind::Track,
            "q",
            PageArgs {
                cursor: None,
                limit: 20,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].cover.as_deref(), Some("https://cover/512"));
    assert_eq!(api.take_warnings().len(), 1);
    install(&host,"expired.test","track:{streams:async(id,ctx)=>{if(!ctx.storage.get('once')){ctx.storage.set('once',true);throw new Expired();}return [{url:'fresh'}];}},auth:{password:{submit:async()=>({credential:{ok:true}})},user:async()=>null}").await;
    let mut draft = host.begin_login("expired.test").await.unwrap();
    draft.password(BTreeMap::new(), None).await.unwrap();
    let api = SourceApi::new(draft.commit().await.unwrap());
    assert_eq!(
        api.streams("x", "standard", None, None).await.unwrap()[0].url,
        "fresh"
    );
}
#[tokio::test]
async fn qrcode_cadence_expiry_cancel_and_failed_draft_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let host = Host::open(dir.path()).await.unwrap();
    install(&host,"qr.test",r#"auth:{user:async()=>null,qrcode:{start:async(v)=>({state:{v},expiresIn:v==='expire'?0.01:60,image:'data:'}),poll:async(state,ctx)=>{const n=ctx.storage.get('polls')||0;ctx.storage.set('polls',n+1);return n?{status:'done',credential:{token:'qr'}}:{status:'scanned'};}},password:{submit:async()=>{throw new Error('bad login');}}},track:{streams:async()=>[]}"#).await;
    let mut draft = host.begin_login("qr.test").await.unwrap();
    assert!(draft.password(BTreeMap::new(), None).await.is_err());
    assert!(host.records().is_empty());
    let mut expired = draft
        .qrcode_start("expire", Cancellation::default())
        .await
        .unwrap();
    assert_eq!(
        draft.qrcode_poll(&mut expired).await.unwrap()["status"],
        "expired"
    );
    let cancel = Cancellation::default();
    let mut session = draft.qrcode_start("qr", cancel.clone()).await.unwrap();
    cancel.cancel();
    let start = Instant::now();
    assert_eq!(
        draft.qrcode_poll(&mut session).await.unwrap_err().code,
        "CANCELLED"
    );
    assert!(start.elapsed() < Duration::from_millis(100));
    let mut session = draft
        .qrcode_start("qr", Cancellation::default())
        .await
        .unwrap();
    let start = Instant::now();
    assert_eq!(
        draft.qrcode_poll(&mut session).await.unwrap()["status"],
        "scanned"
    );
    assert!(start.elapsed() >= Duration::from_secs(2));
    assert_eq!(
        draft.qrcode_poll(&mut session).await.unwrap()["status"],
        "done"
    );
    draft.commit().await.unwrap();
    assert_eq!(host.records().len(), 1);
    let draft = host.begin_login("qr.test").await.unwrap();
    let path = draft.dir.as_ref().unwrap().path().to_owned();
    drop(draft);
    assert!(!path.exists());
}

struct Server {
    url: String,
    status: Arc<AtomicU16>,
    requests: Arc<Mutex<Vec<String>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let status = Arc::new(AtomicU16::new(200));
        let state = status.clone();
        let requests = Arc::new(Mutex::new(vec![]));
        let log = requests.clone();
        let thread = std::thread::spawn(move || {
            let replies: Value = serde_json::from_str(include_str!(
                "../../../../plugins/subsonic/fixtures/replies.json"
            ))
            .unwrap();
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut data = [0; 8192];
                let n = stream.read(&mut data).unwrap();
                let text = String::from_utf8_lossy(&data[..n]).to_string();
                if text.contains("/stop") {
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    break;
                }
                let method = text
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .split('?')
                    .next()
                    .unwrap()
                    .rsplit('/')
                    .next()
                    .unwrap();
                log.lock().unwrap().push(text.clone());
                let code = state.load(Ordering::Acquire);
                let body = if method == "cookieGet" {
                    json!({"persisted":text.contains("session=persisted")}).to_string()
                } else if code == 200 {
                    replies[method].to_string()
                } else {
                    "{}".into()
                };
                let slow = text.contains("query=slow");
                let sets_cookie = method == "cookieSet";
                std::thread::spawn(move || {
                    if slow {
                        std::thread::sleep(Duration::from_millis(500));
                    }
                    let cookie = if sets_cookie {
                        "Set-Cookie: session=persisted; Path=/\r\n"
                    } else {
                        ""
                    };
                    let header = format!(
                        "HTTP/1.1 {code} Result\r\nContent-Length: {}\r\nRetry-After: 30\r\n{cookie}Connection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(body.as_bytes());
                });
            }
        });
        Self {
            url,
            status,
            requests,
            thread: Some(thread),
        }
    }
    async fn stop(mut self) {
        reqwest::get(format!("{}/stop", self.url)).await.unwrap();
        self.thread.take().unwrap().join().unwrap();
    }
}
#[tokio::test]
async fn subsonic_fixture_verify_search_details_streams_paging_errors_cancel() {
    let server = Server::start();
    let dir = tempfile::tempdir().unwrap();
    let host = Host::open(dir.path()).await.unwrap();
    let values = BTreeMap::from([
        ("baseUrl".into(), server.url.clone()),
        ("username".into(), "User".into()),
        ("password".into(), "secret".into()),
    ]);
    server.status.store(401, Ordering::Release);
    assert_eq!(
        host.connect("subsonic.music", values.clone(), None)
            .await
            .err()
            .unwrap()
            .code,
        "AUTH_REQUIRED"
    );
    assert!(host.records().is_empty());
    server.status.store(200, Ordering::Release);
    let source = host.connect("subsonic.music", values, None).await.unwrap();
    assert_eq!(
        source.redacted_connection().unwrap()["password"],
        "[REDACTED]"
    );
    let api = SourceApi::new(source.clone());
    let page = api
        .search(
            SearchKind::Track,
            "fixture",
            PageArgs {
                limit: 1,
                cursor: None,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].title, "Fixture Song");
    assert_eq!(page.next.as_deref(), Some("1"));
    let encoded = page.items[0].reference.as_ref().unwrap().encode();
    assert_eq!(
        haiku_model::EntityRef::decode(&encoded).unwrap(),
        page.items[0].reference.clone().unwrap()
    );
    assert!(page.items[0].cover.as_ref().unwrap().contains("size=512"));
    for kind in [SearchKind::Album, SearchKind::Artist] {
        assert_eq!(
            api.search(
                kind,
                "fixture",
                PageArgs {
                    limit: 20,
                    cursor: None
                },
                None
            )
            .await
            .unwrap()
            .items
            .len(),
            1
        );
    }
    let album = api
        .detail(
            "album",
            "album1",
            PageArgs {
                limit: 1,
                cursor: None,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(album.tracks.items[0].title, "One");
    assert_eq!(album.tracks.next.as_deref(), Some("1"));
    let next = api
        .detail(
            "album",
            "album1",
            PageArgs {
                limit: 1,
                cursor: album.tracks.next,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(next.tracks.items[0].title, "Two");
    assert!(next.tracks.next.is_none());
    assert_eq!(
        api.streams("song1", "lossless", None, None).await.unwrap()[0]
            .format
            .as_deref(),
        Some("flac")
    );
    assert_eq!(
        api.lyrics("song1", None).await.unwrap().unwrap().format,
        "lrc"
    );
    assert_eq!(api.library_cards().unwrap().len(), 3);
    for (kind, id) in [("artist", "artist1"), ("playlist", "playlist1")] {
        assert!(
            api.detail(
                kind,
                id,
                PageArgs {
                    limit: 20,
                    cursor: None
                },
                None
            )
            .await
            .unwrap()
            .info
            .is_some()
        );
    }
    assert_eq!(
        api.artist_albums(
            "artist1",
            PageArgs {
                limit: 20,
                cursor: None
            },
            None
        )
        .await
        .unwrap()
        .items
        .len(),
        1
    );
    for (card, kind) in [
        ("albums", "album"),
        ("artists", "artist"),
        ("playlists", "playlist"),
    ] {
        assert_eq!(
            api.browse(
                card,
                kind,
                PageArgs {
                    limit: 20,
                    cursor: None
                },
                None,
                None
            )
            .await
            .unwrap()
            .items
            .len(),
            1
        );
    }
    server.status.store(401, Ordering::Release);
    assert_eq!(
        api.search(
            SearchKind::Track,
            "expired",
            PageArgs {
                limit: 1,
                cursor: None
            },
            None
        )
        .await
        .unwrap_err()
        .code,
        "AUTH_EXPIRED"
    );
    assert!(source.needs_login());
    for (status, expected) in [
        (404, "NOT_FOUND"),
        (429, "RATE_LIMITED"),
        (503, "UPSTREAM_ERROR"),
    ] {
        server.status.store(status, Ordering::Release);
        let error = api
            .search(
                SearchKind::Track,
                "failure",
                PageArgs {
                    limit: 1,
                    cursor: None,
                },
                None,
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, expected);
        if status == 429 {
            assert_eq!(error.details.unwrap()["retryAfter"], 30);
        }
    }
    server.status.store(200, Ordering::Release);
    let cancellation = Cancellation::default();
    let cancel = cancellation.clone();
    let task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancel.cancel();
    });
    let start = Instant::now();
    assert_eq!(
        api.search(
            SearchKind::Track,
            "slow",
            PageArgs {
                limit: 1,
                cursor: None
            },
            Some(&cancellation)
        )
        .await
        .unwrap_err()
        .code,
        "CANCELLED"
    );
    assert!(start.elapsed() < Duration::from_millis(300));
    task.await.unwrap();
    assert!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.contains("t=") && r.contains("s=") && !r.contains("secret"))
    );
    server.stop().await;
}

#[tokio::test]
async fn sdk_models_home_links_and_complete_lists() {
    let dir = tempfile::tempdir().unwrap();
    let host = Host::open(dir.path()).await.unwrap();
    install(&host, "models.test", r#"auth:{password:{submit:async()=>({credential:{ok:true}})},user:async()=>null},
      search:{artist:async()=>({items:[{name:'Anonymous'},{id:'a',name:'Named'}]})},
      playlist:{get:async()=>({info:{id:'p',title:'Playlist',creator:'Owner'},tracks:{items:[]}})},
      program:{get:async()=>({info:{id:'p',title:'Program',host:'Host',episodeCount:3},tracks:{items:[]}})},
      toplist:{get:async()=>({info:{id:'t',title:'Top',subtitle:'Weekly'},tracks:{items:[]}})},
      home:async()=>({shelves:[
        {id:'tracks',title:'Tracks',type:'tracks',items:[{id:'t',title:'Track',artists:[]},{id:'bad'}],more:{album:'a'}},
        ...['albums','playlists','toplists','programs'].map(type=>({id:type,title:type,type,items:[{id:'e',title:'Entity'}]})),
        {id:'entries',title:'Entries',type:'entries',items:[{title:'Go',cover:'cover/{size}',to:{radio:'r'}},{title:'Bad',to:{radio:1}}],more:{bad:'x'}},
        {id:'bad',title:'Bad',type:'unknown',items:[]},{title:'Missing id'}
      ]})"#).await;
    let mut login = host.begin_login("models.test").await.unwrap();
    login.password(BTreeMap::new(), None).await.unwrap();
    let api = SourceApi::new(login.commit().await.unwrap());
    let artists = api
        .search(SearchKind::Artist, "", PageArgs::default(), None)
        .await
        .unwrap();
    assert!(artists.items[0].reference.is_none());
    assert!(artists.items[1].reference.is_some());
    for (kind, expected) in [
        ("playlist", "Owner"),
        ("program", "Host"),
        ("toplist", "Weekly"),
    ] {
        let info = api
            .detail(kind, "id", PageArgs::default(), None)
            .await
            .unwrap()
            .info
            .unwrap();
        assert_eq!(
            info.creator.or(info.host).or(info.subtitle).as_deref(),
            Some(expected)
        );
        if kind == "program" {
            assert_eq!(info.episode_count, Some(3));
        }
    }
    let home = api.home(None, false, None).await.unwrap();
    assert_eq!(home.shelves.len(), 6);
    assert_eq!(home.shelves[0].items.len(), 1);
    assert_eq!(
        home.shelves[0].more.as_ref().unwrap().reference.kind,
        "album"
    );
    assert_eq!(home.shelves[5].entries.len(), 1);
    assert_eq!(home.shelves[5].entries[0].to.reference.kind, "radio");
    assert_eq!(
        home.shelves[5].entries[0].cover.as_deref(),
        Some("cover/512")
    );
    assert!(home.shelves[5].more.is_none());
    assert_eq!(api.take_warnings().len(), 5);
    let all = SourceApi::collect_all(|cursor| async move {
        let n = cursor.as_deref().unwrap_or("0").parse::<u32>().unwrap();
        Ok(Page {
            items: vec![n],
            next: (n < 2).then(|| (n + 1).to_string()),
            total: None,
        })
    })
    .await
    .unwrap();
    assert_eq!(all, vec![0, 1, 2]);
    let mut count = 0;
    let error = SourceApi::collect_all(|_| {
        count += 1;
        let n = count;
        async move {
            Ok(Page::<()> {
                items: vec![],
                next: Some(n.to_string()),
                total: None,
            })
        }
    })
    .await
    .unwrap_err();
    assert_eq!(count, 100);
    assert_eq!(error.code, "PAGE_LIMIT");
}

#[tokio::test]
async fn auth_metadata_sms_cooldown_web_logout_refresh_failure_and_qr_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let host = Host::open(dir.path()).await.unwrap();
    let server = Server::start();
    install(&host, "sessions.test", r#"auth:{
      user:async()=>null,
      password:{fields:{password:{type:'secret',label:'Password'}},submit:async()=>({credential:{token:'password'}})},
      cookie:{submit:async(cookie)=>({credential:{token:cookie}})},
      sms:{send:async(phone)=>({state:{phone},cooldown:0.02}),verify:async(state,code)=>{if(state.phone!=='456'||code!=='ok')throw new Error('bad SMS');return {credential:{token:'sms'}};}},
      web:{url:'https://login.invalid',userAgent:'Fixture',done:async(page)=>page.url.endsWith('/done')?{credential:{token:'web'}}:null},
      qrcode:{variants:{qr:'QR'},start:async()=>({state:{},expiresIn:2.04}),poll:async(_,ctx)=>{await ctx.http.get('SLOW_FIXTURE/search3?query=slow').response();return {status:'done',credential:{token:'late'}};}},
      refresh:async()=>({token:'still expired'}),logout:async()=>{}
    },track:{streams:async()=>{throw new AuthExpired();}}"#.replace("SLOW_FIXTURE", &server.url).as_str()).await;
    let descriptor = host.package("sessions.test").unwrap().descriptor;
    let auth = descriptor.auth.unwrap();
    assert_eq!(auth["qrcode"]["variants"]["qr"], "QR");
    assert_eq!(auth["password"]["fields"]["password"]["type"], "secret");
    assert_eq!(auth["web"]["url"], "https://login.invalid");
    assert_eq!(auth["web"]["userAgent"], "Fixture");
    assert!(auth["sms"].is_object());
    let mut draft = host.begin_login("sessions.test").await.unwrap();
    draft.password(BTreeMap::new(), None).await.unwrap();
    draft.cookie("cookie", None).await.unwrap();
    assert!(draft.sms_verify("ok", None).await.is_err());
    draft.sms_send("123", None).await.unwrap();
    assert_eq!(
        draft.sms_send("456", None).await.unwrap_err().code,
        "RATE_LIMITED"
    );
    tokio::time::sleep(Duration::from_millis(25)).await;
    draft.sms_send("456", None).await.unwrap();
    assert_eq!(
        draft.sms_verify("ok", None).await.unwrap().credential["token"],
        "sms"
    );
    assert!(
        draft
            .web_done(json!({"url":"https://login.invalid/wait"}), None)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        draft
            .web_done(json!({"url":"https://login.invalid/done"}), None)
            .await
            .unwrap()
            .unwrap()
            .credential["token"],
        "web"
    );
    let source = draft.commit().await.unwrap();
    assert_eq!(
        source
            .invoke("track.streams", vec![json!("t")], json!({}), None)
            .await
            .unwrap_err()
            .code,
        "AUTH_EXPIRED"
    );
    assert!(source.needs_login());
    source.logout().await.unwrap();
    let saved: Value = json_file::read(
        &dir.path()
            .join("plugins/sources/sessions.test/credential.json"),
    )
    .unwrap()
    .unwrap();
    assert!(saved["credential"].is_null());
    let mut draft = host.begin_login("sessions.test").await.unwrap();
    let mut qr = draft
        .qrcode_start("qr", Cancellation::default())
        .await
        .unwrap();
    let start = Instant::now();
    assert_eq!(
        draft.qrcode_poll(&mut qr).await.unwrap()["status"],
        "expired"
    );
    assert!(start.elapsed() < Duration::from_millis(2600));
    assert!(!draft.authenticated);
    let cancel = Cancellation::default();
    let mut restarted = draft.qrcode_start("qr", cancel.clone()).await.unwrap();
    cancel.cancel();
    assert_eq!(
        draft.qrcode_poll(&mut restarted).await.unwrap_err().code,
        "CANCELLED"
    );
    server.stop().await;
}

#[tokio::test]
async fn startup_isolates_bad_descriptors_and_recovers_interrupted_install() {
    let dir = tempfile::tempdir().unwrap();
    let host = Host::open(dir.path()).await.unwrap();
    for id in ["healthy.test", "broken.test"] {
        install(&host, id, "auth:{user:async()=>null,password:{submit:async()=>({credential:{ok:true}})}},track:{streams:async()=>[]}").await;
        let mut draft = host.begin_login(id).await.unwrap();
        draft.password(BTreeMap::new(), None).await.unwrap();
        draft.commit().await.unwrap();
    }
    let root = dir.path().join("plugins");
    let backup = root.join("packages/.backup-crashed");
    std::fs::create_dir_all(&backup).unwrap();
    std::fs::rename(root.join("packages/healthy.test"), backup.join("package")).unwrap();
    std::fs::create_dir_all(root.join("packages/healthy.test")).unwrap();
    std::fs::write(
        root.join("packages/healthy.test/plugin.ts"),
        "broken candidate",
    )
    .unwrap();
    json_file::write(&backup.join("storage-0"), &json!({"keep":42})).unwrap();
    json_file::write(
        &root.join("sources/healthy.test/storage.json"),
        &json!({"keep":-1}),
    )
    .unwrap();
    json_file::write(&backup.join("journal.json"), &json!({"id":"healthy.test","existed":true,"storage":[["healthy.test",true]],"committed":false})).unwrap();
    std::fs::create_dir(root.join("packages/.stage-crashed")).unwrap();
    std::fs::create_dir(root.join("sources/.login-abandoned")).unwrap();
    std::fs::write(
        root.join("packages/broken.test/descriptor.json"),
        "bad json",
    )
    .unwrap();
    drop(host);
    let host = Host::open(dir.path()).await.unwrap();
    assert!(host.source("healthy.test").is_ok());
    assert!(host.source("broken.test").is_err());
    assert!(host.source_errors().contains_key("broken.test"));
    assert_eq!(
        host.packages()
            .unwrap()
            .iter()
            .filter(|p| p.descriptor.id == "healthy.test")
            .count(),
        1
    );
    assert_eq!(
        json_file::read::<Value>(&root.join("sources/healthy.test/storage.json"))
            .unwrap()
            .unwrap()["keep"],
        42
    );
    assert!(!backup.exists());
    assert!(!root.join("packages/.stage-crashed").exists());
    assert!(!root.join("sources/.login-abandoned").exists());
}

#[tokio::test]
async fn persistent_cookie_jar_survives_source_reload() {
    let server = Server::start();
    let dir = tempfile::tempdir().unwrap();
    let host = Host::open(dir.path()).await.unwrap();
    install(&host, "cookies.test", &format!(r#"http:{{cookies:true}},auth:{{user:async()=>null,cookie:{{submit:async(cookie,ctx)=>{{await ctx.http.get('{}/cookieSet').response();return {{credential:{{cookie}}}};}}}}}},track:{{get:async(id,ctx)=>ctx.http.get('{}/cookieGet').json(),streams:async()=>[]}}"#, server.url, server.url)).await;
    let mut draft = host.begin_login("cookies.test").await.unwrap();
    draft.cookie("session", None).await.unwrap();
    draft.commit().await.unwrap();
    host.set_enabled("cookies.test", false).await.unwrap();
    host.set_enabled("cookies.test", true).await.unwrap();
    assert_eq!(
        host.source("cookies.test")
            .unwrap()
            .invoke("track.get", vec![json!("t")], json!({}), None)
            .await
            .unwrap()["persisted"],
        true
    );
    server.stop().await;
}

#[tokio::test]
async fn builtin_offline_contracts_validate_capabilities_ids_pages_errors_and_abort() {
    for name in ["subsonic", "feiniu", "qq-scraper", "kugou-scraper"] {
        let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../plugins")
            .join(name);
        let report = crate::contract::run_contract(&package, crate::contract::ContractMode::Replay)
            .await
            .unwrap();
        assert!(
            report.passed,
            "{}\n{}",
            report.summary(),
            serde_json::to_string_pretty(&report).unwrap()
        );
        let encoded = serde_json::to_string(&report).unwrap();
        let roundtrip: crate::contract::ContractReport = serde_json::from_str(&encoded).unwrap();
        assert_eq!(roundtrip.schema_version, 1);
        assert!(roundtrip.passed);
    }
}

#[test]
fn contract_schema_rejects_missing_required_fields_and_nonserializable_cursors() {
    use crate::contract::validate_schema;
    assert!(validate_schema("page:track", &json!({"items":[{"id":"t","title":"Title"}]})).is_err());
    assert!(validate_schema("page:album", &json!({"items":[],"next":{}})).is_err());
    assert!(
        validate_schema(
            "candidates",
            &json!([{"id":"x","title":"Title","artists":[]}])
        )
        .is_ok()
    );
    assert!(validate_schema("streams", &json!([{"url":"bad"}])).is_err());
}

#[tokio::test]
async fn scrape_orchestration_merges_sources_scores_and_aborts_timeouts() {
    let dir = tempfile::tempdir().unwrap();
    let host = Host::open(dir.path()).await.unwrap();
    let query = crate::ScrapeQuery {
        title: "Fixture".into(),
        artists: vec!["Artist".into()],
        album: None,
        duration: Some(120.),
    };
    let mut sources = Vec::new();
    for id in ["scraper.a", "scraper.b"] {
        install(&host, id, "scrape:{search:async()=>[{id:'native',title:'Fixture',artists:['Artist'],duration:120}]}").await;
        sources.push(host.activate(id).await.unwrap());
    }
    let report = crate::scraper::Scraper::default()
        .search(query.clone(), sources, Cancellation::default())
        .await
        .unwrap();
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].source_count, 2);
    assert_eq!(report.candidates[0].score, 1.);
    assert_eq!(report.verdict, crate::scraper::Verdict::Auto);
    install(
        &host,
        "scraper.slow",
        "scrape:{search:async()=>{await new Promise(()=>{});return [];}}",
    )
    .await;
    let started = Instant::now();
    let error = crate::scraper::Scraper::default()
        .search(
            query,
            vec![host.activate("scraper.slow").await.unwrap()],
            Cancellation::default(),
        )
        .await
        .unwrap_err();
    assert!(error.message.contains("TIMEOUT"));
    assert!(started.elapsed() >= Duration::from_secs(8));
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn contract_runs_auth_expired_refresh_and_replay() {
    let package = tempfile::tempdir().unwrap();
    std::fs::create_dir(package.path().join("fixtures")).unwrap();
    std::fs::write(package.path().join("plugin.ts"), r#"
      import {definePlugin,AuthExpired} from 'haiku';
      export default definePlugin({id:'contract.auth',name:'Refresh',version:'1.0.0',
        auth:{user:async()=>null,refresh:async(ctx)=>({token:'fresh'})},
        track:{streams:async(id,ctx)=>{if(ctx.credential?.token!=='fresh')throw new AuthExpired();return [{url:'https://example.invalid/stream'}];}}
      });"#).unwrap();
    json_file::write(
        &package.path().join("fixtures/contract.json"),
        &json!({"cases":[{"name":"refresh and replay","path":"track.streams","args":["id"]}]}),
    )
    .unwrap();
    let report =
        crate::contract::run_contract(package.path(), crate::contract::ContractMode::Replay)
            .await
            .unwrap();
    assert!(
        report.passed,
        "{}",
        serde_json::to_string_pretty(&report).unwrap()
    );
}

#[tokio::test]
async fn contract_intercepts_bare_fetch_and_traverses_detail_tracks() {
    let package = tempfile::tempdir().unwrap();
    std::fs::create_dir(package.path().join("fixtures")).unwrap();
    std::fs::write(package.path().join("plugin.ts"), r#"
      import {definePlugin} from 'haiku';
      export default definePlugin({id:'contract.bare',name:'Bare fetch',version:'1.0.0',
        album:{get:async(id,ctx)=>{
          const response = await fetch('https://must-never-resolve.invalid/detail?page='+(ctx.cursor||'1'));
          return response.json();
        }}
      });"#).unwrap();
    let fixture = json!({"routes":[
      {"path":"/detail","query":{"page":"1"},"body":{"info":{"id":"a","title":"Album","artists":[]},"tracks":{"items":[{"id":"t1","title":"One","artists":[]}],"next":"2"}}},
      {"path":"/detail","query":{"page":"2"},"body":{"info":{"id":"a","title":"Album","artists":[]},"tracks":{"items":[{"id":"t2","title":"Two","artists":[]}]}}}
    ],"cases":[{"name":"detail pages via bare fetch","path":"album.get","args":["a"]}]});
    let file = package.path().join("fixtures/contract.json");
    json_file::write(&file, &fixture).unwrap();
    let report = contract::run_contract(package.path(), contract::ContractMode::Replay)
        .await
        .unwrap();
    assert!(
        report.passed,
        "{}",
        serde_json::to_string_pretty(&report).unwrap()
    );
    // A duplicate that exists only on page two must fail the very same case.
    let mut bad = fixture;
    bad["routes"][1]["body"]["tracks"]["items"][0]["id"] = json!("t1");
    json_file::write(&file, &bad).unwrap();
    let report = contract::run_contract(package.path(), contract::ContractMode::Replay)
        .await
        .unwrap();
    assert!(!report.passed);
    assert!(
        report
            .cases
            .iter()
            .any(|c| c.error.as_deref().is_some_and(|e| e.contains("Duplicate")))
    );
}
