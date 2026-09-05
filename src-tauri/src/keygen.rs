use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;

/// 与后端 CryptoProperties 同规则:32 字节随机数的 base64 文本
pub fn generate_key() -> String {
    let mut raw = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut raw);
    B64.encode(raw)
}

pub fn validate_key(text: &str) -> Result<(), String> {
    let trimmed = text.trim();
    let raw = B64
        .decode(trimmed)
        .map_err(|e| format!("key 不是合法 base64: {e}"))?;
    if raw.len() != 32 {
        return Err(format!("key 必须为 32 字节(256bit),当前 {} 字节", raw.len()));
    }
    Ok(())
}

/// 存在则读(去空白);不存在则生成并写入(0600)
pub fn load_or_create(path: &Path) -> std::io::Result<String> {
    if path.exists() {
        return std::fs::read_to_string(path).map(|s| s.trim().to_string());
    }
    let key = generate_key();
    write_private(path, &key)?;
    Ok(key)
}

/// 仅新建写入(已存在时报错,防误覆盖);unix 下权限 0600
pub fn write_private(path: &Path, key: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(key.trim().as_bytes())?;
    }
    #[cfg(not(unix))]
    {
        let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
        f.write_all(key.trim().as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("gw-keygen-test-{}-{name}", std::process::id()));
        let _ = fs::remove_file(&p);
        p
    }

    #[test]
    fn generated_key_is_32_byte_base64() {
        let raw = B64.decode(generate_key()).unwrap();
        assert_eq!(raw.len(), 32);
    }

    #[test]
    fn validate_matches_backend_rule() {
        assert!(validate_key(&generate_key()).is_ok());
        assert!(validate_key("short").is_err()); // 非 base64
        assert!(validate_key(&B64.encode([0u8; 16])).is_err()); // 长度错
    }

    #[test]
    fn write_then_load_roundtrip() {
        let p = temp_path("roundtrip");
        let key = generate_key();
        write_private(&p, &key).unwrap();
        assert_eq!(load_or_create(&p).unwrap(), key);
        // 二次 write_private 失败(已存在,防覆盖)
        assert!(write_private(&p, &generate_key()).is_err());
        let _ = fs::remove_file(&p);
    }
}
