use futures::future::BoxFuture;
use haiku_lyrics::{
    formats,
    providers::{self, Candidate, Http, Provider, Query, Text},
    service::{Options, Origin, Service},
};
use haiku_model::Track;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

fn text(value: &str) -> Text {
    Text {
        text: value.into(),
        format: "lrc".into(),
        ..Default::default()
    }
}
struct Sources {
    local: Option<Text>,
    plugin: Option<Text>,
    calls: Arc<AtomicUsize>,
}
impl Origin for Sources {
    fn local(&self, _: Track) -> BoxFuture<'_, Result<Option<Text>, String>> {
        Box::pin(async {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.local.clone())
        })
    }
    fn plugin(&self, _: Track) -> BoxFuture<'_, Result<Option<Text>, String>> {
        Box::pin(async {
            self.calls.fetch_add(10, Ordering::SeqCst);
            Ok(self.plugin.clone())
        })
    }
}
struct Stub {
    id: String,
    raw: Text,
    delay: u64,
    calls: Arc<AtomicUsize>,
}
impl Provider for Stub {
    fn id(&self) -> &str {
        &self.id
    }
    fn search(&self, q: Query) -> BoxFuture<'_, Result<Vec<Candidate>, String>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(self.delay)).await;
            Ok(vec![Candidate {
                provider_id: self.id.clone(),
                title: q.title,
                artist: q.artist,
                inline_result: Some(self.raw.clone()),
                ..Default::default()
            }])
        })
    }
}
fn provider(id: &str, raw: &str, delay: u64, calls: Arc<AtomicUsize>) -> Arc<dyn Provider> {
    Arc::new(Stub {
        id: id.into(),
        raw: text(raw),
        delay,
        calls,
    })
}
fn origin(local: Option<Text>, plugin: Option<Text>) -> Arc<Sources> {
    Arc::new(Sources {
        local,
        plugin,
        calls: Arc::default(),
    })
}
fn track() -> Track {
    let mut t = Track::local("/song.flac");
    t.title = "Song".into();
    t
}
fn options() -> Options {
    Options {
        media_cache_enabled: false,
        ..Default::default()
    }
}

struct PreviewSource(Arc<AtomicUsize>);
impl Provider for PreviewSource {
    fn id(&self) -> &str {
        "preview"
    }
    fn search(&self, _: Query) -> BoxFuture<'_, Result<Vec<Candidate>, String>> {
        Box::pin(async { Err("Fixture offline".into()) })
    }
    fn resolve(&self, _: Candidate) -> BoxFuture<'_, Result<Option<Text>, String>> {
        Box::pin(async {
            struct Dropped(Arc<AtomicUsize>);
            impl Drop for Dropped {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
            }
            let _guard = Dropped(self.0.clone());
            std::future::pending().await
        })
    }
}

#[tokio::test(start_paused = true)]
async fn partial_search_errors_and_preview_replacement_drop_requests() {
    let dir = tempfile::tempdir().unwrap();
    let dropped = Arc::new(AtomicUsize::new(0));
    let service = Service::with_providers(
        dir.path().into(),
        vec![
            provider("ok", "[00:01]good", 0, Arc::default()),
            Arc::new(PreviewSource(dropped.clone())),
        ],
        Http::default(),
    );
    let report = service
        .search_report(Query::default(), None, options())
        .await
        .unwrap();
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.source_errors["preview"], "Fixture offline");
    let candidate = Candidate {
        provider_id: "preview".into(),
        ..Default::default()
    };
    for clear_track in [false, true] {
        let ticket = service.cancel_preview();
        let work = service.preview(candidate.clone(), options(), ticket);
        tokio::pin!(work);
        assert!(futures::poll!(&mut work).is_pending());
        if clear_track {
            service.clear(None);
        } else {
            service.cancel_preview();
        }
        assert!(work.await.is_err());
    }
    assert_eq!(dropped.load(Ordering::SeqCst), 2);
    // An invalidated ticket never starts its HTTP work.
    let ticket = service.cancel_preview();
    service.cancel_preview();
    assert!(service.preview(candidate, options(), ticket).await.is_err());
    assert_eq!(dropped.load(Ordering::SeqCst), 2);
    assert!(service.snapshot().lyrics.is_none());
}

#[test]
fn formats_krc_ttml_yrc_and_reference_scoring() {
    use std::io::Write;
    let krc = "[1000,900]<0,400,0>你<400,500,0>好";
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(krc.as_bytes()).unwrap();
    let compressed = z.finish().unwrap();
    let key = [
        0x40, 0x47, 0x61, 0x77, 0x5e, 0x32, 0x74, 0x47, 0x51, 0x36, 0x31, 0x2d, 0xce, 0xd2, 0x6e,
        0x69,
    ];
    let mut encrypted = b"krc1".to_vec();
    encrypted.extend(compressed.iter().enumerate().map(|(i, b)| b ^ key[i % 16]));
    assert_eq!(formats::decrypt_krc(&encrypted).unwrap(), krc);
    assert!(formats::decrypt_krc(b"invalid").is_err());
    let yrc = formats::parse_yrc("[1000,900](1000,400,0)你(1400,500,0)好");
    assert_eq!(yrc.lines[0].words[1].start, 1.4);
    let xml = r#"<tt xmlns="http://www.w3.org/ns/ttml" xmlns:ttm="http://www.w3.org/ns/ttml#metadata"><head><audio lyricOffset="0.25"/></head><body><div><p begin="1s" end="2s"><span><span begin="1s" end="1.4s">Hello </span><span begin="1400ms" end="2s">world</span></span><span ttm:role="x-translation">你好世界</span><span ttm:role="x-bg">background</span></p></div></body></tt>"#;
    let ttml = formats::parse_ttml(xml);
    assert_eq!(ttml.lines[0].text, "Hello world");
    assert_eq!(ttml.lines[0].translation.as_deref(), Some("你好世界"));
    assert_eq!(ttml.lines[0].words[0].start, 1.25);
    let cached = Text {
        text: xml.into(),
        format: "ttml".into(),
        ..Default::default()
    }
    .reference_text();
    assert_eq!(cached.format, "lrc");
    assert_eq!(
        cached.parsed().lines[0].translation.as_deref(),
        Some("你好世界")
    );
    assert_eq!(cached.parsed().lines[0].words.len(), 2);
    assert!(formats::parse_ttml("<tt><body>").is_empty());
    let q = Query {
        title: "Song (Official Video)".into(),
        artist: "Singer & Guest".into(),
        duration_sec: 100.,
        ..Default::default()
    };
    let c = Candidate {
        title: "Song".into(),
        artist: "Singer".into(),
        duration_ms: Some(101_000.),
        has_word_level: true,
        has_translation: true,
        ..Default::default()
    };
    assert_eq!(providers::score(&q, &c), 100);
    let mut plain = c.clone();
    plain.has_word_level = false;
    plain.has_translation = false;
    plain.scrollable = Some(false);
    assert_eq!(providers::score(&q, &plain), 50);
    let ranked = providers::rank(&q, vec![plain, c]);
    assert!(ranked[0].recommended);
    assert_eq!(ranked[0].stars, 5.);
}

#[tokio::test]
async fn priority_cache_fingerprint_manual_and_plain_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let service = Service::with_providers(
        dir.path().into(),
        vec![provider("line", "[00:01]network", 0, calls.clone())],
        Http::default(),
    );
    let t = track();
    let local = origin(Some(text("[00:01]local")), Some(text("[00:01]plugin")));
    service
        .load(t.clone(), Options::default(), local.clone())
        .await;
    assert_eq!(service.snapshot().source.as_deref(), Some("local-file"));
    assert_eq!(local.calls.load(Ordering::SeqCst), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let cache: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            dir.path()
                .join(format!("{}.json", haiku_lyrics::service::key(&t))),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(cache["savedAt"].as_u64().unwrap() > 0);
    assert!(cache["fingerprint"].is_string());
    let empty = origin(None, None);
    service
        .load(t.clone(), Options::default(), empty.clone())
        .await;
    assert_eq!(empty.calls.load(Ordering::SeqCst), 0);
    let mut edited = t.clone();
    edited.title = "Metadata edit".into();
    service
        .load(
            edited.clone(),
            Options::default(),
            origin(None, Some(text("[00:01]plugin"))),
        )
        .await;
    assert_eq!(service.snapshot().source.as_deref(), Some("local"));
    service
        .adopt(
            Candidate {
                provider_id: "manual-test".into(),
                candidate_id: "choice".into(),
                inline_result: Some(text("manual plain")),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap();
    let reopened = Service::with_providers(dir.path().into(), vec![], Http::default());
    reopened
        .load(t.clone(), options(), origin(None, None))
        .await;
    assert_eq!(
        reopened.snapshot().lyrics.unwrap().lines[0].text,
        "manual plain"
    );
    reopened.reset().unwrap();
    reopened
        .load(
            t.clone(),
            options(),
            origin(None, Some(text("plain plugin"))),
        )
        .await;
    assert!(!reopened.snapshot().lyrics.unwrap().synced);
    service.reset().unwrap();
    service
        .load(
            t,
            options(),
            origin(Some(text("plain file")), Some(text("plain plugin"))),
        )
        .await;
    assert_eq!(service.snapshot().source.as_deref(), Some("line"));
    assert!(!dir
        .path()
        .join(format!("{}.json", haiku_lyrics::service::key(&track())))
        .exists());
}

#[tokio::test(start_paused = true)]
async fn provider_order_disabled_quality_timeouts_and_generation_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let a = Arc::new(AtomicUsize::new(0));
    let b = Arc::new(AtomicUsize::new(0));
    let service = Arc::new(Service::with_providers(
        dir.path().into(),
        vec![
            provider("a", "plain", 0, a.clone()),
            provider("b", "[00:01]<00:01>word<00:02>", 0, b.clone()),
        ],
        Http::default(),
    ));
    service.load(track(), options(), origin(None, None)).await;
    assert_eq!(service.snapshot().source.as_deref(), Some("b"));
    let mut o = options();
    o.lyrics_provider_order = vec!["b".into(), "a".into()];
    service.load(track(), o.clone(), origin(None, None)).await;
    assert_eq!(a.load(Ordering::SeqCst), 1);
    assert_eq!(b.load(Ordering::SeqCst), 2);
    o.lyrics_providers_disabled = vec!["b".into()];
    service.load(track(), o, origin(None, None)).await;
    assert!(!service.snapshot().lyrics.unwrap().synced);
    let stalled = Service::with_providers(
        dir.path().into(),
        (0..4)
            .map(|i| provider(&i.to_string(), "late", 30_000, Arc::default()))
            .collect(),
        Http::default(),
    );
    let start = tokio::time::Instant::now();
    stalled.load(track(), options(), origin(None, None)).await;
    assert_eq!(start.elapsed(), Duration::from_secs(25));
    let errors = stalled.snapshot().source_errors;
    assert!(
        errors.contains_key("0")
            && errors.contains_key("1")
            && errors.contains_key("2")
            && errors.contains_key("total")
    );
    let old = stalled.clone();
    let job = tokio::spawn(async move {
        old.load(track(), Options::default(), origin(None, None))
            .await;
    });
    tokio::task::yield_now().await;
    stalled.clear(None);
    job.await.unwrap();
    assert!(stalled.snapshot().track.is_none());
    assert!(!dir
        .path()
        .join(format!("{}.json", haiku_lyrics::service::key(&track())))
        .exists());
    let old = stalled.clear(Some(track()));
    let new = stalled.clear(None);
    stalled
        .load_generation(track(), options(), origin(None, None), old)
        .await;
    assert_eq!(stalled.snapshot().generation, new);
}

/// All adapters use one loopback server; no fixture can contact an upstream host.
#[tokio::test]
async fn builtin_and_custom_http_requests_use_reference_shapes() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let root = base.clone();
    let server = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let root = root.clone();
            tokio::spawn(async move {
                let mut bytes = vec![0; 8192];
                let n = socket.read(&mut bytes).await.unwrap();
                let request = String::from_utf8_lossy(&bytes[..n]);
                let path = request.split_whitespace().nth(1).unwrap_or("");
                let body: String = if path.starts_with("/lrclib") {
                    assert!(path.contains("track_name=Song"));
                    r#"[{"id":7,"trackName":"Song","artistName":"Singer","duration":60,"syncedLyrics":"[00:01]hello"}]"#.into()
                } else if path.starts_with("/better") {
                    r#"{"ttml":"<tt><body><p begin='1s'><span begin='1s' end='2s'>hello</span></p></body></tt>"}"#.into()
                } else if path.starts_with("/song") {
                    r#"{"data":{"info":[{"hash":"fixture-hash","duration":60}]}}"#.into()
                } else if path.starts_with("/search") {
                    assert!(path.contains("hash=fixture-hash"));
                    r#"{"candidates":[{"id":"k1","accesskey":"key","song":"Song","singer":"Singer","duration":60000,"krctype":1}]}"#.into()
                } else if path.starts_with("/download") {
                    use base64::Engine;
                    serde_json::json!({"content":base64::engine::general_purpose::STANDARD.encode("[00:01]kugou")}).to_string()
                } else if path.starts_with("/plus") {
                    r#"{"type":"Word","lyrics":[{"time":1000,"text":"hello","syllabus":[{"time":1000,"duration":1000,"text":"hello"}],"translation":{"text":"你好"}}]}"#.into()
                } else if path.starts_with("/extra") {
                    r#"{"results":[]}"#.into()
                } else if path.starts_with("/apple-script") {
                    r#"const token="eyJfixture.payload.signature";"#.into()
                } else if path.starts_with("/apple-search") {
                    assert!(request
                        .to_lowercase()
                        .contains("authorization: bearer eyjfixture"));
                    r#"{"results":{"songs":{"data":[{"id":"apple1"}]}},"resources":{"songs":{"apple1":{"attributes":{"name":"Song","artistName":"Singer","durationInMillis":60000}}}}}"#.into()
                } else if path.starts_with("/apple-lyrics") {
                    r#"{"elrc":"[00:01]<00:01>hello<00:02>"}"#.into()
                } else if path.starts_with("/apple") {
                    r#"<script src="/assets/index~test.js"></script>"#.into()
                } else {
                    assert!(path.contains("title=Song"));
                    assert!(request.to_lowercase().contains("x-fixture: enabled"));
                    r#"{"data":{"lyrics":["[00:01]custom"]}}"#.into()
                };
                let _ = root;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            });
        }
    });
    let mut endpoints = BTreeMap::new();
    for (up, path) in [
        ("https://lrclib.net/api/search", "/lrclib"),
        ("https://lyrics-api.boidu.dev/getLyrics", "/better"),
        (
            "https://mobileservice.kugou.com/api/v3/search/song",
            "/song",
        ),
        ("https://lyrics.kugou.com/search", "/search"),
        ("https://lyrics.kugou.com/download", "/download"),
        ("https://lyricsplus.binimum.org/v2/lyrics/get", "/plus"),
        ("https://lyrics-api.binimum.org/", "/extra"),
        ("https://beta.music.apple.com", "/apple"),
        (
            "https://beta.music.apple.com/assets/index~test.js",
            "/apple-script",
        ),
        (
            "https://amp-api.music.apple.com/v1/catalog/us/search",
            "/apple-search",
        ),
        (
            "https://lyrics.paxsenix.org/apple-music/lyrics",
            "/apple-lyrics",
        ),
    ] {
        endpoints.insert(up.into(), format!("{base}{path}"));
    }
    let mut http = Http::default();
    http.endpoints = endpoints;
    let q = Query {
        title: "Song".into(),
        artist: "Singer".into(),
        duration_sec: 60.,
        ..Default::default()
    };
    for p in providers::builtins(http.clone()) {
        let list = p.search(q.clone()).await.unwrap();
        assert!(!list.is_empty(), "{}", p.id());
        let result = p.resolve(list[0].clone()).await.unwrap().unwrap();
        assert!(result.parsed().synced, "{}", p.id());
    }
    let config = providers::Custom {
        id: "custom".into(),
        name: "fixture".into(),
        url_template: format!("{base}/custom?title={{title}}"),
        response_type: "json".into(),
        json_path: "data.lyrics.0".into(),
        headers: BTreeMap::from([("X-Fixture".into(), "enabled".into())]),
    };
    let p = providers::custom(config, http);
    let result = p.search(q).await.unwrap();
    assert_eq!(
        result[0].inline_result.as_ref().unwrap().text,
        "[00:01]custom"
    );
    server.abort();
}
