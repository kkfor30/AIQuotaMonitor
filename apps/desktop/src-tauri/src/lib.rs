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

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

/// 从托盘/其他入口恢复主窗口。
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// 系统托盘：左键点击恢复主窗口；菜单提供「显示主窗口 / 退出」。应用只能从这里真正退出。
fn setup_tray(app: &tauri::App) -> Result<(), tauri::Error> {
    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出 AIQuotaMonitor", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?;
    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("AIQuotaMonitor · 左键点击恢复主窗口")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

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
            commands::platform_commands::add_platform_account,
            commands::platform_commands::refresh_platform,
            commands::platform_commands::validate_source_credential,
            commands::platform_commands::save_platform_setup,
            commands::platform_commands::save_source_credential,
            commands::platform_commands::clear_source_credential,
            commands::platform_commands::start_source_login,
            commands::platform_commands::add_codex_account,
            commands::platform_commands::rename_codex_account,
            commands::platform_commands::remove_codex_account,
            commands::platform_commands::rename_platform_account,
            commands::platform_commands::remove_platform_account,
            commands::platform_commands::close_source_login,
            commands::platform_commands::take_captured_source_secret,
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
            commands::radar_commands::test_radar_model,
            commands::radar_commands::add_radar_custom_model,
            commands::radar_commands::delete_radar_custom_model,
            commands::radar_commands::confirm_radar_quota_change,
            commands::settings_commands::get_app_settings,
            commands::settings_commands::set_app_theme,
            commands::settings_commands::set_refresh_interval,
            commands::settings_commands::set_autostart,
            commands::settings_commands::reorder_platforms,
            commands::settings_commands::set_hoverbar_sort_mode,
            commands::settings_commands::clear_local_cache,
            commands::settings_commands::refresh_all_platforms,
        ])
        .on_window_event(|window, event| {
            // 主窗口点 X 只隐藏（悬浮球继续监控）；其它窗口保持默认关闭行为
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
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

            // 主窗口点 X 隐藏到系统托盘继续监控；应用只能从托盘菜单退出
            setup_tray(app)?;

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
