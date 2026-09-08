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
    if let Some(runtime) = app.try_state::<HoverbarRuntime>() {
        if runtime.is_dragging.load(Ordering::SeqCst) {
            return Err("正在拖拽小球，取消展开详情".to_string());
        }
    }
    let anchor = storage::load_preferences(&app).anchor;
    let detail = hoverbar::ensure_hoverbar_detail_window(&app)?;
    let (width, height) = app
        .try_state::<HoverbarRuntime>()
        .map(|runtime| runtime.current_detail_size_for_edge(&anchor.edge))
        .unwrap_or_else(|| {
            if matches!(anchor.edge.as_str(), "left" | "right") {
                (300.0, 420.0)
            } else {
                (420.0, 360.0)
            }
        });
    hoverbar::apply_detail_layout(&detail, &window, &anchor, width, height)?;
    if let Some(runtime) = app.try_state::<HoverbarRuntime>() {
        if runtime.is_dragging.load(Ordering::SeqCst) {
            return Err("正在拖拽小球，取消展开详情".to_string());
        }
    }
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

/// 立即隐藏悬浮详情窗口（跳过退出动画，供拖拽或极速收起使用）。悬浮球或详情窗口可调用。
#[tauri::command]
pub fn hide_hoverbar_detail_immediately(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<(), String> {
    require_label(&window, &["hoverbar", "hoverbar-detail"])?;
    // 迟到的前端清理请求不能在新一轮原生拖动中改变窗口捕获。
    if app.state::<HoverbarRuntime>().is_dragging.load(Ordering::SeqCst) {
        return Ok(());
    }
    if let Some(detail) = app.get_webview_window("hoverbar-detail") {
        let _ = detail.hide();
    }
    if let Some(runtime) = app.try_state::<HoverbarRuntime>() {
        runtime.detail_visible.store(false, Ordering::SeqCst);
    }
    let _ = app.emit("hoverbar-detail-close-immediate", ());
    let _ = app.emit("hoverbar-detail-visibility", false);
    Ok(())
}

/// 标记悬浮球正在被拖拽或拖拽结束。仅悬浮球窗口可调用。
#[tauri::command]
pub fn set_hoverbar_dragging(
    app: AppHandle,
    window: WebviewWindow,
    dragging: bool,
) -> Result<(), String> {
    require_label(&window, &["hoverbar"])?;
    if let Some(runtime) = app.try_state::<HoverbarRuntime>() {
        runtime.is_dragging.store(dragging, Ordering::SeqCst);
        if dragging {
            let pos = window.outer_position().ok().map(|p| (p.x, p.y));
            if let Ok(mut lock) = runtime.drag_start_pos.lock() {
                *lock = pos;
            }
            if let Some(detail) = app.get_webview_window("hoverbar-detail") {
                let _ = detail.hide();
            }
            runtime.detail_visible.store(false, Ordering::SeqCst);
            let _ = app.emit("hoverbar-detail-close-immediate", ());
            let _ = app.emit("hoverbar-detail-visibility", false);
        } else if let Ok(mut lock) = runtime.drag_start_pos.lock() {
            *lock = None;
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverbarDragResult {
    anchor: HoverbarAnchor,
    moved: bool,
}

/// 在窗口线程内串行隐藏面板、进入原生拖动、松手后吸附。
/// 不使用前端多个并发 IPC，也不把 startDragging 请求入队当作拖动结束。
#[tauri::command]
pub async fn drag_hoverbar(
    app: AppHandle,
    window: WebviewWindow,
    pointer_x: f64,
    pointer_y: f64,
) -> Result<HoverbarDragResult, String> {
    require_label(&window, &["hoverbar"])?;
    if !pointer_x.is_finite() || !pointer_y.is_finite() {
        return Err("拖动起点无效".into());
    }
    // 离开 WebView 的 IPC 回调栈后再进入系统模态循环，避免回调重入。
    let (send, receive) = tokio::sync::oneshot::channel();
    let drag_window = window.clone();
    window.run_on_main_thread(move || {
        let _ = send.send(drag_hoverbar_on_main(app, drag_window, pointer_x, pointer_y));
    }).map_err(|error| error.to_string())?;
    receive.await.map_err(|_| "悬浮球拖动任务已结束".to_string())?
}

fn drag_hoverbar_on_main(
    app: AppHandle,
    window: WebviewWindow,
    pointer_x: f64,
    pointer_y: f64,
) -> Result<HoverbarDragResult, String> {
    if app.state::<HoverbarRuntime>().is_dragging.load(Ordering::SeqCst) {
        return Err("悬浮球正在拖动".into());
    }
    set_hoverbar_dragging(app.clone(), window.clone(), true)?;
    let result = (|| {
        let before = window.outer_position().map_err(|error| error.to_string())?;
        drag_hoverbar_native(&window, pointer_x, pointer_y)?;
        let after = window.outer_position().map_err(|error| error.to_string())?;
        let moved = before != after;
        let anchor = if moved {
            snap_and_persist(&window)?
        } else {
            storage::load_preferences(&app).anchor
        };
        Ok(HoverbarDragResult { anchor, moved })
    })();
    // 无论原生调用或吸附是否失败，都允许下一次按下重新拖动。
    let _ = set_hoverbar_dragging(app, window, false);
    result
}

#[cfg(target_os = "windows")]
fn drag_hoverbar_native(window: &WebviewWindow, pointer_x: f64, pointer_y: f64) -> Result<(), String> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, ReleaseCapture, VK_LBUTTON};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, SendMessageW, SetWindowPos, HTCAPTION, SWP_NOACTIVATE,
        SWP_NOSIZE, SWP_NOZORDER, WM_NCLBUTTONDOWN,
    };
    let hwnd = window.hwnd().map_err(|error| error.to_string())?.0;
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    unsafe {
        // IPC 到达前已经松手的短点击不能重新进入系统拖动循环。
        if GetAsyncKeyState(VK_LBUTTON as i32) >= 0 { return Ok(()); }
        let mut cursor = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut cursor) == 0 { return Err(std::io::Error::last_os_error().to_string()); }
        // 鼠标可能已移出 40px 小球；保留按下时的抓取点，补上 IPC 期间的位移。
        let x = cursor.x - (pointer_x.clamp(0.0, 40.0) * scale).round() as i32;
        let y = cursor.y - (pointer_y.clamp(0.0, 40.0) * scale).round() as i32;
        if SetWindowPos(hwnd, std::ptr::null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        ReleaseCapture();
        // LPARAM 是打包的有符号屏幕坐标，不能传局部 POINTS 的指针。
        // SendMessage 在松手/取消退出系统模态拖动后返回，期间不轮询、不逐帧 IPC。
        SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as usize, drag_message_position(cursor.x, cursor.y));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn drag_hoverbar_native(_window: &WebviewWindow, _pointer_x: f64, _pointer_y: f64) -> Result<(), String> {
    Err("当前系统尚不支持此悬浮球拖动方式".into())
}

#[cfg(any(target_os = "windows", test))]
fn drag_message_position(x: i32, y: i32) -> isize {
    ((x as u16 as u32) | ((y as u16 as u32) << 16)) as isize
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
    let mut prefs = storage::load_preferences(&app);
    let anchor = prefs.anchor.clone();
    if let Some(runtime) = app.try_state::<HoverbarRuntime>() {
        runtime.update_detail_size_for_edge(&anchor.edge, height);
        if runtime.is_dragging.load(Ordering::SeqCst) || !runtime.detail_visible.load(Ordering::SeqCst) {
            return Ok(anchor);
        }
    }
    // 尺寸偏好随使用更新（下次展开沿用）
    prefs.detail_size.width = width;
    prefs.detail_size.height = height;
    storage::save_preferences(&app, &prefs);

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
        let res = snap_and_persist(&window);
        if let Some(runtime) = window.app_handle().try_state::<HoverbarRuntime>() {
            runtime.is_dragging.store(false, Ordering::SeqCst);
        }
        res
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

    // 广播给前端各窗口，通知小球吸附到了新锚点边缘
    let _ = app.emit("hoverbar-anchor-changed", &anchor);

    // 若详情面板窗口存在且当前处于隐藏状态，在后台静默预先更新其尺寸与停靠位置
    if let Some(detail) = app.get_webview_window("hoverbar-detail") {
        if let Some(runtime) = app.try_state::<HoverbarRuntime>() {
            if !runtime.detail_visible.load(Ordering::SeqCst) {
                let (width, height) = runtime.current_detail_size_for_edge(&anchor.edge);
                let _ = hoverbar::apply_detail_layout(&detail, window, &anchor, width, height);
            }
        }
    }
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
    use super::{drag_message_position, validate_http_url};

    #[test]
    fn drag_message_packs_screen_coordinates_including_negative_monitors() {
        for (x, y) in [(125, 360), (-1920, 200), (200, -1080), (-2500, -1200)] {
            let packed = drag_message_position(x, y);
            assert_eq!(packed as u16 as i16 as i32, x);
            assert_eq!((packed >> 16) as u16 as i16 as i32, y);
        }
    }

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
