use std::path::{Path, PathBuf};

/// 数据目录:macOS ~/Library/Application Support/GeekWaves,Windows %APPDATA%\GeekWaves,Linux ~/.local/share/GeekWaves
pub fn data_dir() -> PathBuf {
    dirs::data_dir().expect("无法解析系统数据目录").join("GeekWaves")
}

pub fn db_file(data_dir: &Path) -> PathBuf {
    data_dir.join("geekwaves.mv.db")
}

pub fn key_file(data_dir: &Path) -> PathBuf {
    data_dir.join("crypto.key")
}

pub fn pid_file(data_dir: &Path) -> PathBuf {
    data_dir.join("backend.pid")
}

pub fn imported_marker(data_dir: &Path) -> PathBuf {
    data_dir.join("imported")
}

pub fn log_file(data_dir: &Path) -> PathBuf {
    data_dir.join("logs").join("backend.log")
}

/// 资源目录约定:bundle.resources 数组按 conf 相对路径原样拷贝(见 build.sh 组装与 Task 3 验证)
pub fn java_bin(resource_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        resource_dir.join("resources/runtime/bin/java.exe")
    } else {
        resource_dir.join("resources/runtime/bin/java")
    }
}

pub fn jar_path(resource_dir: &Path) -> PathBuf {
    resource_dir.join("resources/app.jar")
}

pub fn webapp_dir(resource_dir: &Path) -> PathBuf {
    resource_dir.join("resources/webapp")
}

#[cfg(test)]
mod tests {
    #[test]
    fn data_dir_ends_with_geekwaves() {
        let dir = super::data_dir();
        assert!(dir.ends_with("GeekWaves"), "数据目录应为 .../GeekWaves: {dir:?}");
    }
}
