use crate::commands::require_label;
use crate::radar::{self, RadarControl, RadarSnapshot};
use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

#[tauri::command]
pub fn get_radar_snapshot(database: State<'_, Database>) -> Result<RadarSnapshot, String> {
    radar::snapshot(&database)
}

#[tauri::command]
pub async fn run_radar_check(
    analyze: bool,
    range_key: Option<String>,
    source_id: Option<String>,
    model: Option<String>,
    user_prompt: Option<String>,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
    coordinator: State<'_, RefreshCoordinator>,
    control: State<'_, RadarControl>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    if !control.try_begin() {
        return Err("已有检查正在进行".into());
    }
    let my_generation = control.generation.load(Ordering::Relaxed);
    // select 在 await 点（CodexRadar 抓取 / 模型请求）打断；被丢弃的检查不落任何记录。
    let _ = app.emit("radar-check-started", ());
    let run = radar::run_check(
        &database,
        &coordinator,
        analyze,
        range_key.as_deref().unwrap_or("3d"),
        source_id.as_deref(),
        model.as_deref(),
        user_prompt.as_deref(),
    );
    let result = tokio::select! {
        snapshot = run => {
            let _ = app.emit("radar-data-changed", ());
            snapshot
        }
        _ = control.wait_cancelled(my_generation) => Err("已终止本次检查".into()),
    };
    control.finish();
    let _ = app.emit("radar-check-finished", ());
    result
}

/// 终止当前进行中的雷达检查：使代号 +1，运行中的检查在下一个 await 点被打断。
#[tauri::command]
pub fn cancel_radar_check(
    control: State<'_, RadarControl>,
    window: WebviewWindow,
) -> Result<(), String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    control.cancel();
    Ok(())
}

/// 翻译单条 Tibo 动态：主窗口与悬浮详情二级页都可调用。
#[tauri::command]
pub async fn translate_radar_post(
    post_id: String,
    source_id: Option<String>,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
    coordinator: State<'_, RefreshCoordinator>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    let snapshot =
        radar::translate_post(&database, &coordinator, &post_id, source_id.as_deref()).await?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
}

#[tauri::command]
pub fn save_radar_analysis_prefs(
    analyze: bool,
    range_key: String,
    source_id: Option<String>,
    model: Option<String>,
    user_prompt: Option<String>,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main"])?;
    radar::save_analysis_prefs(
        &database,
        analyze,
        &range_key,
        source_id.as_deref(),
        model.as_deref(),
        user_prompt.as_deref(),
    )?;
    let snapshot = radar::snapshot(&database)?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
}

/// 验证连接：向所选来源的对话端点发一次极小请求，验证自定义模型名真实可用。
#[tauri::command]
pub async fn test_radar_model(
    source_id: String,
    model: String,
    window: WebviewWindow,
    database: State<'_, Database>,
    coordinator: State<'_, RefreshCoordinator>,
) -> Result<(), String> {
    require_label(&window, &["main"])?;
    radar::test_chat_model(&database, &coordinator, &source_id, &model).await
}

/// 保存验证过的自定义模型，加入分析模型下拉。
#[tauri::command]
pub fn add_radar_custom_model(
    source_id: String,
    model: String,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main"])?;
    let snapshot = radar::add_custom_model(&database, &source_id, &model)?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
}

/// 删除自定义模型，从分析模型下拉移除。
#[tauri::command]
pub fn delete_radar_custom_model(
    source_id: String,
    model: String,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main"])?;
    let snapshot = radar::delete_custom_model(&database, &source_id, &model)?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
}

/// 用户在主窗口手动确认重置卡：只更新指定观察记录的归因 user_confirmed，
/// 不篡改快照、不推进事件。
#[tauri::command]
pub fn confirm_radar_quota_change(
    observation_id: i64,
    confirmed_at: i64,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main"])?;
    let snapshot = radar::confirm_quota_change(&database, observation_id, confirmed_at)?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
}

/// 用户确认额度已重置：写入事件观察期，不判断官方重置或重置卡。
#[tauri::command]
pub fn confirm_radar_user_reset(
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    let snapshot = radar::confirm_user_reset(&database)?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
}

/// 撤销人工确认额度已重置。
#[tauri::command]
pub fn undo_radar_user_reset(
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    let snapshot = radar::undo_user_reset(&database)?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
}

/// 未保存的独立对话接入：用填写的地址和 API Key 发一次极小 ping。
#[tauri::command]
pub async fn test_radar_chat_endpoint(
    api_base_url: String,
    secret: String,
    model: String,
    window: WebviewWindow,
    coordinator: State<'_, RefreshCoordinator>,
) -> Result<(), String> {
    require_label(&window, &["main"])?;
    radar::test_chat_endpoint(&coordinator, &api_base_url, &secret, &model).await
}

/// 测试通过后保存独立对话接入。
#[tauri::command]
pub async fn save_radar_chat_endpoint(
    display_name: String,
    api_base_url: String,
    secret: String,
    model: String,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
    coordinator: State<'_, RefreshCoordinator>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main"])?;
    let snapshot = radar::save_chat_endpoint(
        &database,
        &coordinator,
        &display_name,
        &api_base_url,
        &secret,
        &model,
    )
    .await?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
}

#[tauri::command]
pub fn delete_radar_chat_endpoint(
    endpoint_id: String,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main"])?;
    let snapshot = radar::delete_chat_endpoint(&database, &endpoint_id)?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
}

/// 隐藏或显示 CodexRadar 公告：主窗口与悬浮详情共用，不影响来源同步。
#[tauri::command]
pub fn set_radar_notice_hidden(
    hidden: bool,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    let snapshot = radar::set_notice_hidden(&database, hidden)?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
}
