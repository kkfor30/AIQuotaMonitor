//! 悬浮球窗口：40x40 锚点小球 + 独立详情面板双窗口。
//!
// 迁移来源：DeepSeekMonitorWindows-final（提交 f3ab3ec6）
//   - src-tauri/src/hoverbar.rs（logical_size 尺寸规则，整体迁移）
//   - src-tauri/src/lib.rs 1380-1953 行（布局、边缘吸附、多显示器定位、
//     全屏检测、窗口创建，拆出并迁移到本模块）
// 变更：
//   - Tauri 命令拆分至 commands/window_commands.rs，本模块只保留窗口几何与生命周期
//   - 配置持久化由旧项目 config.json 改为 storage::HoverbarPreferences
//   - 移除旧版单窗口 HoverbarApp 相关死代码与托盘联动
// 许可证：MIT（原 DeepSeekMonitorWindows-final，提交 f3ab3ec6）

use crate::storage::{self, HoverbarAnchor};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, Manager, Monitor, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

/// 悬浮窗口的运行时状态（区别于可持久化偏好）。
pub struct HoverbarRuntime {
    /// 详情面板当前是否可见（全屏隐藏、收起动画均会置回 false）
    pub detail_visible: AtomicBool,
    /// 详情面板逻辑尺寸（前端按内容高度上报）
    pub detail_size: Mutex<(f64, f64)>,
}

impl HoverbarRuntime {
    pub fn new(detail_size: (f64, f64)) -> Self {
        Self {
            detail_visible: AtomicBool::new(false),
            detail_size: Mutex::new(detail_size),
        }
    }

    pub fn current_detail_size(&self) -> (f64, f64) {
        self.detail_size
            .lock()
            .map(|s| *s)
            .unwrap_or((420.0, 360.0))
    }
}

/// 悬浮窗口的视觉状态：锚点小球或展开详情。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HoverbarWindowState {
    Anchor,
    Detail,
}

/// 悬浮窗口逻辑尺寸规则（与前端 measureHoverbar 镜像）：
/// - 锚点小球恒为 40x40
/// - 详情面板：left/right 停靠宽 300，top/bottom 停靠宽 420，高度按内容 clamp
pub fn logical_size(
    position: &str,
    state: HoverbarWindowState,
    requested_width: Option<f64>,
    requested_height: Option<f64>,
) -> (f64, f64) {
    match (position, state) {
        (_, HoverbarWindowState::Anchor) => (40.0, 40.0),
        ("left" | "right", HoverbarWindowState::Detail) => (
            requested_width.unwrap_or(300.0).clamp(300.0, 300.0),
            requested_height.unwrap_or(180.0).clamp(180.0, 480.0),
        ),
        (_, HoverbarWindowState::Detail) => (
            requested_width.unwrap_or(420.0).clamp(420.0, 420.0),
            requested_height.unwrap_or(180.0).clamp(180.0, 420.0),
        ),
    }
}

/// Windows 下用一次 SetWindowPos 同步位置与尺寸（物理像素，避免分步调用的闪烁）。
#[cfg(target_os = "windows")]
fn set_window_bounds(
    window: &WebviewWindow,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    _logical_width: f64,
    _logical_height: f64,
) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER,
    };

    let hwnd = window.hwnd().map_err(|error| error.to_string())?.0;
    let changed = unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER,
        )
    };
    if changed == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn set_window_bounds(
    window: &WebviewWindow,
    x: i32,
    y: i32,
    _width: i32,
    _height: i32,
    logical_width: f64,
    logical_height: f64,
) -> Result<(), String> {
    use tauri::{LogicalSize, PhysicalPosition};
    window
        .set_size(tauri::Size::Logical(LogicalSize::new(
            logical_width,
            logical_height,
        )))
        .map_err(|error| error.to_string())?;
    window
        .set_position(tauri::Position::Physical(PhysicalPosition::new(x, y)))
        .map_err(|error| error.to_string())
}

/// 两个独立窗口视觉重叠时，小球必须始终位于详情面板上方。
#[cfg(target_os = "windows")]
pub fn keep_anchor_above_detail(window: &WebviewWindow) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    let hwnd = window.hwnd().map_err(|error| error.to_string())?.0;
    let changed = unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        )
    };
    if changed == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn keep_anchor_above_detail(window: &WebviewWindow) -> Result<(), String> {
    window
        .set_always_on_top(true)
        .map_err(|error| error.to_string())
}

/// 悬浮球所在显示器：优先当前显示器，失败回退主显示器（多显示器支持）。
pub fn monitor_for(window: &WebviewWindow) -> Result<Monitor, String> {
    window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or_else(|| "无法获取显示器信息".to_string())
}

/// 按锚点重新布局小球窗口：40x40，嵌入屏幕边缘 4 物理像素（半隐藏效果），
/// 正交轴 clamp 到工作区内防止越界。
pub fn apply_anchor_layout(window: &WebviewWindow, anchor: &HoverbarAnchor) -> Result<(), String> {
    let monitor = monitor_for(window)?;
    let work_area = monitor.work_area();
    let scale = monitor.scale_factor();
    let (logical_w, logical_h) =
        logical_size(&anchor.edge, HoverbarWindowState::Anchor, None, None);
    let w = (logical_w * scale).round() as i32;
    let h = (logical_h * scale).round() as i32;
    let wa_x = work_area.position.x;
    let wa_y = work_area.position.y;
    let wa_w = work_area.size.width as i32;
    let wa_h = work_area.size.height as i32;
    let right = wa_x + wa_w;
    let bottom = wa_y + wa_h;
    let tucked = (4.0 * scale).round() as i32;
    let center_x = wa_x + ((wa_w as f64) * anchor.ratio).round() as i32;
    let center_y = wa_y + ((wa_h as f64) * anchor.ratio).round() as i32;
    let clamp_axis = |value: i32, start: i32, length: i32, size: i32| {
        value.clamp(start, (start + length - size).max(start))
    };
    let (x, y) = match anchor.edge.as_str() {
        "left" => (wa_x - tucked, clamp_axis(center_y - h / 2, wa_y, wa_h, h)),
        "right" => (
            right - w + tucked,
            clamp_axis(center_y - h / 2, wa_y, wa_h, h),
        ),
        "bottom" => (
            clamp_axis(center_x - w / 2, wa_x, wa_w, w),
            bottom - h + tucked,
        ),
        _ => (clamp_axis(center_x - w / 2, wa_x, wa_w, w), wa_y - tucked),
    };
    set_window_bounds(window, x, y, w, h, logical_w, logical_h)
}

/// 按锚点布局详情面板：与停靠边保持 12 逻辑像素内缩（让小球约一半嵌入面板边缘）。
pub fn apply_detail_layout(
    detail: &WebviewWindow,
    anchor_window: &WebviewWindow,
    anchor: &HoverbarAnchor,
    requested_width: f64,
    requested_height: f64,
) -> Result<HoverbarAnchor, String> {
    let monitor = monitor_for(anchor_window)?;
    let work_area = monitor.work_area();
    let scale = monitor.scale_factor();
    let (logical_w, logical_h) = logical_size(
        &anchor.edge,
        HoverbarWindowState::Detail,
        Some(requested_width),
        Some(requested_height),
    );
    let w = (logical_w * scale).round() as i32;
    let h = (logical_h * scale).round() as i32;
    let wa_x = work_area.position.x;
    let wa_y = work_area.position.y;
    let wa_w = work_area.size.width as i32;
    let wa_h = work_area.size.height as i32;
    let right = wa_x + wa_w;
    let bottom = wa_y + wa_h;
    let inset = (12.0 * scale).round() as i32;
    let center_x = wa_x + ((wa_w as f64) * anchor.ratio).round() as i32;
    let center_y = wa_y + ((wa_h as f64) * anchor.ratio).round() as i32;
    let clamp_axis = |value: i32, start: i32, length: i32, size: i32| {
        value.clamp(start, (start + length - size).max(start))
    };
    let (x, y) = match anchor.edge.as_str() {
        "left" => (wa_x + inset, clamp_axis(center_y - h / 2, wa_y, wa_h, h)),
        "right" => (
            right - w - inset,
            clamp_axis(center_y - h / 2, wa_y, wa_h, h),
        ),
        "bottom" => (
            clamp_axis(center_x - w / 2, wa_x, wa_w, w),
            bottom - h - inset,
        ),
        _ => (clamp_axis(center_x - w / 2, wa_x, wa_w, w), wa_y + inset),
    };
    set_window_bounds(detail, x, y, w, h, logical_w, logical_h)?;
    Ok(anchor.clone())
}

/// 计算窗口当前离哪条工作区边缘最近，得出吸附锚点（不落盘、不移动窗口）。
pub fn compute_snap_anchor(window: &WebviewWindow) -> Result<HoverbarAnchor, String> {
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or_else(|| "无法获取显示器信息".to_string())?;
    let work_area = monitor.work_area();
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let left = work_area.position.x;
    let top = work_area.position.y;
    let right = left + work_area.size.width as i32;
    let bottom = top + work_area.size.height as i32;
    let x = position.x;
    let y = position.y;
    let width = size.width as i32;
    let height = size.height as i32;
    let candidates = [
        ("top", (y - top).abs()),
        ("right", (right - (x + width)).abs()),
        ("bottom", (bottom - (y + height)).abs()),
        ("left", (x - left).abs()),
    ];
    let edge = candidates
        .into_iter()
        .min_by_key(|(_, distance)| *distance)
        .map(|(edge, _)| edge)
        .unwrap_or("top");
    let ratio = if matches!(edge, "top" | "bottom") {
        ((x + width / 2 - left) as f64 / work_area.size.width as f64).clamp(0.0, 1.0)
    } else {
        ((y + height / 2 - top) as f64 / work_area.size.height as f64).clamp(0.0, 1.0)
    };
    Ok(HoverbarAnchor {
        edge: edge.to_string(),
        ratio,
    })
}

/// 创建悬浮球锚点窗口（40x40 透明置顶，标记脚本区分视图）。
fn create_anchor_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    // 标记脚本：让前端区分悬浮球视图与主窗口视图，与 URL 形式无关、最可靠。
    const MARKER_JS: &str = "window.__HOVERBAR_ANCHOR__ = true;";
    let url = WebviewUrl::App("index.html".parse().expect("valid path"));
    let window = WebviewWindowBuilder::new(app, "hoverbar", url)
        .title("额度悬浮球")
        .decorations(false)
        .transparent(true)
        .background_color(tauri::window::Color(0, 0, 0, 0))
        .shadow(false)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .focused(false)
        .visible(false)
        .inner_size(40.0, 40.0)
        .initialization_script(MARKER_JS)
        .build()?;

    let anchor = storage::load_preferences(app).anchor;
    let _ = apply_anchor_layout(&window, &anchor);
    let _ = window.show();
    Ok(window)
}

/// 创建悬浮详情窗口（独立窗口，初始不可见，由 show_hoverbar_detail 命令布局后显示）。
fn create_detail_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let url = WebviewUrl::App("index.html".parse().expect("valid path"));
    let detail = WebviewWindowBuilder::new(app, "hoverbar-detail", url)
        .title("额度监控详情")
        .decorations(false)
        .transparent(true)
        .background_color(tauri::window::Color(0, 0, 0, 0))
        .shadow(false)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .focused(false)
        .visible(false)
        .inner_size(420.0, 360.0)
        .initialization_script("window.__HOVERBAR_DETAIL__ = true;")
        .build()?;
    let app_for_focus = app.clone();
    detail.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Focused(true)) {
            if let Some(anchor_window) = app_for_focus.get_webview_window("hoverbar") {
                let _ = keep_anchor_above_detail(&anchor_window);
            }
        }
    });
    Ok(detail)
}

/// 按偏好确保悬浮球与详情窗口存在（缺失则创建）。
/// 详情窗必须在启动时建好并挂上监听：悬停展开等不了 WebView 冷启动。
pub fn ensure_hoverbar_windows(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window("hoverbar").is_none() {
        create_anchor_window(app).map_err(|error| error.to_string())?;
    }
    if app.get_webview_window("hoverbar-detail").is_none() {
        create_detail_window(app).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// 展开前确保详情窗存在；正常路径在启动时已创建。
pub fn ensure_hoverbar_detail_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(detail) = app.get_webview_window("hoverbar-detail") {
        return Ok(detail);
    }
    create_detail_window(app).map_err(|error| error.to_string())
}

/// 隐藏悬浮球与详情窗口，并同步前端可见性状态。
pub fn hide_hoverbar_windows(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("hoverbar") {
        let _ = window.hide();
    }
    if let Some(detail) = app.get_webview_window("hoverbar-detail") {
        let _ = detail.hide();
    }
    if let Some(runtime) = app.try_state::<HoverbarRuntime>() {
        runtime.detail_visible.store(false, Ordering::SeqCst);
    }
    let _ = app.emit_to("hoverbar", "hoverbar-detail-visibility", false);
}

/// 设置页切换悬浮球开关：写偏好并立即同步窗口可见性。
/// 窗口已存在但被隐藏时必须重新布局并显示（对应旧版 VisibilityAction::Show 分支）。
pub fn set_hoverbar_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let mut prefs = storage::load_preferences(app);
    prefs.enabled = enabled;
    storage::save_preferences(app, &prefs);
    if enabled {
        match app.get_webview_window("hoverbar") {
            Some(window) => {
                let anchor = prefs.anchor;
                apply_anchor_layout(&window, &anchor)?;
                let _ = window.show();
                if app.get_webview_window("hoverbar-detail").is_none() {
                    create_detail_window(app).map_err(|error| error.to_string())?;
                }
            }
            None => ensure_hoverbar_windows(app)?,
        }
    } else {
        hide_hoverbar_windows(app);
    }
    Ok(())
}

/// 前台窗口是否处于全屏（无边框全屏游戏/视频等）：
/// 排除 Windows 桌面窗口，并要求无标题栏窗口与显示器物理边界基本重合。
/// 不能使用“覆盖工作区 95%”判断，否则桌面、输入面板等 Shell 窗口会被误判为全屏。
#[cfg(windows)]
fn is_foreground_fullscreen() -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetDesktopWindow, GetForegroundWindow, GetShellWindow, GetWindowLongW,
        GetWindowRect, GWL_STYLE, WS_CAPTION,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return false;
        }
        if hwnd == GetShellWindow() || hwnd == GetDesktopWindow() {
            return false;
        }
        let mut class_name = [0u16; 64];
        let class_name_len = GetClassNameW(hwnd, class_name.as_mut_ptr(), class_name.len() as i32);
        if class_name_len > 0 {
            let class_name = String::from_utf16_lossy(&class_name[..class_name_len as usize]);
            if matches!(class_name.as_str(), "Progman" | "WorkerW") {
                return false;
            }
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return false;
        }
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        if style & WS_CAPTION != 0 {
            return false;
        }
        let hmonitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if hmonitor.is_null() {
            return false;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..std::mem::zeroed()
        };
        if GetMonitorInfoW(hmonitor, &mut info) == 0 {
            return false;
        }
        let monitor = &info.rcMonitor;
        const EDGE_TOLERANCE: i32 = 2;
        (rect.left - monitor.left).abs() <= EDGE_TOLERANCE
            && (rect.top - monitor.top).abs() <= EDGE_TOLERANCE
            && (rect.right - monitor.right).abs() <= EDGE_TOLERANCE
            && (rect.bottom - monitor.bottom).abs() <= EDGE_TOLERANCE
    }
}

#[cfg(not(windows))]
fn is_foreground_fullscreen() -> bool {
    false
}

/// 全屏监视线程：每 2 秒轮询一次，状态翻转时才动作。
/// 进入全屏隐藏悬浮球；退出全屏后仅当偏好启用时恢复小球（详情面板不自动恢复）。
pub fn start_fullscreen_watcher(app: AppHandle) {
    thread::spawn(move || {
        let mut last_fullscreen = false;
        loop {
            thread::sleep(Duration::from_millis(2000));
            let fullscreen = is_foreground_fullscreen();
            if fullscreen == last_fullscreen {
                continue;
            }
            last_fullscreen = fullscreen;
            let Some(window) = app.get_webview_window("hoverbar") else {
                continue;
            };
            if fullscreen {
                let _ = window.hide();
                if let Some(detail) = app.get_webview_window("hoverbar-detail") {
                    let _ = detail.hide();
                }
                if let Some(runtime) = app.try_state::<HoverbarRuntime>() {
                    runtime.detail_visible.store(false, Ordering::SeqCst);
                }
                let _ = app.emit_to("hoverbar", "hoverbar-detail-visibility", false);
            } else {
                let enabled = storage::load_preferences(&app).enabled;
                if enabled {
                    let _ = window.show();
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{logical_size, HoverbarWindowState};

    #[test]
    fn every_anchor_uses_the_glass_orb_window_size() {
        assert_eq!(
            logical_size("top", HoverbarWindowState::Anchor, None, None),
            (40.0, 40.0)
        );
        assert_eq!(
            logical_size("bottom", HoverbarWindowState::Anchor, None, None),
            (40.0, 40.0)
        );
    }

    #[test]
    fn detail_size_is_clamped() {
        assert_eq!(
            logical_size("top", HoverbarWindowState::Detail, Some(900.0), Some(900.0)),
            (420.0, 420.0)
        );
        assert_eq!(
            logical_size(
                "right",
                HoverbarWindowState::Detail,
                Some(900.0),
                Some(900.0)
            ),
            (300.0, 480.0)
        );
    }

    #[test]
    fn top_and_bottom_detail_have_the_same_horizontal_bounds() {
        assert_eq!(
            logical_size("bottom", HoverbarWindowState::Detail, None, Some(220.0)),
            (420.0, 220.0)
        );
    }
}
