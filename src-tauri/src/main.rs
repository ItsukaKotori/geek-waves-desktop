// Windows release 隐藏控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod import;
mod keygen;
mod paths;
mod pidfile;
mod port;

use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use backend::BackendHandle;

/// 外链接管:每次导航(含后端远端页)注入。前端全站 target="_blank",Tauri 默认拦新窗口;
/// 拦截外部 http(s) 点击 → 系统浏览器(opener,经 capability remote 授权 127.0.0.1 源)。
const EXTERNAL_LINKS_SCRIPT: &str = r#"
(function () {
  if (window.__gwExternalLinks) return;
  window.__gwExternalLinks = true;
  document.addEventListener('click', function (ev) {
    var a = ev.target && ev.target.closest ? ev.target.closest('a') : null;
    if (!a) return;
    var href = a.getAttribute('href') || '';
    if (!/^https?:\/\//i.test(href)) return;      // 相对链接(SPA 路由)与锚点不管
    if (href.indexOf('http://127.0.0.1:') === 0) return; // 后端自身
    ev.preventDefault();
    var opener = window.__TAURI__ && window.__TAURI__.opener;
    if (opener && opener.openUrl) {
      Promise.resolve(opener.openUrl(href)).catch(function (e) {
        console.warn('[GeekWaves] 打开外链失败:', e);
      });
    } else {
      console.warn('[GeekWaves] opener 不可用,无法用系统浏览器打开:', href);
    }
  }, true);
})();
"#;

struct AppState {
    data_dir: std::path::PathBuf,
    backend: Mutex<Option<BackendHandle>>,
    splash_url: Mutex<String>,
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .manage({
            let data_dir = paths::data_dir();
            fs::create_dir_all(&data_dir).expect("无法创建数据目录");
            AppState { data_dir, backend: Mutex::new(None), splash_url: Mutex::new(String::new()) }
        })
        .invoke_handler(tauri::generate_handler![
            first_run_state,
            start_backend,
            backend_log,
            pick_db_file,
            do_import
        ])
        .setup(|app| {
            // 主窗口运行时创建:需要挂 initialization_script(外链接管)
            let w = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("GeekWaves")
            .inner_size(1280.0, 800.0)
            .min_inner_size(1024.0, 700.0)
            .initialization_script(EXTERNAL_LINKS_SCRIPT)
            .build()?;
            // 捕获 splash 绝对地址:成功切到后端页后,崩溃守护需要用绝对地址导航回 splash(#crash)
            *app.state::<AppState>().splash_url.lock().unwrap() =
                w.url().map(|u| u.to_string()).unwrap_or_default();
            // 非首启:后台线程直接启动(splash 显示 loading 态)
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let first = {
                    let state: tauri::State<AppState> = handle.state();
                    is_first_run(&state.data_dir)
                };
                if !first && let Err(e) = run_startup_sequence(&handle) {
                    emit_backend_error(&handle, &e);
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关窗即退出整个 app(含后端子进程);macOS 同口径
            if matches!(event, tauri::WindowEvent::Destroyed) && window.label() == "main" {
                let app = window.app_handle();
                let state: tauri::State<AppState> = app.state();
                if let Some(mut b) = state.backend.lock().unwrap().take() {
                    backend::graceful_shutdown(&mut b.child, &paths::pid_file(&state.data_dir));
                }
                app.exit(0);
            }
        })
        .build(tauri::generate_context!())
        .expect("GeekWaves 桌面壳启动失败")
        .run(|app, event| {
            // 兜底清理:macOS Cmd+Q/App quits 走 NSApplication terminate,不触发窗口 Destroyed
            // (Task 3 本机端到端实修:osascript quit 后 java 孤儿残留)。Exit 事件收尾,
            // 正常关窗路径已先取走句柄,此处为 None 直接跳过。
            if matches!(event, tauri::RunEvent::Exit) {
                let state: tauri::State<AppState> = app.state();
                if let Some(mut b) = state.backend.lock().unwrap().take() {
                    backend::graceful_shutdown(&mut b.child, &paths::pid_file(&state.data_dir));
                }
            }
        });
}

/// 首启判定:数据目录无 *.mv.db 且无 imported 标记
fn is_first_run(data_dir: &Path) -> bool {
    let has_db = fs::read_dir(data_dir)
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                e.file_name().to_string_lossy().ends_with(".mv.db")
            })
        })
        .unwrap_or(false);
    !has_db && !paths::imported_marker(data_dir).exists()
}

/// 完整启动序列(幂等入口见 start_backend 命令):
/// 孤儿清理 → 密钥 → 端口 → spawn → pid → 健康检查(60s)→ 句柄入库 → imported 标记 → 导航
fn run_startup_sequence(app: &AppHandle) -> Result<(), String> {
    let state: tauri::State<AppState> = app.state();
    let data_dir = state.data_dir.clone();

    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("资源目录解析失败: {e}"))?;
    pidfile::cleanup_orphan(&paths::pid_file(&data_dir), &resource_dir.to_string_lossy());

    let key = keygen::load_or_create(&paths::key_file(&data_dir))
        .map_err(|e| format!("读取/生成加密密钥失败: {e}"))?;
    let port = port::pick_free_port().ok_or_else(|| {
        "端口 8977-8999 全部被占用,请释放后重试(或检查是否有残留 GeekWaves 后端)".to_string()
    })?;

    if let Some(parent) = paths::log_file(&data_dir).parent() {
        fs::create_dir_all(parent).map_err(|e| format!("日志目录创建失败: {e}"))?;
    }
    let mut child = backend::spawn_backend(
        &paths::java_bin(&resource_dir),
        &paths::jar_path(&resource_dir),
        &paths::webapp_dir(&resource_dir),
        &data_dir,
        port,
        &key,
        &paths::log_file(&data_dir),
    )
    .map_err(|e| format!("后端进程启动失败: {e}"))?;
    pidfile::write_pid(&paths::pid_file(&data_dir), child.id());

    if let Err(e) = backend::wait_healthy(port, Duration::from_secs(60)) {
        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_file(paths::pid_file(&data_dir));
        return Err(e);
    }
    *state.backend.lock().unwrap() = Some(BackendHandle { child, port });
    let _ = fs::write(paths::imported_marker(&data_dir), b"");

    // 运行中守护:500ms 轮询子进程;意外退出 → 清 pid → 回 splash 错误屏(#crash,splash 读 hash 呈现并取日志)
    let watch = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(500));
        let state = watch.state::<AppState>();
        let mut guard = state.backend.lock().unwrap();
        match &mut *guard {
            None => return, // 正常退出路径已取走句柄,守护结束
            Some(h) => {
                if matches!(h.child.try_wait(), Ok(Some(_))) {
                    *guard = None;
                    drop(guard);
                    let data_dir = watch.state::<AppState>().data_dir.clone();
                    let _ = fs::remove_file(paths::pid_file(&data_dir));
                    let splash_url = watch.state::<AppState>().splash_url.lock().unwrap().clone();
                    if let Some(w) = watch.get_webview_window("main") {
                        let _ = w.eval(&format!("location.replace('{splash_url}#crash')"));
                    }
                    return;
                }
            }
        }
    });

    if let Some(w) = app.get_webview_window("main") {
        let _ = w.eval(&format!("location.replace('http://127.0.0.1:{port}/')"));
    }
    Ok(())
}

fn emit_backend_error(app: &AppHandle, message: &str) {
    let state: tauri::State<AppState> = app.state();
    let log = backend::log_tail(&paths::log_file(&state.data_dir), 50);
    let _ = app.emit(
        "backend-error",
        serde_json::json!({ "message": message, "log": log }),
    );
}

#[tauri::command]
fn first_run_state(app: AppHandle) -> bool {
    let state: tauri::State<AppState> = app.state();
    is_first_run(&state.data_dir)
}

/// 全新开始 / 重试 共用:load_or_create 幂等(已有 key 复用)
#[tauri::command]
fn start_backend(app: AppHandle) -> Result<(), String> {
    run_startup_sequence(&app).map_err(|e| {
        emit_backend_error(&app, &e);
        e
    })
}

/// 错误屏取日志尾部(崩溃守护回 splash 后由其调用)
#[tauri::command]
fn backend_log(app: AppHandle) -> String {
    let state: tauri::State<AppState> = app.state();
    backend::log_tail(&paths::log_file(&state.data_dir), 50)
}

/// 系统文件选择器挑 .mv.db(阻塞式,命令跑在工作线程不卡 UI)
#[tauri::command]
fn pick_db_file(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .add_filter("H2 数据库文件", &["mv.db"])
        .blocking_pick_file()
        .map(|p| p.to_string())
}

/// 导入:校验 key 与源文件 → 拷库 → 落 key(0600)→ 走启动序列
#[tauri::command]
fn do_import(app: AppHandle, db_path: String, key: String) -> Result<(), String> {
    let state: tauri::State<AppState> = app.state();
    keygen::validate_key(&key)?;
    let src = Path::new(&db_path);
    import::validate_source(src)?;
    import::copy_db(src, &state.data_dir)?;
    keygen::write_private(&paths::key_file(&state.data_dir), &key)
        .map_err(|e| format!("密钥写入失败: {e}"))?;
    run_startup_sequence(&app).map_err(|e| {
        emit_backend_error(&app, &e);
        e
    })
}
