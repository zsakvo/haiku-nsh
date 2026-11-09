//! Isolated haiku-jsrt echo probe for M0; no plugin package SDK yet.

use haiku_jsrt::Runtime;
use serde_json::{Value, json};
use std::time::Duration;

const ECHO: &str = r#"
    import { createHash } from 'crypto';
    import { Buffer } from 'buffer';
    globalThis.__haikuPluginLeak = 1;
    export async function echo(args) {
        return {
            ok: true,
            n: args.n * 2,
            md5: createHash('md5').update(Buffer.from('abc')).digest('hex'),
            uiGlobalVisible: typeof globalThis.__haikuUiLeak !== 'undefined'
        };
    }
"#;

/// Doubles a finite number in a fresh VM, checking MD5, Buffer and VM isolation.
///
/// Run on a background executor: VM startup and shutdown join an OS thread.
/// Each invocation loads the inline JS independently and always shuts it down.
pub async fn plugin_echo(n: f64) -> Result<f64, String> {
    if !n.is_finite() || !(n * 2.0).is_finite() {
        return Err("The echo input and doubled result must be finite".into());
    }
    let runtime = Runtime::spawn().map_err(|error| error.to_string())?;
    let result = call_echo(&runtime, n).await;
    let shutdown = runtime.shutdown().map_err(|error| error.to_string());
    let value = result?;
    shutdown?;
    Ok(value)
}

async fn call_echo(runtime: &Runtime, n: f64) -> Result<f64, String> {
    runtime
        .load_module("haiku-m0-echo.js", ECHO)
        .await
        .map_err(|error| error.to_string())?;
    let response: Value = runtime
        .call(
            "echo",
            &json!({ "n": n }).to_string(),
            Duration::from_secs(10),
        )
        .await
        .map_err(|error| error.to_string())?;
    if response["ok"] != true
        || response["md5"] != "900150983cd24fb0d6963f7d28e17f72"
        || response["uiGlobalVisible"] != false
    {
        return Err(format!("Echo stdlib/isolation check failed: {response}"));
    }
    response["n"]
        .as_f64()
        .ok_or_else(|| format!("Echo result has no numeric n: {response}"))
}
