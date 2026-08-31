use crate::commands::require_label;
use crate::radar;
use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsView {
    pub theme: String,
    pub autostart: bool,
    pub refresh_interval_minutes: i64,
    pub hoverbar_sort_mode: String,
    pub hoverbar_auto_radar_check: bool,
}

#[tauri::command]
pub fn get_app_settings(database: State<'_, Database>) -> Result<AppSettingsView, String> {
    Ok(AppSettingsView {
        theme: database
            .setting_string("theme")?
            .unwrap_or_else(|| "system".into()),
        autostart: database.setting_bool("autostart")?,
        refresh_interval_minutes: database
            .setting_string("refresh_interval_minutes")?
            .and_then(|value| value.parse().ok())
            .unwrap_or(15),
        hoverbar_sort_mode: database
            .setting_string("hoverbar_sort_mode")?
            .unwrap_or_else(|| "manual".into()),
        hoverbar_auto_radar_check: database.setting_bool("hoverbar_auto_radar_check")?,
    })
}

#[tauri::command]
pub fn set_hoverbar_auto_radar_check(
    enabled: bool,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<AppSettingsView, String> {
    require_label(&window, &["main"])?;
    database.set_setting_bool("hoverbar_auto_radar_check", enabled)?;
    let _ = app.emit("app-settings-changed", ());
    get_app_settings(database)
}

#[tauri::command]
pub fn set_app_theme(
    theme: String,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<AppSettingsView, String> {
    require_label(&window, &["main"])?;
    if !matches!(theme.as_str(), "light" | "dark" | "system") {
        return Err("不支持的主题".into());
    }
    database.set_setting_string("theme", &theme)?;
    let _ = app.emit("app-settings-changed", ());
    get_app_settings(database)
}

#[tauri::command]
pub fn set_refresh_interval(
    minutes: i64,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<AppSettingsView, String> {
    require_label(&window, &["main"])?;
    if !matches!(minutes, 0 | 5 | 15 | 30) {
        return Err("刷新间隔只支持关闭、5、15 或 30 分钟".into());
    }
    database.set_setting_string("refresh_interval_minutes", &minutes.to_string())?;
    let _ = app.emit("app-settings-changed", ());
    get_app_settings(database)
}

#[tauri::command]
pub fn set_autostart(
    enabled: bool,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<AppSettingsView, String> {
    require_label(&window, &["main"])?;
    set_windows_autostart(enabled)?;
    database.set_setting_bool("autostart", enabled)?;
    let _ = app.emit("app-settings-changed", ());
    get_app_settings(database)
}

#[tauri::command]
pub fn reorder_platforms(
    platform_ids: Vec<String>,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<(), String> {
    require_label(&window, &["main"])?;
    database.reorder_user_platforms(&platform_ids)?;
    let _ = app.emit("app-settings-changed", ());
    let _ = app.emit("platform-data-changed", ());
    Ok(())
}

#[tauri::command]
pub fn set_hoverbar_sort_mode(
    mode: String,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<AppSettingsView, String> {
    require_label(&window, &["main"])?;
    if !matches!(mode.as_str(), "manual" | "smart") {
        return Err("排序模式只支持手动或智能".into());
    }
    database.set_setting_string("hoverbar_sort_mode", &mode)?;
    let _ = app.emit("app-settings-changed", ());
    get_app_settings(database)
}

#[tauri::command]
pub fn clear_local_cache(
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<(), String> {
    require_label(&window, &["main"])?;
    database.clear_cached_snapshots()?;
    let _ = app.emit("platform-data-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn refresh_all_platforms(
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
    coordinator: State<'_, RefreshCoordinator>,
) -> Result<(), String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    coordinator.refresh_all(&database).await?;
    radar::reconcile_event_state_now(&database)?;
    let _ = app.emit("platform-data-changed", ());
    Ok(())
}

fn set_windows_autostart(enabled: bool) -> Result<(), String> {
    let startup = startup_dir()?;
    fs::create_dir_all(&startup).map_err(|error| format!("无法创建启动目录：{error}"))?;
    let path = startup.join("AIQuotaMonitor.bat");
    if enabled {
        let exe = std::env::current_exe().map_err(|error| format!("无法定位程序路径：{error}"))?;
        let script = format!("@echo off\r\nstart \"\" \"{}\"\r\n", exe.display());
        fs::write(&path, script).map_err(|error| format!("写入开机自启失败：{error}"))?;
    } else if path.exists() {
        fs::remove_file(&path).map_err(|error| format!("关闭开机自启失败：{error}"))?;
    }
    Ok(())
}

fn startup_dir() -> Result<PathBuf, String> {
    let appdata = std::env::var("APPDATA").map_err(|_| "无法定位 APPDATA".to_string())?;
    Ok(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\Startup"))
}
