use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessesToUpdate, Signal, System};

pub fn read_pid(pid_path: &Path) -> Option<u32> {
    fs::read_to_string(pid_path).ok()?.trim().parse().ok()
}

pub fn write_pid(pid_path: &Path, pid: u32) {
    let _ = fs::write(pid_path, pid.to_string());
}

/// 杀掉上次残留的后端:PID 存活且命令行同时含 marker(资源目录路径)与 app.jar 才动,防误杀。
/// SIGTERM → 等 5s → SIGKILL,最后清 pid 文件。
pub fn cleanup_orphan(pid_path: &Path, marker: &str) {
    let Some(pid) = read_pid(pid_path) else { return };
    let pid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]));
    let Some(proc) = sys.process(pid) else {
        let _ = fs::remove_file(pid_path);
        return;
    };
    let cmdline = proc.cmd().iter().map(|s| s.to_string_lossy()).collect::<String>();
    if !cmdline.contains(marker) || !cmdline.contains("app.jar") {
        let _ = fs::remove_file(pid_path);
        return;
    }
    proc.kill_with(Signal::Term).unwrap_or_else(|| proc.kill());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        sys.refresh_processes(ProcessesToUpdate::Some(&[pid]));
        if sys.process(pid).is_none() {
            break;
        }
        if Instant::now() > deadline {
            if let Some(p) = sys.process(pid) {
                p.kill();
            }
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    let _ = fs::remove_file(pid_path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_roundtrip() {
        let p = std::env::temp_dir().join(format!("gw-pid-test-{}", std::process::id()));
        write_pid(&p, 424242);
        assert_eq!(read_pid(&p), Some(424242));
        let _ = fs::remove_file(&p);
        assert_eq!(read_pid(&p), None); // 不存在 → None
    }
}
