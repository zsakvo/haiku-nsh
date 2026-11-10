use haiku_plugins::{
    contract::{ContractMode, run_contract},
    describe_package,
};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn sdk() -> Result<PathBuf> {
    Ok(std::env::var_os("HAIKU_SDK_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../sdk/haiku.d.ts"))
        .canonicalize()?)
}
fn scaffold(name: &str, template: &str) -> Result<()> {
    let path = PathBuf::from(name);
    let id = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid directory name")?;
    if !id
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        || [".", ".."].contains(&id)
    {
        return Err("Use a lowercase plugin name containing only a-z, 0-9, dot or hyphen".into());
    }
    if path.exists() {
        return Err("Destination already exists".into());
    }
    if !["provider", "library", "scraper"].contains(&template) {
        return Err("Unknown template: use provider, library or scraper".into());
    }
    let sdk = sdk()?;
    let common = format!(
        "import {{ definePlugin, field }} from 'haiku';\n\nexport default definePlugin({{\n  id: '{id}',\n  name: '{id}',\n  version: '1.0.0',\n"
    );
    let content = if template == "scraper" {
        "  scrape: {\n    async search(query, ctx) {\n      return [];\n    },\n    async lyrics(candidate, ctx) {\n      return null;\n    },\n  },\n"
    } else {
        "  track: {\n    async get(id, ctx) {\n      return { id, title: 'Example', artists: [] };\n    },\n    async streams(id, ctx) {\n      return [{ url: 'https://example.invalid/audio.mp3', format: 'mp3' }];\n    },\n    async lyrics(id, ctx) {\n      return null;\n    },\n  },\n"
    };
    let connection = if template == "library" {
        "  connection: {\n    fields: { baseUrl: field.url('Server') },\n    identity: ['baseUrl'],\n    async verify(conn, ctx) {\n      return { name: conn.baseUrl };\n    },\n  },\n"
    } else {
        ""
    };
    let cases = if template == "scraper" {
        json!([
            {"name":"search schema","path":"scrape.search","args":[{"title":"Example","artists":[]}]},
            {"name":"lyrics schema","path":"scrape.lyrics","args":[{"id":"example","title":"Example","artists":[]}]}
        ])
    } else {
        json!([
            {"name":"native id","path":"track.get","args":["example"]},
            {"name":"stream schema","path":"track.streams","args":["example"]},
            {"name":"lyrics schema","path":"track.lyrics","args":["example"]}
        ])
    };
    std::fs::create_dir_all(path.join("fixtures"))?;
    std::fs::write(
        path.join("plugin.ts"),
        format!("{common}{connection}{content}}});\n"),
    )?;
    std::fs::write(
        path.join("tsconfig.json"),
        serde_json::to_string_pretty(&json!({
            "compilerOptions":{"strict":true,"noEmit":true,"target":"ES2022","module":"ESNext","moduleResolution":"bundler","lib":["ES2022","DOM","DOM.Iterable"]},
            "files":[sdk,"plugin.ts"]
        }))?,
    )?;
    std::fs::write(
        path.join("fixtures/contract.json"),
        serde_json::to_string_pretty(&json!({
            "connection":{"baseUrl":"http://example.invalid"},"routes":[],"cases":cases
        }))?,
    )?;
    std::fs::write(
        path.join("README.md"),
        format!(
            "# {id}\n\nGenerated {template} plugin. Replace the example handlers and add upstream HTTP fixtures.\n\n```sh\nhaiku-plugin check .\nhaiku-plugin test .\n```\n\nSet HAIKU_TSC to a tsc executable or tsc.js when it is not on PATH. HAIKU_SDK_PATH can select another haiku.d.ts.\n"
        ),
    )?;
    println!("Created {} ({template})", path.display());
    Ok(())
}
async fn check(path: &Path) -> Result<()> {
    let path = path.canonicalize()?;
    let temp = tempfile::tempdir()?;
    let config = temp.path().join("tsconfig.json");
    std::fs::write(
        &config,
        serde_json::to_vec_pretty(&json!({
            "compilerOptions":{"strict":true,"noEmit":true,"target":"ES2022","module":"ESNext","moduleResolution":"bundler","lib":["ES2022","DOM","DOM.Iterable"]},
            "files":[sdk()?,path.join("plugin.ts")]
        }))?,
    )?;
    let compiler = std::env::var("HAIKU_TSC").unwrap_or_else(|_| "tsc".into());
    let mut command = if compiler.ends_with(".js") {
        let mut command = Command::new("node");
        command.arg(&compiler);
        command
    } else {
        Command::new(&compiler)
    };
    let status = command.args(["--noEmit", "-p"]).arg(&config).status()
        .map_err(|e| format!("Cannot run TypeScript compiler {compiler}: {e}. Install tsc or set HAIKU_TSC to its executable/tsc.js; no npm runtime is required by the app."))?;
    if !status.success() {
        return Err("TypeScript check failed".into());
    }
    let descriptor = describe_package(&path).await?;
    eprintln!(
        "{} [{}]: {}",
        descriptor.id,
        descriptor.kind,
        descriptor.capabilities.join(", ")
    );
    println!("{}", serde_json::to_string_pretty(&descriptor)?);
    Ok(())
}
async fn run() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("new") if args.len() == 4 && args[2] == "--template" => scaffold(&args[1], &args[3]),
        Some("check") if args.len() == 2 => check(Path::new(&args[1])).await,
        Some("test") if args.len() >= 2 => {
            let flags = &args[2..];
            if flags.iter().any(|s| s != "--live" && s != "--record") || flags.len() > 1 {
                return Err("Use at most one of --live or --record".into());
            }
            let mode = match flags.first().map(String::as_str) {
                Some("--live") => ContractMode::Live,
                Some("--record") => ContractMode::Record,
                _ => ContractMode::Replay,
            };
            let report = run_contract(Path::new(&args[1]), mode).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            eprintln!("{}", report.summary());
            if report.passed { Ok(()) } else { Err("Contract tests failed".into()) }
        }
        _ => Err("Usage: haiku-plugin new <name> --template provider|library|scraper\n       haiku-plugin check <dir>\n       haiku-plugin test <dir> [--live|--record]".into()),
    }
}
#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("haiku-plugin: {error}");
        std::process::exit(1);
    }
}
