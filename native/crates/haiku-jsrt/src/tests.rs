use super::*;
use serde_json::json;

const SECOND: Duration = Duration::from_secs(1);

async fn loaded(source: &str) -> Runtime {
    let runtime = Runtime::spawn().unwrap();
    runtime.load_module("test.js", source).await.unwrap();
    runtime
}

#[tokio::test]
async fn sync_async_json_and_top_level_await() {
    let runtime = loaded(
        r#"
        const base = await Promise.resolve(2);
        export function sync(args) { return { n: args.n * base, nested: [true, null, "字"] }; }
        export async function asyncEcho(args) { return await Promise.resolve(args); }
    "#,
    )
    .await;
    assert_eq!(
        runtime.call("sync", r#"{"n":21}"#, SECOND).await.unwrap(),
        json!({"n":42,"nested":[true,null,"字"]})
    );
    assert_eq!(
        runtime.call("asyncEcho", "[1,2]", SECOND).await.unwrap(),
        json!([1, 2])
    );
    runtime.shutdown().unwrap();
}

#[tokio::test]
async fn stdlib_hash_buffer_and_async_jobs() {
    let runtime = loaded(
        r#"
        import { createHash, webcrypto } from 'crypto';
        import { Buffer } from 'buffer';
        import { gzip, gunzip } from 'zlib';
        export async function probe() {
            const data = Buffer.from('abc');
            const digest = await webcrypto.subtle.digest('SHA-256', data);
            const compressed = await new Promise((resolve, reject) =>
                gzip(data, (error, result) => error ? reject(error) : resolve(result)));
            const plain = await new Promise((resolve, reject) =>
                gunzip(compressed, (error, result) => error ? reject(error) : resolve(result)));
            return {
                md5: createHash('md5').update(data).digest('hex'),
                sha256: Buffer.from(digest).toString('hex'),
                text: plain.toString('utf8')
            };
        }
    "#,
    )
    .await;
    assert_eq!(
        runtime.call("probe", "null", SECOND).await.unwrap(),
        json!({
            "md5": "900150983cd24fb0d6963f7d28e17f72",
            "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "text": "abc"
        })
    );
}

#[tokio::test]
async fn preserves_exceptions_and_recovers_from_regular_errors() {
    let runtime = loaded(
        r#"
        export function fail() { throw new TypeError('bad input'); }
        export async function reject() { throw new RangeError('bad range'); }
        export function primitive() { throw 'plain exception'; }
        export function ok(args) { return args; }
        export function cyclic() { const x = {}; x.x = x; return x; }
    "#,
    )
    .await;
    let error = runtime.call("fail", "null", SECOND).await.unwrap_err();
    assert_eq!(error.name, "TypeError");
    assert_eq!(error.message, "bad input");
    assert!(error.stack.contains("test.js"), "{error:?}");
    let error = runtime.call("reject", "null", SECOND).await.unwrap_err();
    assert_eq!(error.name, "RangeError");
    assert_eq!(error.message, "bad range");
    assert_eq!(
        runtime
            .call("primitive", "null", SECOND)
            .await
            .unwrap_err()
            .message,
        "plain exception"
    );
    assert!(runtime.call("missing", "null", SECOND).await.is_err());
    assert!(runtime.call("ok", "{bad json", SECOND).await.is_err());
    assert!(runtime.call("cyclic", "null", SECOND).await.is_err());
    assert_eq!(runtime.call("ok", "42", SECOND).await.unwrap(), json!(42));
}

#[tokio::test]
async fn syntax_and_module_errors_do_not_poison_the_instance() {
    let runtime = Runtime::spawn().unwrap();
    assert!(
        runtime
            .load_module("syntax.js", "export function {")
            .await
            .is_err()
    );
    assert!(
        runtime
            .load_module("missing.js", "import 'not-installed';")
            .await
            .is_err()
    );
    runtime
        .load_module("valid.js", "export const answer = () => 42;")
        .await
        .unwrap();
    assert_eq!(
        runtime.call("answer", "null", SECOND).await.unwrap(),
        json!(42)
    );
}

#[tokio::test]
async fn pending_promise_timeout_retires_vm() {
    let runtime = loaded("export const never = () => new Promise(() => {});").await;
    let start = Instant::now();
    assert_eq!(
        runtime
            .call("never", "null", Duration::from_millis(30))
            .await
            .unwrap_err()
            .name,
        "TimeoutError"
    );
    assert!(start.elapsed() < SECOND);
    assert_eq!(
        runtime
            .call("never", "null", SECOND)
            .await
            .unwrap_err()
            .name,
        "RebuildRequired"
    );
    assert_eq!(
        runtime
            .load_module("new.js", "export const ok = () => 1;")
            .await
            .unwrap_err()
            .name,
        "RebuildRequired"
    );
    runtime.shutdown().unwrap();
}

#[tokio::test]
async fn interrupts_sync_and_async_infinite_loops() {
    for source in [
        "export function spin() { while (true) {} }",
        "export async function spin() { await Promise.resolve(); while (true) {} }",
        "export function spin() { for (;;) { try { while (true) {} } catch (_) {} } }",
        "export function spin() { return { toJSON() { while (true) {} } }; }",
        "export function spin() { return new Promise(() => { const again = () => Promise.resolve().then(again); again(); }); }",
    ] {
        let runtime = loaded(source).await;
        let start = Instant::now();
        assert_eq!(
            runtime
                .call("spin", "null", Duration::from_millis(30))
                .await
                .unwrap_err()
                .name,
            "TimeoutError"
        );
        assert!(start.elapsed() < SECOND);
        assert_eq!(
            runtime.call("spin", "null", SECOND).await.unwrap_err().name,
            "RebuildRequired"
        );
        runtime.shutdown().unwrap();
    }
}

#[tokio::test]
async fn separate_instances_have_distinct_threads_and_globals() {
    let a = loaded("globalThis.leak = 1; export const get = () => globalThis.leak;").await;
    let b = loaded("globalThis.leak = 2; export const get = () => globalThis.leak;").await;
    let c = loaded("export const get = () => typeof globalThis.leak;").await;
    assert_ne!(
        a.thread.as_ref().unwrap().thread().id(),
        b.thread.as_ref().unwrap().thread().id()
    );
    let (one, two, three) = tokio::join!(
        a.call("get", "null", SECOND),
        b.call("get", "null", SECOND),
        c.call("get", "null", SECOND),
    );
    assert_eq!(one.unwrap(), json!(1));
    assert_eq!(two.unwrap(), json!(2));
    assert_eq!(three.unwrap(), json!("undefined"));
}

#[tokio::test]
async fn same_instance_serializes_concurrent_commands() {
    let runtime = loaded("let n = 0; export async function next() { await 0; return ++n; }").await;
    let (a, b, c) = tokio::join!(
        runtime.call("next", "null", SECOND),
        runtime.call("next", "null", SECOND),
        runtime.call("next", "null", SECOND),
    );
    assert_eq!(
        [a.unwrap(), b.unwrap(), c.unwrap()],
        [json!(1), json!(2), json!(3)]
    );
}

#[tokio::test]
async fn shutdown_and_drop_join_threads_even_with_pending_work() {
    for explicit in [true, false] {
        for source in [
            "export const work = () => new Promise(() => {});",
            "export const work = () => { while (true) {} };",
        ] {
            for _ in 0..8 {
                let runtime = loaded(source).await;
                let weak = Arc::downgrade(&runtime.stop);
                let (reply, response) = oneshot::channel();
                runtime
                    .commands
                    .send(Command {
                        operation: Operation::Call {
                            export: "work".into(),
                            args: "null".into(),
                            context: "{}".into(),
                        },
                        deadline: Instant::now() + Duration::from_secs(60),
                        reply,
                        abort: AbortHandle::default(),
                    })
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(5)).await;
                let start = Instant::now();
                if explicit {
                    runtime.shutdown().unwrap();
                } else {
                    drop(runtime);
                }
                assert!(start.elapsed() < SECOND);
                assert!(
                    weak.upgrade().is_none(),
                    "worker still holds lifecycle state after join"
                );
                assert_eq!(response.await.unwrap().unwrap_err().name, "ShutdownError");
            }
        }
    }
}

#[tokio::test]
async fn job_pump_wakes_for_genuinely_pending_tokio_future() {
    // Test-only function: verifies an external Tokio wake, alongside stdlib tests.
    let mut vm = Engine::new(Arc::new(Stop::default())).await.unwrap();
    vm.context
        .with(|ctx| {
            ctx.globals()
                .set(
                    "delay",
                    rquickjs::function::Func::from(rquickjs::function::Async(async || {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        Ok::<_, rquickjs::Error>(42)
                    })),
                )
                .unwrap();
        })
        .await;
    vm.execute(Operation::Load {
        name: "delayed.js".into(),
        source: "export async function run() { return await delay(); }".into(),
    })
    .await
    .unwrap();
    let start = Instant::now();
    let result = tokio::time::timeout(
        SECOND,
        vm.execute(Operation::Call {
            export: "run".into(),
            args: "null".into(),
            context: "{}".into(),
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result, json!(42));
    assert!(start.elapsed() >= Duration::from_millis(20));
}

#[tokio::test]
async fn dropping_vm_cancels_pending_native_futures() {
    let vm = Engine::new(Arc::new(Stop::default())).await.unwrap();
    let weak = vm.runtime.weak();
    let cancelled = Arc::new(AtomicBool::new(false));
    struct Cancelled(Arc<AtomicBool>);
    impl Drop for Cancelled {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let guard = Cancelled(cancelled.clone());
    vm.context
        .with(|ctx| {
            ctx.spawn(async move {
                let _guard = guard;
                tokio::time::sleep(Duration::from_secs(60)).await;
            });
        })
        .await;
    assert!(vm.runtime.execute_pending_job().await.is_ok());
    drop(vm);
    assert!(
        weak.try_ref().is_none(),
        "runtime retained by a native future"
    );
    assert!(
        cancelled.load(Ordering::Acquire),
        "native future was not dropped"
    );
}

#[tokio::test]
async fn r1_lifecycle_and_r4_concurrent_instances() {
    let a =
        loaded("globalThis.n=0; export default { next(x){return {n:++globalThis.n,x};} };").await;
    let b =
        loaded("globalThis.n=10; export default { next(x){return {n:++globalThis.n,x};} };").await;
    let (a1, a2, b1, b2) = tokio::join!(
        a.call("default.next", "1", SECOND),
        a.call("default.next", "2", SECOND),
        b.call("default.next", "3", SECOND),
        b.call("default.next", "4", SECOND)
    );
    assert_eq!(
        [a1.unwrap(), a2.unwrap(), b1.unwrap(), b2.unwrap()],
        [
            json!({"n":1,"x":1}),
            json!({"n":2,"x":2}),
            json!({"n":11,"x":3}),
            json!({"n":12,"x":4})
        ]
    );
    a.unload().await.unwrap();
    assert!(a.call("default.next", "1", SECOND).await.is_err());
    a.shutdown().unwrap();
    b.shutdown().unwrap();
}
#[tokio::test]
async fn r2_typescript_maps_and_package_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("dep.ts"),
        "export const twice = (n: number): number => n*2;",
    )
    .unwrap();
    std::fs::write(dir.path().join("main.ts"),"import {twice} from './dep.ts';\ninterface Hidden { n: number }\n\nexport default { run(x: Hidden) { return twice(x.n); },\n fail() {\n throw new TypeError('mapped');\n } };\n").unwrap();
    let vm = Runtime::spawn().unwrap();
    vm.load_package(dir.path(), "main.ts").await.unwrap();
    assert_eq!(
        vm.call("default.run", r#"{"n":21}"#, SECOND).await.unwrap(),
        json!(42)
    );
    let error = vm.call("default.fail", "null", SECOND).await.unwrap_err();
    assert_eq!(error.line, Some(6), "{error:?}");
    assert!(error.file.unwrap().ends_with("main.ts"));
    std::fs::write(outside.path().join("secret.ts"), "export const secret=1").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.path().join("secret.ts"),
        dir.path().join("escape.ts"),
    )
    .unwrap();
    for entry in [
        "../secret.ts",
        outside.path().join("secret.ts").to_str().unwrap(),
        "escape.ts",
    ] {
        assert!(vm.load_package(dir.path(), entry).await.is_err(), "{entry}");
    }
    for import in [
        "../secret.ts",
        outside.path().join("secret.ts").to_str().unwrap(),
        "./escape.ts",
    ] {
        std::fs::write(
            dir.path().join("bad.ts"),
            format!(
                "import {{secret}} from '{}'; export const n=secret;",
                import
            ),
        )
        .unwrap();
        assert!(
            vm.load_package(dir.path(), "bad.ts").await.is_err(),
            "{import}"
        );
    }
}
fn native_factory<'js>(
    ctx: &Ctx<'js>,
    name: &str,
) -> rquickjs::Result<Module<'js, rquickjs::module::Declared>> {
    struct Native;
    impl rquickjs::module::ModuleDef for Native {
        fn declare(declarations: &rquickjs::module::Declarations<'_>) -> rquickjs::Result<()> {
            declarations.declare("add")?;
            Ok(())
        }
        fn evaluate<'js>(
            ctx: &Ctx<'js>,
            exports: &rquickjs::module::Exports<'js>,
        ) -> rquickjs::Result<()> {
            exports.export("add", Function::new(ctx.clone(), |n: i32| n + 1)?)?;
            Ok(())
        }
    }
    Module::declare_def::<Native, _>(ctx.clone(), name)
}
#[tokio::test]
async fn r3_source_and_rust_modules() {
    let mut options = RuntimeOptions::default();
    options
        .modules
        .insert("haiku".into(), "export const n=41;".into());
    options
        .native_modules
        .insert("native".into(), native_factory);
    let vm = Runtime::with_options(options).unwrap();
    vm.load_module(
        "main.js",
        "import {n} from 'haiku'; import {add} from 'native'; export const run=()=>add(n);",
    )
    .await
    .unwrap();
    assert_eq!(vm.call("run", "null", SECOND).await.unwrap(), json!(42));
}
#[tokio::test]
async fn r4_paths_this_and_structured_errors() {
    let vm=loaded("export default {n:42, nested:{fail(){throw new RangeError('nested');}},run(){return this.n;}};").await;
    assert_eq!(
        vm.call("default.run", "null", SECOND).await.unwrap(),
        json!(42)
    );
    let error = vm
        .call("default.nested.fail", "null", SECOND)
        .await
        .unwrap_err();
    assert_eq!(error.name, "RangeError");
    assert_eq!(error.file.as_deref(), Some("test.js"));
    assert!(error.line.is_some());
}
#[tokio::test]
async fn r5_cancel_async_and_cpu_then_reuse_concurrently() {
    for source in [
        "export const run=(_,ctx)=>new Promise(resolve=>ctx.signal.addEventListener('abort',()=>resolve(1))); export const ok=()=>42;",
        "export const run=()=>{while(true){}}; export const ok=()=>42;",
    ] {
        let a = loaded(source).await;
        let b = loaded(source).await;
        let first = a.start_call("run", json!(null), json!({}), SECOND).unwrap();
        let second = b.start_call("run", json!(null), json!({}), SECOND).unwrap();
        let cancel1 = first.abort.clone();
        let cancel2 = second.abort.clone();
        let cancel = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            cancel1.abort();
            cancel2.abort();
        });
        let (one, two, three, four) = tokio::join!(
            first.result(),
            second.result(),
            a.call("ok", "null", SECOND),
            b.call("ok", "null", SECOND)
        );
        cancel.join().unwrap();
        assert_eq!(one.unwrap_err().name, "AbortError");
        assert_eq!(two.unwrap_err().name, "AbortError");
        assert_eq!(three.unwrap(), json!(42));
        assert_eq!(four.unwrap(), json!(42));
    }
}
#[tokio::test]
async fn r6_fresh_context_and_signal() {
    let vm=loaded("let previous; export const run=(_,ctx)=>{ const result=[ctx.id,ctx!==previous,!!ctx.signal,typeof ctx.http,typeof ctx.storage.get,typeof ctx.log.info];previous=ctx;return result;};").await;
    for id in [1, 2] {
        assert_eq!(
            vm.start_call("run", json!(null), json!({"id":id}), SECOND)
                .unwrap()
                .result()
                .await
                .unwrap(),
            json!([id, true, true, "function", "function", "function"])
        );
    }
}

fn http_server() -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", server.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        for stream in server.incoming() {
            let mut stream = stream.unwrap();
            // Read exactly the headers and declared body, retaining raw bytes.
            let mut data = Vec::new();
            let header_end = loop {
                let mut chunk = [0; 8192];
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break data.len();
                }
                data.extend_from_slice(&chunk[..n]);
                if let Some(offset) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    break offset + 4;
                }
            };
            let request = String::from_utf8_lossy(&data[..header_end]);
            let length = request
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|n| n.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if request.contains("/stop") {
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                break;
            }
            let slow = request.contains("/slow");
            let redirect = request.contains("/redirect");
            let post = request.starts_with("POST");
            while data.len() < header_end + length {
                let mut chunk = [0; 8192];
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&chunk[..n]);
            }
            std::thread::spawn(move || {
                if slow {
                    std::thread::sleep(Duration::from_millis(150));
                }
                if redirect {
                    let _=stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: /ok\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                } else {
                    let body = if post {
                        &data[header_end..]
                    } else {
                        b"{\"ok\":true}".as_slice()
                    };
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(body);
                }
            });
        }
    });
    (url, handle)
}
async fn stop_server(url: &str, handle: std::thread::JoinHandle<()>) {
    reqwest::get(format!("{url}/stop")).await.unwrap();
    handle.join().unwrap();
}
#[tokio::test]
async fn r7_http_bodies_redirect_abort_timeout_and_concurrency() {
    let (url, server) = http_server();
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = events.clone();
    let options = RuntimeOptions {
        http_hook: Some(Arc::new(move |event| captured.lock().unwrap().push(event))),
        ..Default::default()
    };
    let a = Runtime::with_options(options.clone()).unwrap();
    let b = Runtime::with_options(options).unwrap();
    let source = r#"export async function run(url,ctx){const r=await ctx.http(url+'/redirect',{headers:{Authorization:'Bearer secret'}});return [r.status,r.redirected,await r.json(),r.headers.getSetCookie()];}
      export async function post(url){const r=await fetch(new Request(url,{method:'POST',body:Buffer.from([0,1,255])}));const clone=r.clone();return [Array.from(await r.bytes()),Array.from(new Uint8Array(await clone.arrayBuffer()))];}
      export async function slow(url,ctx){return (await ctx.http(url+'/slow')).status;}
      export async function timeout(url){return await fetch(url+'/slow',{timeout:10});}
      export async function jsAbort(url){const controller=new AbortController();const pending=fetch(url+'/slow',{signal:controller.signal});controller.abort();try{await pending;}catch(e){return e.name;}}
    "#;
    for vm in [&a, &b] {
        vm.load_module("http.js", source).await.unwrap();
    }
    let slow = a.start_call("slow", json!(url), json!({}), SECOND).unwrap();
    let abort = slow.abort.clone();
    let cancel = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        abort.abort();
    });
    let url_json = json!(url).to_string();
    let post_json = json!(format!("{url}/ok")).to_string();
    let (cancelled, one, two, three) = tokio::join!(
        slow.result(),
        a.call("run", &url_json, SECOND),
        b.call("run", &url_json, SECOND),
        b.call("post", &post_json, SECOND)
    );
    cancel.await.unwrap();
    assert_eq!(cancelled.unwrap_err().name, "AbortError");
    for result in [one, two] {
        assert_eq!(result.unwrap(), json!([200,true,{"ok":true},["a=1","b=2"]]));
    }
    assert_eq!(three.unwrap(), json!([[0, 1, 255], [0, 1, 255]]));
    assert_eq!(
        b.call("timeout", &json!(url).to_string(), SECOND)
            .await
            .unwrap_err()
            .name,
        "TimeoutError"
    );
    assert_eq!(
        b.call("jsAbort", &json!(url).to_string(), SECOND)
            .await
            .unwrap(),
        json!("AbortError")
    );
    {
        let events = events.lock().unwrap();
        assert!(
            events
                .iter()
                .any(|e| e.phase == "response" && e.status == Some(200))
        );
        assert!(
            events
                .iter()
                .filter_map(|e| e.headers.get("authorization"))
                .all(|v| v == "[REDACTED]")
        );
    }
    stop_server(&url, server).await;
}
#[tokio::test]
async fn r8_console_channel_and_redaction() {
    let logs = Arc::new(Mutex::new(Vec::new()));
    let captured = logs.clone();
    let vm = Runtime::with_options(RuntimeOptions {
        instance_id: "instance-a".into(),
        log: Some(Arc::new(move |record| {
            captured.lock().unwrap().push(record)
        })),
        ..Default::default()
    })
    .unwrap();
    vm.load_module("log.js",r#"export const run=()=>{ for(const level of ['debug','log','info','warn','error','trace']) console[level]({Authorization:'hidden',nested:{access_token:'hidden',Cookie:'hidden'}},'token=hidden');return true;};"#).await.unwrap();
    vm.call("run", "null", SECOND).await.unwrap();
    let logs = logs.lock().unwrap();
    assert_eq!(logs.len(), 6);
    assert!(logs.iter().all(|r| r.instance_id == "instance-a"
        && !r.message.contains("hidden")
        && r.message.contains("[REDACTED]")));
}
#[tokio::test]
async fn r9_atomic_persistence_across_unload_and_recreate() {
    let dir = tempfile::tempdir().unwrap();
    let options = RuntimeOptions {
        storage_path: Some(dir.path().join("kv.json")),
        ..Default::default()
    };
    let vm = Runtime::with_options(options.clone()).unwrap();
    vm.load_module(
        "set.js",
        "export const run=(_,ctx)=>{ctx.storage.set('n',{value:42});return ctx.storage.get('n');};",
    )
    .await
    .unwrap();
    assert_eq!(
        vm.call("run", "null", SECOND).await.unwrap(),
        json!({"value":42})
    );
    vm.unload().await.unwrap();
    vm.shutdown().unwrap();
    let vm = Runtime::with_options(options).unwrap();
    vm.load_module("get.js", "export const run=()=>localStorage.get('n');")
        .await
        .unwrap();
    assert_eq!(
        vm.call("run", "null", SECOND).await.unwrap(),
        json!({"value":42})
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[tokio::test]
async fn r10_nist_aes_vectors_and_node_shapes() {
    let vm=loaded(r#"import crypto,{createCipheriv,createDecipheriv} from 'crypto';import {Buffer} from 'buffer';
      export function run(){for(const name of ['createCipheriv','createDecipheriv','publicEncrypt'])if(typeof crypto[name]!=='function')throw new Error('Missing crypto.'+name);if(typeof createCipheriv!=='function'||typeof createDecipheriv!=='function')throw new Error('Missing named crypto exports');const result=[];const plain=Buffer.from('6bc1bee22e409f96e93d7e117393172a','hex');
        for(const [bits,key] of [[128,'2b7e151628aed2a6abf7158809cf4f3c'],[192,'8e73b0f7da0e6452c810f32b809079e562f8ead2522c6b7b'],[256,'603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4']]){
          for(const mode of ['ecb','cbc','ctr','gcm']){const iv=mode==='ecb'?null:Buffer.from(mode==='gcm'?'000000000000000000000000':mode==='ctr'?'f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff':'000102030405060708090a0b0c0d0e0f','hex');
            const c=crypto.createCipheriv(`aes-${bits}-${mode}`,Buffer.from(key,'hex'),iv);c.setAutoPadding(false);const enc=Buffer.concat([c.update(plain),c.final()]);
            const d=createDecipheriv(`aes-${bits}-${mode}`,Buffer.from(key,'hex'),iv);d.setAutoPadding(false);if(mode==='gcm')d.setAuthTag(c.getAuthTag());const dec=Buffer.concat([d.update(enc),d.final()]);if(dec.toString('hex')!==plain.toString('hex'))throw new Error('roundtrip');if(mode!=='gcm')result.push(enc.toString('hex'));
          }
        }
        const c=createCipheriv('aes-128-gcm',Buffer.alloc(16),Buffer.alloc(12));const encrypted=Buffer.concat([c.update(Buffer.alloc(16)),c.final()]);result.push(encrypted.toString('hex'),c.getAuthTag().toString('hex'));
        const pad=createCipheriv('aes-128-cbc',Buffer.alloc(16),Buffer.alloc(16));const enc=Buffer.concat([pad.update('hello','utf8'),pad.final()]);const dec=createDecipheriv('aes-128-cbc',Buffer.alloc(16),Buffer.alloc(16));result.push(Buffer.concat([dec.update(enc),dec.final()]).toString());return result;}
    "#).await;
    // NIST SP 800-38A F.1/F.2/F.5 first blocks; SP 800-38D test case 2.
    assert_eq!(
        vm.call("run", "null", SECOND).await.unwrap(),
        json!([
            "3ad77bb40d7a3660a89ecaf32466ef97",
            "7649abac8119b246cee98e9b12e9197d",
            "874d6191b620e3261bef6864990db6ce",
            "bd334f1d6e45f25ff712a214571fa5cc",
            "4f021db243bc633d7178183a9fa071e8",
            "1abc932417521ca24f2b0459fe7e6e0b",
            "f3eed1bdb5d2a03c064b5a7e3db181f8",
            "f58c4c04d6e5f1ba779eabfb5f7bfbd6",
            "601ec313775789a5b7a7f504bbf3d228",
            "0388dace60b6a392f328c2b971b2fe78",
            "ab6e47d42cec13bdf53a67b21257bddf",
            "hello"
        ])
    );
}
#[tokio::test]
async fn r11_memory_cpu_and_describe_only() {
    let vm = Runtime::with_options(RuntimeOptions {
        describe_only: true,
        ..Default::default()
    })
    .unwrap();
    vm.load_module(
        "description.js",
        "export const meta={id:'p'};export default {run(){while(true){}}};",
    )
    .await
    .unwrap();
    assert_eq!(
        vm.describe().await.unwrap(),
        json!({"meta":{"id":"p"},"default":{"run":"function"}})
    );
    assert_eq!(
        vm.call("default.run", "null", SECOND)
            .await
            .unwrap_err()
            .name,
        "DescribeOnly"
    );
    let vm = Runtime::with_options(RuntimeOptions {
        memory_limit: 8 * 1024 * 1024,
        ..Default::default()
    })
    .unwrap();
    vm.load_module(
        "memory.js",
        "export function run(){const a=[];while(true)a.push(new Array(10000).fill(42));}",
    )
    .await
    .unwrap();
    let error = vm.call("run", "null", SECOND).await.unwrap_err();
    assert_eq!(error.name, "MemoryLimitError", "{error:?}");
    assert_eq!(
        vm.call("run", "null", SECOND).await.unwrap_err().name,
        "RebuildRequired"
    );
    vm.unload().await.unwrap();
    let vm = loaded("export function spin(){while(true){}}").await;
    assert_eq!(
        vm.call("spin", "null", Duration::from_millis(20))
            .await
            .unwrap_err()
            .name,
        "TimeoutError"
    );
}
#[tokio::test]
async fn r12_reload_keeps_storage_and_completes_old_calls() {
    let dir = tempfile::tempdir().unwrap();
    let vm = Runtime::with_options(RuntimeOptions {
        storage_path: Some(dir.path().join("kv.json")),
        ..Default::default()
    })
    .unwrap();
    let path = dir.path().join("main.ts");
    std::fs::write(&path,"export async function run(_:unknown,ctx:any){await 0;ctx.storage.set('n',42);return 'old';}").unwrap();
    vm.load_package(dir.path(), "main.ts").await.unwrap();
    let old = vm
        .start_call("run", json!(null), json!({}), SECOND)
        .unwrap();
    std::fs::write(
        &path,
        "export function run(_:unknown,ctx:any){return ['new',ctx.storage.get('n')];}",
    )
    .unwrap();
    vm.reload(dir.path(), "main.ts").await.unwrap();
    assert_eq!(old.result().await.unwrap(), json!("old"));
    assert_eq!(
        vm.call("run", "null", SECOND).await.unwrap(),
        json!(["new", 42])
    );
}

#[tokio::test]
async fn r10_rsa_pem_padding_and_authentication_failures() {
    use rsa::{
        Oaep, Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey,
        pkcs1::EncodeRsaPublicKey,
        pkcs8::{DecodePrivateKey, EncodePublicKey, LineEnding},
    };
    // Public RustCrypto RSA test fixture (MIT/Apache-2.0), never a user credential.
    const PRIVATE: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC2xCxRXxCmqvKC
xj7b4kJDoXDz+iYzvUgzY39Hyk9vNuA6XSnvwxkayA85DYdLOeMPQU/Owfyg7YHl
R+3CzTgsdvYckBiXPbn6U3lyp8cB9rd+CYLfwV/AGSfuXnzZS09Zn/BwE6fIKBvf
Ity8mtfKu3xDEcmC9Y7bchOtRVizMiZtdDrtgZLRiEytuLFHOaja2mbclwgG2ces
RQyxPQ18V1+xmFNPxhvEG8DwV04OATDHu7+9/cn2puLj4q/xy+rIm6V4hFKNVc+w
gyeh6MifTgA88oiOkzJB2daVvLus3JC0Tj4JX6NwWOolsT9eKVy+rG3oOKuMUK9h
4piXW4cvAgMBAAECggEAfsyDYsDtsHQRZCFeIvdKudkboGkAcAz2NpDlEU2O5r3P
uy4/lhRpKmd6CD8Wil5S5ZaOZAe52XxuDkBk+C2gt1ihTxe5t9QfX0jijWVRcE9W
5p56qfpjD8dkKMBtJeRV3PxVt6wrT3ZkP97T/hX/eKuyfmWsxKrQvfbbJ+9gppEM
XEoIXtQydasZwdmXoyxu/8598tGTX25gHu3hYaErXMJ8oh+B0smcPR6gjpDjBTqw
m++nJN7w0MOjwel0DA2fdhJqFJ7Aqn2AeCBUhCVNlR2wfEz5H7ZFTAlliP1ZJNur
6zWcogJSaNAE+dZus9b3rcETm61A8W3eY54RZHN2wQKBgQDcwGEkLU6Sr67nKsUT
ymW593A2+b1+Dm5hRhp+92VCJewVPH5cMaYVem5aE/9uF46HWMHLM9nWu+MXnvGJ
mOQi7Ny+149Oz9vl9PzYrsLJ0NyGRzypvRbZ0jjSH7Xd776xQ8ph0L1qqNkfM6CX
eQ6WQNvJEIXcXyY0O6MTj2stZwKBgQDT8xR1fkDpVINvkr4kI2ry8NoEo0ZTwYCv
Z+lgCG2T/eZcsj79nQk3R2L1mB42GEmvaM3XU5T/ak4G62myCeQijbLfpw5A9/l1
ClKBdmR7eI0OV3eiy4si480mf/cLTzsC06r7DhjFkKVksDGIsKpfxIFWsHYiIUJD
vRIn76fy+QKBgQDOaLesGw0QDWNuVUiHU8XAmEP9s5DicF33aJRXyb2Nl2XjCXhh
fi78gEj0wyQgbbhgh7ZU6Xuz1GTn7j+M2D/hBDb33xjpqWPE5kkR1n7eNAQvLibj
06GtNGra1rm39ncIywlOYt7p/01dZmmvmIryJV0c6O0xfGp9hpHaNU0S2wKBgCX2
5ZRCIChrTfu/QjXA7lhD0hmAkYlRINbKeyALgm0+znOOLgBJj6wKKmypacfww8oa
sLxAKXEyvnU4177fTLDvxrmO99ulT1aqmaq85TTEnCeUfUZ4xRxjx4x84WhyMbTI
61h65u8EgMuvT8AXPP1Yen5nr1FfubnedREYOXIpAoGAMZlUBtQGIHyt6uo1s40E
DF+Kmhrggn6e0GsVPYO2ghk1tLNqgr6dVseRtYwnJxpXk9U6HWV8CJl5YLFDPlFx
mH9FLxRKfHIwbWPh0//Atxt1qwjy5FpILpiEUcvkeOEusijQdFbJJLZvbO0EjYU/
Uz4xpoYU8cPObY7JmDznKvc=
-----END PRIVATE KEY-----
"#;
    let key = RsaPrivateKey::from_pkcs8_pem(PRIVATE).unwrap();
    let public = RsaPublicKey::from(&key);
    let vm=loaded(r#"import crypto,{publicEncrypt,constants} from 'crypto';
      export function rsa(args){const options={key:args.key,padding:args.padding,oaepHash:args.hash};return Array.from(publicEncrypt(options,Buffer.from('fixture')));}
      export function invalid(){const c=crypto.createCipheriv('aes-128-gcm',Buffer.alloc(16),Buffer.alloc(12));const encrypted=Buffer.concat([c.update('authenticated'),c.final()]);const d=crypto.createDecipheriv('aes-128-gcm',Buffer.alloc(16),Buffer.alloc(12));d.setAuthTag(Buffer.alloc(16));d.update(encrypted);try{d.final();return false;}catch(e){return e.message==='Authentication failed';}}
      export function streaming(){const c=crypto.createCipheriv('aes-128-cbc',Buffer.alloc(16),Buffer.alloc(16));const first=c.update(Buffer.alloc(16));const last=c.final();const d=crypto.createDecipheriv('aes-128-cbc',Buffer.alloc(16),Buffer.alloc(16));const plain=Buffer.concat([d.update(first),d.update(last),d.final()]);return [first.length,last.length,plain.length];}
    "#).await;
    for pem in [
        public.to_public_key_pem(LineEnding::LF).unwrap(),
        public.to_pkcs1_pem(LineEnding::LF).unwrap(),
    ] {
        for (padding, hash) in [(1, "sha1"), (4, "sha1"), (4, "sha256")] {
            let result = vm
                .call(
                    "rsa",
                    &json!({"key":pem,"padding":padding,"hash":hash}).to_string(),
                    SECOND,
                )
                .await
                .unwrap();
            let encrypted: Vec<u8> = serde_json::from_value(result).unwrap();
            let plain = match (padding, hash) {
                (1, _) => key.decrypt(Pkcs1v15Encrypt, &encrypted),
                (4, "sha1") => key.decrypt(Oaep::<sha1::Sha1>::new(), &encrypted),
                _ => key.decrypt(Oaep::<sha2::Sha256>::new(), &encrypted),
            }
            .unwrap();
            assert_eq!(plain, b"fixture");
        }
    }
    assert_eq!(
        vm.call("invalid", "null", SECOND).await.unwrap(),
        json!(true)
    );
    assert_eq!(
        vm.call("streaming", "null", SECOND).await.unwrap(),
        json!([16, 16, 16])
    );
}

#[tokio::test]
async fn cipher_many_chunks_equal_single_chunk() {
    let vm=loaded(r#"import {createCipheriv,createDecipheriv} from 'crypto';export function run(){const data=Buffer.from(Array.from({length:4099},(_,i)=>i%256));for(const bits of [128,192,256])for(const mode of ['ecb','cbc','ctr','gcm']){const key=Buffer.alloc(bits/8),iv=mode==='ecb'?null:Buffer.alloc(mode==='gcm'?12:16);function encrypt(step){const c=createCipheriv(`aes-${bits}-${mode}`,key,iv);const out=[];for(let i=0;i<data.length;i+=step)out.push(c.update(data.subarray(i,i+step)));out.push(c.final());return {data:Buffer.concat(out),tag:mode==='gcm'?c.getAuthTag():null};}const full=encrypt(data.length),parts=encrypt(7);if(full.data.toString('hex')!==parts.data.toString('hex'))throw new Error('chunk mismatch');const d=createDecipheriv(`aes-${bits}-${mode}`,key,iv);if(parts.tag)d.setAuthTag(parts.tag);const plain=[];for(let i=0;i<parts.data.length;i+=11)plain.push(d.update(parts.data.subarray(i,i+11)));plain.push(d.final());if(Buffer.concat(plain).toString('hex')!==data.toString('hex'))throw new Error('decrypt mismatch');}return true;}"#).await;
    assert_eq!(
        vm.call("run", "null", Duration::from_secs(5))
            .await
            .unwrap(),
        json!(true)
    );
}

#[tokio::test]
async fn timers_repeat_clear_abort_and_unload() {
    let runtime = loaded(
        r#"
        let fired = 0;
        export async function run() {
          const removed = setTimeout(() => { fired += 100; }, 5);
          clearTimeout(removed);
          await new Promise(resolve => {
            const interval = setInterval(() => {
              if (++fired === 3) { clearInterval(interval); resolve(); }
            }, 2);
          });
          return await new Promise(resolve => setTimeout((n) => resolve(fired + n), 5, 4));
        }
        export async function pending() {
          setInterval(() => fired += 100, 60);
          await new Promise(() => {});
        }
        export async function check() {
          await new Promise(resolve => setTimeout(resolve, 90));
          return fired;
        }
    "#,
    )
    .await;
    assert_eq!(runtime.call("run", "null", SECOND).await.unwrap(), 7);
    let call = runtime
        .start_call("pending", Value::Null, json!({}), SECOND)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(15)).await;
    call.abort.abort();
    assert_eq!(call.result().await.unwrap_err().name, "AbortError");
    assert_eq!(runtime.call("check", "null", SECOND).await.unwrap(), 3);
    runtime.unload().await.unwrap();
    runtime
        .load_module(
            "fresh.js",
            "export async function run(){return await new Promise(r=>setTimeout(()=>r(42),2));}",
        )
        .await
        .unwrap();
    assert_eq!(runtime.call("run", "null", SECOND).await.unwrap(), 42);
}

#[tokio::test]
async fn package_relative_import_can_recursively_load_without_holding_source_lock() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("leaf.js"), "export const leaf = 21;").unwrap();
    std::fs::write(
        directory.path().join("helper.js"),
        "import { leaf } from './leaf.js'; export const value = leaf * 2;",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("plugin.ts"),
        "import { value } from './helper.js'; export function answer() { return value; }",
    )
    .unwrap();
    let vm = Runtime::spawn().unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        vm.load_package(directory.path(), "plugin.ts"),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        vm.call("answer", "null", SECOND).await.unwrap(),
        serde_json::json!(42)
    );
}
