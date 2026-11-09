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
                        },
                        deadline: Instant::now() + Duration::from_secs(60),
                        reply,
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
