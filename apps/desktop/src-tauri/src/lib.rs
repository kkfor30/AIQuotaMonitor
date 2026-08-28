//! AIQuotaMonitor 桌面应用 Rust 侧入口。
//!
//! 模块职责：
//! - commands：Tauri IPC 命令（窗口命令 + 平台数据命令）
//! - domain：前后端共享 ViewModel 契约
//! - providers：平台 Provider 注册表（阶段一为静态数据）
//! - storage：本地持久化（阶段一仅悬浮球偏好 JSON）
//! - windows：悬浮球等特殊窗口的几何与生命周期

mod commands;
mod domain;
mod providers;
mod refresh;
mod storage;
mod windows;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::platform_commands::get_platform_summaries,
            commands::platform_commands::list_platform_catalog,
            commands::platform_commands::get_platform_setup,
            commands::platform_commands::add_user_platforms,
            commands::platform_commands::refresh_platform,
            commands::platform_commands::validate_source_credential,
            commands::platform_commands::save_platform_setup,
            commands::platform_commands::save_source_credential,
            commands::platform_commands::clear_source_credential,
            commands::platform_commands::start_source_login,
            commands::platform_commands::add_codex_account,
            commands::platform_commands::remove_codex_account,
            commands::platform_commands::close_source_login,
            commands::platform_commands::inspect_legacy_config,
            commands::platform_commands::import_legacy_config,
            commands::window_commands::show_hoverbar_detail,
            commands::window_commands::request_hide_hoverbar_detail,
            commands::window_commands::finish_hide_hoverbar_detail,
            commands::window_commands::set_hoverbar_detail_size,
            commands::window_commands::set_hoverbar_detail_pointer_inside,
            commands::window_commands::snap_hoverbar_to_edge,
            commands::window_commands::open_main_window,
            commands::window_commands::get_hoverbar_preferences,
            commands::window_commands::set_hoverbar_enabled,
        ])
        .setup(|app| {
            let database = storage::database::Database::initialize(app.handle())
                .map_err(std::io::Error::other)?;
            app.manage(database);
            let refresh = refresh::RefreshCoordinator::new().map_err(std::io::Error::other)?;
            app.manage(refresh);

            let prefs = storage::load_preferences(app.handle());
            app.manage(windows::hoverbar::HoverbarRuntime::new((
                prefs.detail_size.width,
                prefs.detail_size.height,
            )));

            // 主窗口关闭即退出整个应用（阶段一无托盘）
            if let Some(main_window) = app.get_webview_window("main") {
                let app_handle = app.handle().clone();
                main_window.on_window_event(move |event| {
                    if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                        app_handle.exit(0);
                    }
                });
            }

            if prefs.enabled {
                windows::hoverbar::ensure_hoverbar_windows(app.handle())?;
            }
            windows::hoverbar::start_fullscreen_watcher(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
