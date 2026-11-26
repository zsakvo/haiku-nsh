//! Agent-authored plugin workspaces for the developer tools window.
//!
//! A workspace keeps the reference material next to the package, never inside it:
//! `plugin/` is the package the host installs; `sdk/`, `examples/`, `tsconfig.json`
//! and `AGENT.md` only guide the agent. The agent writes `DONE` last as its signal.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const SDK_TYPES: &str = include_str!("../../../sdk/haiku.d.ts");
const SDK_GUIDE: &str = include_str!("../../../sdk/GUIDE.md");
const EXAMPLE: &str = include_str!("../../../plugins/subsonic/plugin.ts");
const META: &str = "workspace.json";
pub(crate) const DONE: &str = "DONE";

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
- 插件 ID：{id}（definePlugin 的 id 必须使用这个值）
- 版本：0.1.0
- 需求描述：{description}

## 工作目录
`{ws}`
- 插件包只写在 `{ws}/plugin/` 内：入口为 `plugin/plugin.ts`，默认导出 `definePlugin({{ ... }})`，SDK 从 `'haiku'` 导入。可以拆分多个模块，但只允许 `./` 相对导入，不能引用 `plugin/` 以外的文件，不要放 node_modules。
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

## 校验
在工作目录执行类型检查，直到没有错误：
`cd "{ws}" && npx -y -p typescript tsc -p tsconfig.json`

## 完成信号
Haiku 的开发工具正在监听这个工作目录：`plugin/` 每次保存都会尝试热加载，加载失败的原因会显示在开发工具里。
全部实现并通过类型检查后，最后一步在 `{ws}/DONE` 写入一段简短总结（实现了哪些能力、已知限制）。开发工具看到 DONE 且插件加载成功，即视为完成。之后如需修改，直接编辑 `plugin/`，开发工具会再次热加载。
"#,
        name = text("name"),
        author = text("author"),
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
        "name": meta["name"], "author": meta["author"], "description": meta["description"],
        "pluginId": meta["pluginId"], "createdAt": meta["createdAt"],
        "done": path.join(DONE).is_file(),
        "prompt": prompt(path, &meta),
    })
}

/// Create `<root>/<slug>-<short id>` with reference material and an empty package directory.
pub(crate) fn create(root: &Path, name: &str, author: &str, description: &str) -> Result<Value, String> {
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
        "name": name, "author": author.trim(), "description": description.trim(),
        "pluginId": format!("{owner}.{base}"), "createdAt": created,
    });
    let write = |file: &str, body: &str| std::fs::write(path.join(file), body).map_err(|e| e.to_string());
    write(META, &serde_json::to_string_pretty(&meta).unwrap())?;
    write("sdk/haiku.d.ts", SDK_TYPES)?;
    write("sdk/GUIDE.md", SDK_GUIDE)?;
    write("examples/subsonic.ts", EXAMPLE)?;
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

/// The DONE marker's modification time and its summary text, when present.
pub(crate) fn done(path: &Path) -> Option<(std::time::SystemTime, String)> {
    let marker = path.join(DONE);
    let modified = std::fs::metadata(&marker).ok()?.modified().ok()?;
    let text = std::fs::read_to_string(marker).unwrap_or_default();
    Some((modified, text.trim().chars().take(2000).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_keeps_reference_material_outside_the_package() {
        let dir = tempfile::tempdir().unwrap();
        let ws = create(dir.path(), "网易 Cloud Music", "Alice B", "demo").unwrap();
        let path = PathBuf::from(ws["path"].as_str().unwrap());
        assert!(path.join("plugin").is_dir() && path.join("sdk/haiku.d.ts").is_file());
        assert!(ws["pluginId"].as_str().unwrap().starts_with("alice-b.cloud-music"));
        assert_eq!(list(dir.path()).len(), 1);
        assert!(resolve(dir.path(), ws["path"].as_str().unwrap()).is_ok());
        assert!(resolve(dir.path(), dir.path().to_str().unwrap()).is_err());
    }
}
