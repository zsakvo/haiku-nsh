use crate::{PluginError, Result, sdk};
use haiku_jsrt::{Runtime, RuntimeOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Descriptor {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub logo: Option<String>,
    pub description: Option<String>,
    pub kind: String,
    pub scraper: bool,
    pub capabilities: Vec<String>,
    pub auth_methods: Vec<String>,
    pub connection: Option<Value>,
    pub http: Option<Value>,
    pub library_cards: Vec<Value>,
    #[serde(default)]
    pub builtin: bool,
}
pub(crate) fn options(
    id: &str,
    storage: Option<std::path::PathBuf>,
    describe: bool,
) -> RuntimeOptions {
    RuntimeOptions {
        instance_id: id.into(),
        storage_path: storage,
        describe_only: describe,
        modules: BTreeMap::from([("haiku".into(), sdk::MODULE.into())]),
        log: Some(std::sync::Arc::new(
            |record| log::debug!(target:"haiku_plugins","{} {} {}",record.instance_id,record.level,record.message),
        )),
        ..Default::default()
    }
}
pub async fn describe_package(root: &Path) -> Result<Descriptor> {
    let vm = Runtime::with_options(options("descriptor", None, true))?;
    vm.load_package(root, "plugin.ts").await?;
    let shape = vm.describe().await?;
    let def = &shape["default"];
    let required = |key: &str| -> Result<String> {
        def[key]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| PluginError::new("INVALID_PLUGIN", format!("Missing {key}")))
    };
    let id = required("id")?;
    if id == "."
        || id == ".."
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
    {
        return Err(PluginError::new("INVALID_PLUGIN", "Invalid plugin id"));
    }
    let version = required("version")?;
    semver::Version::parse(&version)
        .map_err(|e| PluginError::new("INVALID_PLUGIN", e.to_string()))?;
    let mut capabilities = Vec::new();
    fn functions(value: &Value, path: &str, out: &mut Vec<String>) {
        if value == "function" {
            out.push(path.into());
        } else if let Some(object) = value.as_object() {
            for (k, v) in object {
                functions(v, &format!("{path}.{k}"), out);
            }
        }
    }
    for key in [
        "search",
        "track",
        "album",
        "artist",
        "playlist",
        "toplist",
        "program",
        "radio",
        "home",
        "categories",
        "me",
        "library",
        "scrape",
        "auth",
    ] {
        if !def[key].is_null() {
            functions(&def[key], key, &mut capabilities);
        }
    }
    let scraper = !def["scrape"].is_null();
    let content = capabilities.iter().any(|c| !c.starts_with("scrape."));
    let kind = if !def["connection"].is_null() {
        "library"
    } else if content {
        "provider"
    } else if scraper {
        "scraper"
    } else {
        return Err(PluginError::new(
            "INVALID_PLUGIN",
            "Plugin has no capabilities",
        ));
    };
    let mut connection = def.get("connection").cloned();
    if let Some(value) = &mut connection {
        if let Some(o) = value.as_object_mut() {
            o.remove("verify");
        }
    }
    let auth_methods = ["qrcode", "sms", "password", "cookie", "web"]
        .into_iter()
        .filter(|k| !def["auth"][k].is_null())
        .map(str::to_owned)
        .collect();
    let descriptor = Descriptor {
        id,
        name: required("name")?,
        version,
        author: def["author"].as_str().map(str::to_owned),
        logo: def["logo"].as_str().map(str::to_owned),
        description: def["description"].as_str().map(str::to_owned),
        kind: kind.into(),
        scraper,
        capabilities,
        auth_methods,
        connection,
        http: def.get("http").cloned(),
        library_cards: def["library"]["cards"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
        builtin: false,
    };
    Ok(descriptor)
}
pub fn normalize_source_url(raw: &str) -> String {
    let mut text = raw.trim().to_owned();
    if text.is_empty() {
        return text;
    }
    if !text.to_ascii_lowercase().starts_with("http://")
        && !text.to_ascii_lowercase().starts_with("https://")
    {
        text = format!("http://{text}");
    }
    text = text.trim_end_matches('/').to_owned();
    let lower = text.to_ascii_lowercase();
    if lower.ends_with("/rest") {
        text.truncate(text.len() - 5);
    } else if lower.ends_with("/app") {
        text.truncate(text.len() - 4);
    }
    url::Url::parse(&text)
        .map(|u| u.to_string().trim_end_matches('/').into())
        .unwrap_or(text)
}
pub fn source_id(descriptor: &Descriptor, values: &BTreeMap<String, String>) -> String {
    use sha1::{Digest, Sha1};
    if descriptor.kind != "library" {
        return descriptor.id.clone();
    }
    let connection = descriptor.connection.as_ref().unwrap();
    let keys = connection["identity"]
        .as_array()
        .map(|a| a.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let identity = keys
        .iter()
        .filter_map(|key| key.as_str())
        .map(|key| {
            let value = values.get(key).map(String::as_str).unwrap_or("");
            if connection["fields"][key]["type"] == "url" {
                normalize_source_url(value)
            } else {
                value.trim().into()
            }
        })
        .collect::<Vec<_>>()
        .join("|");
    {
        let hash = Sha1::digest(identity.as_bytes())
            .iter()
            .take(4)
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        format!("{}#{}", descriptor.id, &hash[..8])
    }
}
