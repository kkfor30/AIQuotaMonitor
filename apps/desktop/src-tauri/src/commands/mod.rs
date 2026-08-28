pub mod platform_commands;
pub mod radar_commands;
pub mod settings_commands;
pub mod window_commands;

use tauri::WebviewWindow;

/// 命令调用方窗口校验：IPC 白名单之外的第二层防线。
pub fn require_label(window: &WebviewWindow, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&window.label()) {
        Ok(())
    } else {
        Err(format!("窗口 {} 无权调用该命令", window.label()))
    }
}
