use std::path::{Path, PathBuf};

/// 校验导入源:必须存在的 .mv.db 文件
pub fn validate_source(src: &Path) -> Result<(), String> {
    if !src.is_file() {
        return Err(format!("文件不存在或不是普通文件: {}", src.display()));
    }
    if !src
        .file_name()
        .map(|n| n.to_string_lossy().ends_with(".mv.db"))
        .unwrap_or(false)
    {
        return Err("请选择 .mv.db 结尾的 H2 数据库文件(通常名为 geekwaves.mv.db)".into());
    }
    Ok(())
}

/// 拷贝库文件到数据目录(目标固定名 geekwaves.mv.db),顺带清掉可能存在的旧 trace 文件
pub fn copy_db(src: &Path, data_dir: &Path) -> Result<PathBuf, String> {
    let dst = data_dir.join("geekwaves.mv.db");
    std::fs::copy(src, &dst).map_err(|e| format!("数据库拷贝失败: {e}"))?;
    let _ = std::fs::remove_file(data_dir.join("geekwaves.trace.db"));
    Ok(dst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gw-import-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn validate_accepts_mv_db_file() {
        let d = temp_dir("accept");
        let f = d.join("geekwaves.mv.db");
        fs::write(&f, b"x").unwrap();
        assert!(validate_source(&f).is_ok());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn validate_rejects_wrong_suffix_and_missing() {
        let d = temp_dir("reject");
        let txt = d.join("notes.txt");
        fs::write(&txt, b"x").unwrap();
        assert!(validate_source(&txt).is_err());
        assert!(validate_source(&d.join("nope.mv.db")).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn copy_db_writes_fixed_name_and_cleans_trace() {
        let d = temp_dir("copy");
        let src = d.join("old.mv.db");
        fs::write(&src, b"payload").unwrap();
        fs::write(d.join("geekwaves.trace.db"), b"stale").unwrap();
        let dst = copy_db(&src, &d).unwrap();
        assert_eq!(dst, d.join("geekwaves.mv.db"));
        assert_eq!(fs::read(&dst).unwrap(), b"payload");
        assert!(!d.join("geekwaves.trace.db").exists());
        let _ = fs::remove_dir_all(&d);
    }
}
