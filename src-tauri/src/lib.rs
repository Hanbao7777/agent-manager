mod app_config;
mod app_store;
mod auto_launch;
mod claude_desktop_config;
mod claude_mcp;
mod claude_plugin;
mod codex_config;
mod codex_history_migration;
mod codex_state_db;
mod commands;
mod config;
mod database;
mod deeplink;
mod error;
mod gemini_config;
mod gemini_mcp;
pub mod hermes_config;
mod init_status;
#[cfg(target_os = "linux")]
mod linux_fix;
mod mcp;
mod openclaw_config;
mod opencode_config;
mod panic_hook;
mod prompt;
mod prompt_files;
mod provider;
mod provider_defaults;
mod proxy;
mod services;
mod session_manager;
mod settings;
mod store;
mod usage_events;
mod usage_script;

pub use app_config::AppType;
pub use commands::{
    get_tool_versions, probe_tool_installations, run_tool_lifecycle_action, set_window_theme,
};
pub use database::Database;

use tauri::Emitter;
use tauri::Manager;
#[cfg(target_os = "macos")]
use tauri::RunEvent;
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

#[cfg(target_os = "windows")]
fn set_windows_app_user_model_id(app: &tauri::AppHandle) {
    let app_id = app.config().identifier.clone();
    let wide_app_id: Vec<u16> = app_id.encode_utf16().chain(std::iter::once(0)).collect();
    let result = unsafe {
        windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(wide_app_id.as_ptr())
    };
    if result < 0 {
        log::warn!("设置 Windows AppUserModelID 失败: 0x{result:08X}");
    } else {
        log::debug!("Windows AppUserModelID 已设置为 {app_id}");
    }
}

fn redact_url_for_log(url_str: &str) -> String {
    match url::Url::parse(url_str) {
        Ok(url) => {
            let mut output = format!("{}://", url.scheme());
            if let Some(host) = url.host_str() {
                output.push_str(host);
            }
            output.push_str(url.path());
            let mut keys: Vec<String> = url.query_pairs().map(|(k, _)| k.to_string()).collect();
            keys.sort();
            keys.dedup();
            if !keys.is_empty() {
                output.push_str("?[keys:");
                output.push_str(&keys.join(","));
                output.push(']');
            }
            output
        }
        Err(_) => {
            let base = url_str.split('#').next().unwrap_or(url_str);
            match base.split_once('?') {
                Some((prefix, _)) => format!("{prefix}?[redacted]"),
                None => base.to_string(),
            }
        }
    }
}

/// Parse a deep link and notify the frontend without exposing query values in logs.
fn handle_deeplink_url(
    app: &tauri::AppHandle,
    url_str: &str,
    focus_main_window: bool,
    source: &str,
) -> bool {
    if !url_str.starts_with("ccswitch://") {
        return false;
    }

    log::info!(
        "Deep link URL detected from {source}: {}",
        redact_url_for_log(url_str)
    );
    match crate::deeplink::parse_deeplink_url(url_str) {
        Ok(request) => {
            if let Err(error) = app.emit("deeplink-import", &request) {
                log::error!("Failed to emit deeplink-import event: {error}");
            }
            if focus_main_window {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                    #[cfg(target_os = "linux")]
                    linux_fix::nudge_main_window(window);
                }
            }
        }
        Err(error) => {
            log::error!("Failed to parse deep link URL: {error}");
            let _ = app.emit(
                "deeplink-error",
                serde_json::json!({"url": url_str, "error": error.to_string()}),
            );
        }
    }
    true
}

fn window_state_flags() -> StateFlags {
    StateFlags::POSITION | StateFlags::SIZE | StateFlags::MAXIMIZED
}

pub fn save_window_state_before_exit(app: &tauri::AppHandle) {
    if let Err(error) = app.save_window_state(window_state_flags()) {
        log::error!("退出前保存窗口状态失败: {error}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    panic_hook::setup_panic_hook();
    let mut builder = tauri::Builder::default();

    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            for arg in args {
                if handle_deeplink_url(app, &arg, false, "single_instance args") {
                    break;
                }
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    }

    let builder = builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(window_state_flags())
                .build(),
        )
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                window.app_handle().exit(0);
            }
        })
        .setup(|app| {
            let _ = rustls::crypto::ring::default_provider().install_default();
            #[cfg(target_os = "windows")]
            set_windows_app_user_model_id(app.handle());

            use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
            let log_dir = panic_hook::get_log_dir();
            std::fs::create_dir_all(&log_dir).map_err(|error| error.to_string())?;
            let _ = std::fs::remove_file(log_dir.join("cc-switch.log"));
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Trace)
                    .targets([
                        Target::new(TargetKind::Stdout),
                        Target::new(TargetKind::Folder {
                            path: log_dir,
                            file_name: Some("cc-switch".into()),
                        }),
                    ])
                    .rotation_strategy(RotationStrategy::KeepSome(2))
                    .max_file_size(1024 * 1024 * 1024)
                    .timezone_strategy(TimezoneStrategy::UseLocal)
                    .build(),
            )?;
            panic_hook::init_app_config_dir(crate::config::get_app_config_dir());

            #[cfg(any(target_os = "linux", all(debug_assertions, windows)))]
            if let Err(error) = app.deep_link().register_all() {
                log::error!("Failed to register deep link schemes: {error}");
            }
            app.deep_link().on_open_url({
                let app_handle = app.handle().clone();
                move |event| {
                    for url in event.urls() {
                        if handle_deeplink_url(&app_handle, url.as_str(), true, "on_open_url") {
                            break;
                        }
                    }
                }
            });

            #[cfg(target_os = "linux")]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.with_webview(|webview| {
                    use webkit2gtk::{HardwareAccelerationPolicy, SettingsExt, WebViewExt};
                    if let Some(settings) = WebViewExt::settings(&webview.inner()) {
                        SettingsExt::set_hardware_acceleration_policy(
                            &settings,
                            HardwareAccelerationPolicy::Never,
                        );
                    }
                });
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                #[cfg(target_os = "linux")]
                linux_fix::nudge_main_window(window);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_tool_versions,
            commands::run_tool_lifecycle_action,
            commands::probe_tool_installations,
            commands::set_window_theme,
        ]);

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while running tauri application");
    app.run(|app_handle, event| {
        #[cfg(target_os = "macos")]
        if let RunEvent::Opened { urls } = event {
            if let Some(url) = urls.first() {
                handle_deeplink_url(app_handle, url.as_str(), true, "RunEvent::Opened");
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app_handle, event);
    });
}
