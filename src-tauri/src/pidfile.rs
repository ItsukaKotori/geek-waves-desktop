use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, Signal, System, UpdateKind};

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
    // sysinfo 0.31 的 refresh_processes 默认刷新项不含 cmd,cmd() 恒为空 → 防误杀检查必失败且从不 kill
    // (Task 3 本机端到端实修)。必须显式 with_cmd。
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        ProcessRefreshKind::new().with_cmd(UpdateKind::Always),
    );
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
    use sysinfo::{ProcessesToUpdate, System};

    #[test]
    fn refreshed_process_exposes_cmdline_for_orphan_guard() {
        // 回归钉住:sysinfo 0.31 默认 refresh_processes 不含 cmd,cmd() 为空 →
        // cleanup_orphan 的防误杀检查恒不通过,孤儿永远不会被清理(Task 3 本机端到端实修)。
        // cleanup_orphan 必须用 refresh_processes_specifics + with_cmd(UpdateKind::Always)。
        let pid = Pid::from_u32(std::process::id());
        let mut sys = System::new();

        sys.refresh_processes(ProcessesToUpdate::Some(&[pid]));
        let default_cmd: String = sys
            .process(pid)
            .map(|p| p.cmd().iter().map(|c| c.to_string_lossy()).collect())
            .unwrap_or_default();
        assert!(
            default_cmd.is_empty(),
            "默认刷新若已含 cmd,说明 sysinfo 行为变化,请复核 cleanup_orphan 是否可回归普通 refresh_processes: {default_cmd:?}"
        );

        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            ProcessRefreshKind::new().with_cmd(UpdateKind::Always),
        );
        let full_cmd: String = sys
            .process(pid)
            .map(|p| p.cmd().iter().map(|c| c.to_string_lossy()).collect())
            .unwrap_or_default();
        assert!(
            !full_cmd.is_empty(),
            "with_cmd 刷新后 cmd 仍为空,cleanup_orphan 防误杀检查会失效"
        );
    }

    #[test]
    fn pid_roundtrip() {
        let p = std::env::temp_dir().join(format!("gw-pid-test-{}", std::process::id()));
        write_pid(&p, 424242);
        assert_eq!(read_pid(&p), Some(424242));
        let _ = fs::remove_file(&p);
        assert_eq!(read_pid(&p), None); // 不存在 → None
    }
}
