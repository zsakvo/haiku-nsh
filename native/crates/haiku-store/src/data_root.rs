//! Haiku NSH 独立数据根；环境变量可覆盖，默认不读取参考版数据。
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
        home.join("Library/Application Support/Haiku NSH")
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("haiku-nsh")
    }
}
