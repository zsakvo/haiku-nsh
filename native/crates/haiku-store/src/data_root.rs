//! 参考版数据根策略；提取自 RPC，供原生模块直接调用。
use std::path::PathBuf;

pub fn data_root() -> PathBuf {
    if let Some(root) = std::env::var_os("HAIKU_DATA_DIR") {
        return root.into();
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    {
        home.join("Library/Application Support/Haiku GPUIX")
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("haiku-gpuix")
    }
}
