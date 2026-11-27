//! Agent-authored plugin workspaces for the developer tools window.
//!
//! A workspace keeps the reference material next to the package, never inside it:
//! `plugin/` is the package the host installs; `sdk/`, `examples/`, `tsconfig.json`
//! and `AGENT.md` only guide the agent. The agent reports progress by rewriting `STATUS`
//! (first line `writing`, `ready` or `done`, then a message); Haiku watches that one file
//! through file system events and writes the outcome of each load to `LOAD`.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const SDK_TYPES: &str = include_str!("../../../sdk/haiku.d.ts");
const SDK_GUIDE: &str = include_str!("../../../sdk/GUIDE.md");
const EXAMPLE: &str = include_str!("../../../plugins/subsonic/plugin.ts");
const META: &str = "workspace.json";
pub(crate) const STATUS: &str = "STATUS";
pub(crate) const LOAD: &str = "LOAD";
pub(crate) const LOGO: &str = "logo.svg";

/// What the agent last wrote to `STATUS`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Status {
    Writing(String),
    Ready(String),
    Done(String),
}

/// A placeholder logo (no text, so no font is needed): a tinted rounded square with a note.
fn default_logo(seed: &str) -> String {
    let hue = seed.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) % 360;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128"><rect width="128" height="128" rx="28" fill="hsl({hue},62%,48%)"/><g transform="translate(28 28) scale(3)" fill="none" stroke="#fff" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"><path d="M3 20a3 3 0 1 0 6 0a3 3 0 1 0-6 0m12-2a3 3 0 1 0 6 0a3 3 0 1 0-6 0"/><path d="M9 20V5l12-2v15M9 9l12-2"/></g></svg>"##
    )
}

fn slug(input: &str) -> String {
    let mut out = String::new();
    for c in input.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').chars().take(32).collect()
}

fn prompt(path: &Path, meta: &Value) -> String {
    let ws = path.display();
    let text = |key: &str| meta[key].as_str().unwrap_or("").to_string();
    let description = text("description");
    format!(
        r#"你是 Haiku 音乐播放器的插件开发者。请在本机目录中实现一个 Haiku v2 插件，完成后按“完成信号”一节通知 Haiku。

## 插件信息
- 名称：{name}
- 作者：{author}
- 官网：{website}
- 插件 ID：{id}（definePlugin 的 id 必须使用这个值）
- 版本：0.1.0
- 需求描述：{description}

## 工作目录
`{ws}`
- 插件包只写在 `{ws}/plugin/` 内：入口为 `plugin/plugin.ts`，默认导出 `definePlugin({{ ... }})`，SDK 从 `'haiku'` 导入。
- 图标：`plugin/logo.svg` 只是占位图，请主动替换为该服务清晰的官方图标（见“图标”一节）。可以拆分多个模块，但只允许 `./` 相对导入，不能引用 `plugin/` 以外的文件，不要放 node_modules。
- 开发指南：`{ws}/sdk/GUIDE.md`（必读：来源与实例、认证方式、能力约束、错误类型、分页规则）。
- SDK 类型声明：`{ws}/sdk/haiku.d.ts`（definePlugin、field、ctx.http、错误类与全部能力签名）。
- 完整可运行示例：`{ws}/examples/subsonic.ts`（库插件：connection + 分页 + 播放地址）。

## 实现要求
1. 先通读指南和类型声明，再根据需求描述判断插件类型：平台账号用 `auth`，自建服务器/曲库用 `connection`，元数据或歌词刮削用 `scrape.*`。
2. 只实现上游真正支持的能力；不支持的能力直接省略，不要返回“成功但永远为空”的结果。
3. 网络请求使用 `ctx.http`（或 fetch），尊重 `ctx.signal`；不要在模块顶层发起请求；HTTP/业务失败转换为从 `'haiku'` 导入的 SDK 错误（Upstream、NotFound、AuthRequired 等）。
4. 返回上游原始 ID，搜索结果 ID 必须能被 `track.get` 回查；分页返回 `{{ items, next? }}`，结束时省略 next。
5. 不要写入任何真实账号、Cookie、Token 或密钥；凭据由宿主保存，不要复制进 KV。
6. 运行时是 QuickJS，不是 Node：只有受限的 crypto/buffer/zlib、timers 和 fetch。

## 图标
插件列表、来源选择等处都会显示图标，务必替换占位图：
1. 从官网（{website_hint}）获取官方图标，优先级：SVG 矢量 Logo（页面内联 svg、`<link rel="icon" type="image/svg+xml">`、mask-icon）> `apple-touch-icon` / manifest.json 中最大尺寸的 PNG（至少 180×180）> 应用商店图标。不要使用 16/32px 的 favicon.ico，也不要自己绘制或用文字拼凑。
2. 下载到 `plugin/` 内，例如 `plugin/logo.svg` 或 `plugin/logo.png`（不超过 512 KiB；方形、主体居中、透明或纯色背景），并把 definePlugin 的 `logo` 改成对应文件名，删除不再使用的占位图。
3. 实在找不到官方图标时保留占位图并在 `done` 总结中说明。

## 校验
在工作目录执行类型检查，直到没有错误：
`cd "{ws}" && npx -y -p typescript tsc -p tsconfig.json`

## 状态文件
Haiku 的开发工具监听 `{ws}/STATUS` 这一个文件（文件系统事件，不轮询 `plugin/`）。每次覆盖写入整个文件：第一行是状态，之后是给用户看的说明。
- `writing`：正在编写，例如 `writing\n正在实现搜索`。开始工作时先写一次。
- `ready`：`plugin/` 已可加载。Haiku 收到后立即热加载（保留 KV），结果写入 `{ws}/LOAD`：第一行 `ok` 或 `error`，之后是插件信息或错误原因。每完成一组可测试的改动就写一次 `ready`，读取 `LOAD`，遇到 `error` 修复后再写 `ready`。
- `done`：全部实现、类型检查通过且 `LOAD` 为 `ok` 后，最后一步写 `done`，之后几行是简短总结（实现了哪些能力、已知限制）。

之后如需修改，编辑 `plugin/` 并再次写入 `ready` 或 `done` 即可重新加载。
"#,
        name = text("name"),
        author = text("author"),
        website = if text("website").is_empty() { "（未填写）".into() } else { text("website") },
        website_hint = if text("website").is_empty() { "未填写，请根据需求描述自行查找该服务官网".into() } else { text("website") },
        id = text("pluginId"),
        description = if description.is_empty() { "（未填写）".into() } else { description },
    )
}

fn read_meta(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path.join(META)).ok()?).ok()
}

fn summary(path: &Path, meta: Value) -> Value {
    json!({
        "path": path.to_string_lossy(),
        "name": meta["name"], "author": meta["author"], "website": meta["website"], "description": meta["description"],
        "pluginId": meta["pluginId"], "createdAt": meta["createdAt"],
        "done": matches!(status(path), Some(Status::Done(_))),
        "logo": logo(path),
        "prompt": prompt(path, &meta),
    })
}

/// Create `<root>/<slug>-<short id>` with reference material and an empty package directory.
pub(crate) fn create(root: &Path, name: &str, author: &str, website: &str, description: &str) -> Result<Value, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("请填写插件名称".into());
    }
    let short = uuid::Uuid::new_v4().simple().to_string()[..6].to_string();
    let base = match slug(name) {
        s if s.is_empty() => format!("plugin-{short}"),
        s => s,
    };
    let owner = match slug(author) {
        s if s.is_empty() => "local".to_string(),
        s => s,
    };
    let path = root.join(format!("{base}-{short}"));
    std::fs::create_dir_all(path.join("plugin")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(path.join("sdk")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(path.join("examples")).map_err(|e| e.to_string())?;
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let meta = json!({
        "name": name, "author": author.trim(), "website": website.trim(), "description": description.trim(),
        "pluginId": format!("{owner}.{base}"), "createdAt": created,
    });
    let write = |file: &str, body: &str| std::fs::write(path.join(file), body).map_err(|e| e.to_string());
    write(META, &serde_json::to_string_pretty(&meta).unwrap())?;
    write("sdk/haiku.d.ts", SDK_TYPES)?;
    write("sdk/GUIDE.md", SDK_GUIDE)?;
    write("examples/subsonic.ts", EXAMPLE)?;
    write(&format!("plugin/{LOGO}"), &default_logo(&meta["pluginId"].to_string()))?;
    write(
        "tsconfig.json",
        r#"{
  "compilerOptions": {
    "strict": true,
    "noEmit": true,
    "allowImportingTsExtensions": true,
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2022", "DOM", "DOM.Iterable"]
  },
  "files": ["sdk/haiku.d.ts", "plugin/plugin.ts"]
}
"#,
    )?;
    let text = prompt(&path, &meta);
    write("AGENT.md", &text)?;
    Ok(summary(&path, meta))
}

/// Workspaces under `root`, newest first; folders without metadata are not ours.
pub(crate) fn list(root: &Path) -> Vec<Value> {
    let mut items: Vec<(u64, Value)> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let meta = read_meta(&path)?;
            Some((meta["createdAt"].as_u64().unwrap_or_default(), summary(&path, meta)))
        })
        .collect();
    items.sort_by(|a, b| b.0.cmp(&a.0));
    items.into_iter().map(|(_, v)| v).collect()
}

/// Only paths created by [`create`] may be watched or removed through the tools window.
pub(crate) fn resolve(root: &Path, path: &str) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = Path::new(path).canonicalize().map_err(|e| e.to_string())?;
    if path.parent() != Some(root.as_path()) || read_meta(&path).is_none() {
        return Err("不是开发工具创建的插件工作区".into());
    }
    Ok(path)
}

pub(crate) fn remove(root: &Path, path: &str) -> Result<(), String> {
    std::fs::remove_dir_all(resolve(root, path)?).map_err(|e| e.to_string())
}

/// What the agent last wrote to `STATUS`; an unknown keyword reads as still writing.
pub(crate) fn status(path: &Path) -> Option<Status> {
    let text = std::fs::read_to_string(path.join(STATUS)).ok()?;
    let text = text.trim();
    let (head, rest) = text.split_once('\n').unwrap_or((text, ""));
    let rest: String = rest.trim().chars().take(2000).collect();
    Some(match head.trim().to_ascii_lowercase().as_str() {
        "ready" => Status::Ready(rest),
        "done" => Status::Done(rest),
        _ => Status::Writing(rest),
    })
}

/// Tell the agent how its last `ready`/`done` loaded; it reads this file to fix errors.
pub(crate) fn write_load(path: &Path, result: &Result<Value, String>) {
    let body = match result {
        Ok(plugin) => format!("ok\n{}\n", serde_json::to_string_pretty(plugin).unwrap_or_default()),
        Err(error) => format!("error\n{error}\n"),
    };
    let _ = std::fs::write(path.join(LOAD), body);
}

/// The prompt of an existing workspace, regenerated from its metadata.
pub(crate) fn prompt_of(path: &Path) -> Result<String, String> {
    let meta = read_meta(path).ok_or("工作区元数据缺失")?;
    Ok(prompt(path, &meta))
}

/// The system clipboard, through the platform's command-line tool.
pub(crate) fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "macos") { ("pbcopy", &[]) }
        else if cfg!(target_os = "windows") { ("clip", &[]) } else { ("xclip", &["-selection", "clipboard"]) };
    let mut child = std::process::Command::new(program).args(args)
        .stdin(std::process::Stdio::piped()).spawn().map_err(|e| format!("无法写入剪贴板：{e}"))?;
    child.stdin.take().ok_or("无法写入剪贴板")?.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("无法写入剪贴板：{program} 退出码 {status}")) }
}

/// The package logo as a data URL: an official raster logo the agent fetched, else `logo.svg`
/// (the placeholder, or an official SVG written over it).
fn logo(path: &Path) -> Option<String> {
    let package = path.join("plugin");
    ["logo.png", "logo.webp", "logo.jpg", "logo.jpeg", LOGO]
        .into_iter()
        .find_map(|file| crate::plugin_tools::logo(&package, Some(file)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_keeps_reference_material_outside_the_package() {
        let dir = tempfile::tempdir().unwrap();
        let ws = create(dir.path(), "网易 Cloud Music", "Alice B", "https://music.163.com", "demo").unwrap();
        let path = PathBuf::from(ws["path"].as_str().unwrap());
        assert!(path.join("plugin").is_dir() && path.join("sdk/haiku.d.ts").is_file());
        assert!(path.join("plugin/logo.svg").is_file() && ws["logo"].is_string());
        assert_eq!(status(&path), None);
        std::fs::write(path.join(STATUS), "DONE\n搜索与播放").unwrap();
        assert_eq!(status(&path), Some(Status::Done("搜索与播放".into())));
        assert!(ws["pluginId"].as_str().unwrap().starts_with("alice-b.cloud-music"));
        assert_eq!(list(dir.path()).len(), 1);
        assert!(resolve(dir.path(), ws["path"].as_str().unwrap()).is_ok());
        assert!(resolve(dir.path(), dir.path().to_str().unwrap()).is_err());
    }
}
