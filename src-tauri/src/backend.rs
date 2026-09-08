use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessesToUpdate, Signal, System};

pub struct BackendHandle {
    pub child: Child,
    pub port: u16,
}

const H2_PARAMS: &str = ";MODE=MySQL;DATABASE_TO_LOWER=TRUE;CASE_INSENSITIVE_IDENTIFIERS=TRUE";
const LOG_TRUNCATE_BYTES: u64 = 5 * 1024 * 1024;

/// 拉起后端子进程:cwd=数据目录(兜底后端 ./data 相对路径),stdout/stderr 追加进日志(>5MB 先清空)
#[allow(clippy::too_many_arguments)]
pub fn spawn_backend(
    java_bin: &Path,
    jar: &Path,
    webapp_dir: &Path,
    data_dir: &Path,
    port: u16,
    key: &str,
    log_path: &Path,
) -> std::io::Result<Child> {
    if let Ok(meta) = fs::metadata(log_path) {
        if meta.len() > LOG_TRUNCATE_BYTES {
            let _ = fs::write(log_path, b"");
        }
    }
    let log = OpenOptions::new().create(true).append(true).open(log_path)?;
    // file: URL:Windows 路径需正斜杠 + 盘符前补斜杠(file:/D:/x;file:D:/x 是 opaque URI 不可解析)
    let to_url_path = |p: &Path| p.display().to_string().replace('\\', "/");
    let datasource = format!("jdbc:h2:file:{}{}", to_url_path(&data_dir.join("geekwaves")), H2_PARAMS);
    let webapp_url = to_url_path(webapp_dir);
    let webapp_loc = if webapp_url.starts_with('/') {
        format!("file:{webapp_url}/")
    } else {
        format!("file:/{webapp_url}/")
    };
    Command::new(java_bin)
        .arg("-jar")
        .arg(jar)
        .arg(format!("--server.port={port}"))
        .arg("--server.address=127.0.0.1")
        .arg(format!("--spring.datasource.url={datasource}"))
        .arg(format!("--spring.web.resources.static-locations={webapp_loc}"))
        .arg("--geekwaves.web.spa-fallback=true")
        .env("GEEKWAVES_CRYPTO_KEY", key)
        .current_dir(data_dir)
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .spawn()
}

/// 轮询 GET /api/ping(手写 HTTP/1.1,零额外依赖),250ms 间隔,超时报错
pub fn wait_healthy(port: u16, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if ping_ok(port) {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(format!(
                "后端 {timeout:?} 内未通过健康检查(127.0.0.1:{port}/api/ping),详见日志"
            ));
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn ping_ok(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let req = format!("GET /api/ping HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = String::new();
    let _ = stream.read_to_string(&mut buf);
    buf.starts_with("HTTP/1.1 200") || buf.starts_with("HTTP/1.0 200")
}

/// SIGTERM → 5s → SIGKILL,收尸并清 pid 文件。
/// 死亡检测必须用 `child.try_wait()`:sysinfo 0.31 的 `ProcessesToUpdate::Some` 刷新从不移除
/// 死亡条目(apple/system.rs remove_processes=false),`sys.process(pid).is_none()` 恒假,
/// 轮询必空耗满超时,且 deadline 后对可能复用的 pid 盲发 SIGKILL(Task 3 审查实修)。
/// std `child.kill()` 直发句柄(SIGKILL),无 pid 复用风险。
pub fn graceful_shutdown(child: &mut Child, pid_path: &Path) {
    let pid = Pid::from_u32(child.id());
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]));
    if let Some(p) = sys.process(pid) {
        p.kill_with(Signal::Term).unwrap_or_else(|| p.kill());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if matches!(child.try_wait(), Ok(Some(_))) {
                break; // 已 reap,确定死亡,立即退出
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            thread::sleep(Duration::from_millis(200));
        }
    }
    let _ = child.wait();
    let _ = fs::remove_file(pid_path);
}

/// 日志尾部 N 行(日志有 5MB 截断上限,全量读取可接受)
pub fn log_tail(path: &Path, lines: usize) -> String {
    let Ok(mut f) = File::open(path) else {
        return "(无日志文件)".into();
    };
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf);
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn log_tail_returns_last_n_lines() {
        let p = std::env::temp_dir().join(format!("gw-logtail-test-{}", std::process::id()));
        let mut f = fs::File::create(&p).unwrap();
        for i in 0..60 {
            writeln!(f, "line-{i}").unwrap();
        }
        drop(f);
        let tail = log_tail(&p, 50);
        assert!(tail.starts_with("line-10"));
        assert!(tail.ends_with("line-59"));
        assert_eq!(tail.lines().count(), 50);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn wait_healthy_times_out_when_nothing_listens() {
        // 区间外挑一个大概率空闲端口,短超时验证报错文案
        let port = 18977;
        if TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse().unwrap(),
            Duration::from_millis(200),
        )
        .is_ok()
        {
            return; // 被占用则跳过(本机环境差异)
        }
        let err = wait_healthy(port, Duration::from_millis(600)).unwrap_err();
        assert!(err.contains("健康检查"), "报错应说明健康检查失败: {err}");
    }
}
