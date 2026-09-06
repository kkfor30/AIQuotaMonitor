//! 悬浮球窗口相关 Tauri Commands。
//!
// 迁移来源：DeepSeekMonitorWindows-final/src-tauri/src/lib.rs（提交 f3ab3ec6）
//   - 194/1562-1762 行的窗口命令与权限校验逻辑
// 变更：
//   - 配置持久化改为 storage::HoverbarPreferences（旧项目为 config.json）
//   - save_hoverbar_enabled 改名 set_hoverbar_enabled，返回 Unit 而非整份 AppConfig
//   - 权限校验统一为 require_label 单函数
// 许可证：MIT（原 DeepSeekMonitorWindows-final，提交 f3ab3ec6）

use crate::commands::require_label;
use crate::storage::{self, HoverbarAnchor};
use crate::windows::hoverbar::{self, HoverbarRuntime};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// 展开悬浮详情：布局详情窗口、显示并广播事件。仅悬浮球窗口可调用。
#[tauri::command]
pub fn show_hoverbar_detail(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<HoverbarAnchor, String> {
    require_label(&window, &["hoverbar"])?;
    let anchor = storage::load_preferences(&app).anchor;
    let detail = hoverbar::ensure_hoverbar_detail_window(&app)?;
    let (width, height) = app
        .try_state::<HoverbarRuntime>()
        .map(|runtime| runtime.current_detail_size())
        .unwrap_or((420.0, 360.0));
    hoverbar::apply_detail_layout(&detail, &window, &anchor, width, height)?;
    let _ = detail.show();
    // 两个独立窗口发生视觉重叠时，小球必须始终位于详情面板上方。
    hoverbar::keep_anchor_above_detail(&window)?;
    let _ = app.emit("hoverbar-detail-open", &anchor);
    let _ = app.emit("hoverbar-detail-visibility", true);
    if let Some(runtime) = app.try_state::<HoverbarRuntime>() {
        runtime.detail_visible.store(true, Ordering::SeqCst);
    }
    Ok(anchor)
}

/// 请求收起详情（先让详情窗口播放退出动画）。悬浮球或详情窗口可调用。
#[tauri::command]
pub fn request_hide_hoverbar_detail(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    require_label(&window, &["hoverbar", "hoverbar-detail"])?;
    app.emit("hoverbar-detail-close", ())
        .map_err(|error| error.to_string())
}

/// 动画结束后真正隐藏详情窗口。仅详情窗口可调用。
#[tauri::command]
pub fn finish_hide_hoverbar_detail(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    require_label(&window, &["hoverbar-detail"])?;
    let _ = window.hide();
    if let Some(runtime) = app.try_state::<HoverbarRuntime>() {
        runtime.detail_visible.store(false, Ordering::SeqCst);
    }
    let _ = app.emit("hoverbar-detail-visibility", false);
    Ok(())
}

/// 详情窗口按内容高度上报期望尺寸并重新布局。仅详情窗口可调用。
#[tauri::command]
pub fn set_hoverbar_detail_size(
    app: AppHandle,
    window: WebviewWindow,
    width: f64,
    height: f64,
) -> Result<HoverbarAnchor, String> {
    require_label(&window, &["hoverbar-detail"])?;
    if let Some(runtime) = app.try_state::<HoverbarRuntime>() {
        if let Ok(mut size) = runtime.detail_size.lock() {
            *size = (width, height);
        }
    }
    // 尺寸偏好随使用更新（下次展开沿用）
    let mut prefs = storage::load_preferences(&app);
    prefs.detail_size.width = width;
    prefs.detail_size.height = height;
    storage::save_preferences(&app, &prefs);

    let anchor = prefs.anchor;
    let Some(anchor_window) = app.get_webview_window("hoverbar") else {
        return Ok(anchor);
    };
    hoverbar::apply_detail_layout(&window, &anchor_window, &anchor, width, height)
}

/// 详情窗口上报指针进出，用于悬浮球侧的延迟收起判断。仅详情窗口可调用。
#[tauri::command]
pub fn set_hoverbar_detail_pointer_inside(
    app: AppHandle,
    window: WebviewWindow,
    inside: bool,
) -> Result<(), String> {
    require_label(&window, &["hoverbar-detail"])?;
    app.emit("hoverbar-detail-pointer", inside)
        .map_err(|error| error.to_string())
}

/// 悬停/拖动结束后吸附最近边缘。仅悬浮球窗口可调用。
/// Windows 下先等待鼠标左键释放再吸附，避免布局抢占拖动过程。
#[tauri::command]
pub async fn snap_hoverbar_to_edge(window: WebviewWindow) -> Result<HoverbarAnchor, String> {
    require_label(&window, &["hoverbar"])?;
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
            while unsafe { GetAsyncKeyState(VK_LBUTTON as i32) } < 0 {
                thread::sleep(Duration::from_millis(16));
            }
        }
        snap_and_persist(&window)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn snap_and_persist(window: &WebviewWindow) -> Result<HoverbarAnchor, String> {
    let anchor = hoverbar::compute_snap_anchor(window)?;
    let app = window.app_handle();
    let mut prefs = storage::load_preferences(&app);
    prefs.anchor = anchor.clone();
    storage::save_preferences(&app, &prefs);
    hoverbar::apply_anchor_layout(window, &anchor)?;
    Ok(anchor)
}

/// 打开（显示并聚焦）主窗口。
#[tauri::command]
pub fn open_main_window(app: AppHandle) {
    show_main_window(&app);
}

/// 读取悬浮球偏好（悬浮球窗口初始化锚点样式用）。
#[tauri::command]
pub fn get_hoverbar_preferences(app: AppHandle) -> Result<storage::HoverbarPreferences, String> {
    Ok(storage::load_preferences(&app))
}

/// 用系统浏览器打开 http(s) 链接。主窗口与悬浮详情都可以调用。
#[tauri::command]
pub fn open_external_url(window: WebviewWindow, url: String) -> Result<(), String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    open_http_url(&url)
}

pub(crate) fn open_http_url(url: &str) -> Result<(), String> {
    let url = validate_http_url(url)?;
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        fn wide(value: &str) -> Vec<u16> {
            value.encode_utf16().chain(std::iter::once(0)).collect()
        }
        let operation = wide("open");
        let file = wide(&url);
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL as i32,
            )
        };
        if result as isize <= 32 {
            return Err("无法打开系统浏览器".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err("当前系统不支持打开外部链接".into())
    }
}

fn validate_http_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return Err("链接无效".into());
    }
    let rest = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .ok_or_else(|| "只能打开 http(s) 链接".to_string())?;
    if rest.is_empty() || rest.contains(' ') {
        return Err("链接无效".into());
    }
    Ok(trimmed.to_string())
}

/// 设置页切换悬浮球开关。仅主窗口可调用。
#[tauri::command]
pub async fn set_hoverbar_enabled(
    app: AppHandle,
    window: WebviewWindow,
    enabled: bool,
) -> Result<(), String> {
    require_label(&window, &["main"])?;
    // 异步执行：窗口创建不能阻塞主线程上的 IPC 调用方
    tauri::async_runtime::spawn_blocking(move || hoverbar::set_hoverbar_enabled(&app, enabled))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::validate_http_url;

    #[test]
    fn accepts_http_s_urls() {
        assert_eq!(
            validate_http_url(" https://open.bigmodel.cn/usercenter/apikeys ").unwrap(),
            "https://open.bigmodel.cn/usercenter/apikeys"
        );
        assert!(validate_http_url("http://example.com").is_ok());
    }

    #[test]
    fn rejects_non_http_urls() {
        assert!(validate_http_url("javascript:alert(1)").is_err());
        assert!(validate_http_url("file:///C:/Windows").is_err());
        assert!(validate_http_url("https://").is_err());
        assert!(validate_http_url("https://evil example").is_err());
    }
}
