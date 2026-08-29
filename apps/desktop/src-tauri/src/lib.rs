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
mod radar;
mod refresh;
mod storage;
mod windows;

use tauri::{Emitter, Manager};

fn start_auto_refresh(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last = 0_i64;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let Some(database) = app.try_state::<storage::database::Database>() else {
                continue;
            };
            let minutes = database
                .setting_string("refresh_interval_minutes")
                .ok()
                .flatten()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(15);
            if minutes == 0 {
                continue;
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            if now - last < (minutes as i64) * 60 {
                continue;
            }
            last = now;
            if let Some(coordinator) = app.try_state::<refresh::RefreshCoordinator>() {
                if coordinator.refresh_all(&database).await.is_ok() {
                    let _ = app.emit("platform-data-changed", ());
                }
            }
        }
    });
}

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
            commands::window_commands::open_external_url,
            commands::platform_commands::remove_user_platform,
            commands::platform_commands::reveal_source_secret,
            commands::platform_commands::test_api_endpoints,
            commands::radar_commands::get_radar_snapshot,
            commands::radar_commands::run_radar_check,
            commands::radar_commands::translate_radar_post,
            commands::radar_commands::save_radar_analysis_prefs,
            commands::settings_commands::get_app_settings,
            commands::settings_commands::set_app_theme,
            commands::settings_commands::set_refresh_interval,
            commands::settings_commands::set_autostart,
            commands::settings_commands::reorder_platforms,
            commands::settings_commands::set_hoverbar_sort_mode,
            commands::settings_commands::clear_local_cache,
            commands::settings_commands::refresh_all_platforms,
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
            start_auto_refresh(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
