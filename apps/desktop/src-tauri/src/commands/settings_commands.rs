use crate::commands::require_label;
use crate::radar;
use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State, WebviewWindow};

const WEBVIEW_IDENTIFIER: &str = "com.aiquotamonitor.desktop";
const CREDENTIAL_STORE_LABEL: &str =
    "Windows 凭据管理器 · Windows 凭据 · 通用凭据（目标名以 AIQuotaMonitor/ 开头）";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDataLocationsView {
    pub app_data_dir: String,
    pub database_path: String,
    pub web_sessions_dir: String,
    pub extra_codex_dir: String,
    pub webview_dir: String,
    pub credential_store: String,
    pub codex_cli_dir: String,
    pub claude_cli_dir: String,
    pub grok_cli_dir: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsView {
    pub theme: String,
    pub autostart: bool,
    pub refresh_interval_minutes: i64,
    pub hoverbar_sort_mode: String,
    pub hoverbar_auto_radar_check: bool,
    pub local_data: LocalDataLocationsView,
}

#[tauri::command]
pub fn get_app_settings(database: State<'_, Database>) -> Result<AppSettingsView, String> {
    // 只保留浅/深两档：旧库里的 "system" 或空值一律归一为浅色。
    let saved_theme = database.setting_string("theme")?;
    let theme = if saved_theme.as_deref() == Some("dark") {
        "dark".to_string()
    } else {
        "light".to_string()
    };
    Ok(AppSettingsView {
        theme,
        autostart: database.setting_bool("autostart")?,
        refresh_interval_minutes: database
            .setting_string("refresh_interval_minutes")?
            .and_then(|value| value.parse().ok())
            .unwrap_or(15),
        hoverbar_sort_mode: database
            .setting_string("hoverbar_sort_mode")?
            .unwrap_or_else(|| "manual".into()),
        hoverbar_auto_radar_check: database.setting_bool("hoverbar_auto_radar_check")?,
        local_data: local_data_locations(database.path()),
    })
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn env_dir(key: &str, fallback: &str) -> PathBuf {
    std::env::var_os(key)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(fallback))
}

pub(crate) fn local_data_locations(database_path: &Path) -> LocalDataLocationsView {
    let app_data_dir = database_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| database_path.to_path_buf());
    let profile = env_dir("USERPROFILE", r"%USERPROFILE%");
    let local_app_data = env_dir("LOCALAPPDATA", r"%LOCALAPPDATA%");
    LocalDataLocationsView {
        app_data_dir: path_string(&app_data_dir),
        database_path: path_string(database_path),
        web_sessions_dir: path_string(&app_data_dir.join("web-sessions")),
        extra_codex_dir: path_string(&app_data_dir.join("codex-accounts")),
        webview_dir: path_string(&local_app_data.join(WEBVIEW_IDENTIFIER)),
        credential_store: CREDENTIAL_STORE_LABEL.into(),
        codex_cli_dir: path_string(&profile.join(".codex")),
        claude_cli_dir: path_string(&profile.join(".claude")),
        grok_cli_dir: path_string(&profile.join(".grok")),
    }
}

#[tauri::command]
pub fn open_local_data_dir(
    window: WebviewWindow,
    database: State<'_, Database>,
) -> Result<(), String> {
    require_label(&window, &["main"])?;
    let path = local_data_locations(database.path()).app_data_dir;
    open_in_explorer(Path::new(&path))
}

fn open_in_explorer(path: &Path) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(path.as_os_str())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开目录：{error}"))
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
    // 主题是全局偏好：主窗口标题栏/设置页与悬浮球都允许切换，切换后广播给所有窗口实时跟随。
    require_label(&window, &["main", "hoverbar", "hoverbar-detail"])?;
    if !matches!(theme.as_str(), "light" | "dark") {
        return Err("不支持的主题".into());
    }
    database.set_setting_string("theme", &theme)?;
    let _ = app.emit("app-settings-changed", ());
    let _ = app.emit(
        "app-theme-changed",
        serde_json::json!({ "theme": theme }),
    );
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
    if !matches!(minutes, 0 | 3 | 5 | 15 | 30 | 60) {
        return Err("刷新间隔只支持关闭、3、5、15、30 或 60 分钟".into());
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
    sync_windows_autostart(enabled)?;
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

const AUTOSTART_RUN_VALUE: &str = "AIQuotaMonitor";

pub(crate) fn sync_windows_autostart(enabled: bool) -> Result<(), String> {
    remove_legacy_startup_bat();
    #[cfg(windows)]
    {
        set_registry_run(enabled)
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("当前系统不支持开机自启".into())
    }
}

fn autostart_command_line(exe: &Path) -> String {
    format!("\"{}\" --autostart", exe.display())
}

fn remove_legacy_startup_bat() {
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return;
    };
    let path = Path::new(&appdata)
        .join(r"Microsoft\Windows\Start Menu\Programs\Startup")
        .join("AIQuotaMonitor.bat");
    let _ = std::fs::remove_file(path);
}

#[cfg(windows)]
fn set_registry_run(enabled: bool) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY_CURRENT_USER,
        KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let subkey = wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let name = wide(AUTOSTART_RUN_VALUE);
    let mut hkey = std::ptr::null_mut();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            std::ptr::null_mut(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut hkey,
            std::ptr::null_mut(),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("无法打开开机自启注册表：{status}"));
    }
    let result = (|| {
        if enabled {
            let exe = std::env::current_exe().map_err(|error| format!("无法定位程序路径：{error}"))?;
            let command = autostart_command_line(&exe);
            let data = wide(&command);
            let bytes = (data.len() * 2) as u32;
            let status = unsafe {
                RegSetValueExW(
                    hkey,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr().cast(),
                    bytes,
                )
            };
            if status != ERROR_SUCCESS {
                return Err(format!("无法写入开机自启：{status}"));
            }
        } else {
            let status = unsafe { RegDeleteValueW(hkey, name.as_ptr()) };
            if status != ERROR_SUCCESS && status != ERROR_FILE_NOT_FOUND {
                return Err(format!("无法关闭开机自启：{status}"));
            }
        }
        Ok(())
    })();
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::autostart_command_line;
    use std::path::Path;

    #[test]
    fn autostart_command_quotes_exe_and_passes_flag() {
        let exe = Path::new(r"C:\Program Files\AIQuotaMonitor\AIQuotaMonitor.exe");
        assert_eq!(
            autostart_command_line(exe),
            r#""C:\Program Files\AIQuotaMonitor\AIQuotaMonitor.exe" --autostart"#
        );
    }

    #[test]
    fn local_data_locations_use_database_parent() {
        let view = super::local_data_locations(Path::new(
            r"C:\Users\demo\AppData\Roaming\com.aiquotamonitor.desktop\ai-quota-monitor.db",
        ));
        assert_eq!(
            view.app_data_dir,
            r"C:\Users\demo\AppData\Roaming\com.aiquotamonitor.desktop"
        );
        assert!(view.database_path.ends_with("ai-quota-monitor.db"));
        assert!(view.web_sessions_dir.ends_with("web-sessions"));
        assert!(view.extra_codex_dir.ends_with("codex-accounts"));
        assert!(view.credential_store.contains("AIQuotaMonitor/"));
        assert!(view.codex_cli_dir.ends_with(".codex"));
        assert!(view.claude_cli_dir.ends_with(".claude"));
        assert!(view.grok_cli_dir.ends_with(".grok"));
        assert!(view.webview_dir.ends_with("com.aiquotamonitor.desktop"));
    }
}
