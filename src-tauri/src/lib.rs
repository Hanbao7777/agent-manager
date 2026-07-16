mod commands;
mod diagnostics;
pub(crate) mod installer;
#[cfg(target_os = "linux")]
mod linux_fix;
mod panic_hook;
mod proxy;
#[cfg(target_os = "windows")]
mod settings;

use tauri::{Emitter, Manager};
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
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    }

    let builder = builder
        .manage(installer::InstallTaskStore::with_persistence(
            panic_hook::get_app_config_dir(),
        ))
        .manage(diagnostics::DiagnosticReportStore::default())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(window_state_flags())
                .build(),
        )
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window
                    .state::<installer::InstallTaskStore>()
                    .active_task()
                    .is_some()
                {
                    api.prevent_close();
                    let _ = window
                        .app_handle()
                        .emit("agent-manager://install-exit-blocked", ());
                } else {
                    api.prevent_close();
                    window.app_handle().exit(0);
                }
            }
        })
        .setup(|app| {
            let _ = rustls::crypto::ring::default_provider().install_default();
            #[cfg(target_os = "windows")]
            set_windows_app_user_model_id(app.handle());

            use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
            let log_dir = panic_hook::get_log_dir();
            std::fs::create_dir_all(&log_dir).map_err(|error| error.to_string())?;
            let _ = std::fs::remove_file(log_dir.join("agent-manager.log"));
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Trace)
                    .targets([
                        Target::new(TargetKind::Stdout),
                        Target::new(TargetKind::Folder {
                            path: log_dir,
                            file_name: Some("agent-manager".into()),
                        }),
                    ])
                    .rotation_strategy(RotationStrategy::KeepSome(2))
                    .max_file_size(1024 * 1024 * 1024)
                    .timezone_strategy(TimezoneStrategy::UseLocal)
                    .build(),
            )?;
            panic_hook::init_app_config_dir(panic_hook::default_app_config_dir());

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
            installer::prepare_tool_install,
            installer::start_tool_install,
            installer::get_install_task,
            installer::get_active_install_task,
            installer::replay_startup_install_recovery,
            installer::cancel_install_task,
            diagnostics::generate_diagnostic_report,
            diagnostics::export_diagnostic_report,
        ]);

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while running tauri application");
    app.run(|_, _| {});
}
