use crate::commands::require_label;
use crate::radar::{self, RadarControl, RadarHistoryView, RadarSnapshot, TiboPostView};
use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

#[tauri::command]
pub async fn get_radar_snapshot(database: State<'_, Database>, control: State<'_, RadarControl>) -> Result<RadarSnapshot, String> {
    let database = database.inner().clone();
    let mut snapshot = tauri::async_runtime::spawn_blocking(move || radar::snapshot(&database))
        .await
        .map_err(|error| format!("读取雷达快照任务失败: {error}"))??;
    snapshot.check_running = control.is_running();
    Ok(snapshot)
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
    let _ = (range_key, user_prompt);
    radar::run_controlled_check(&app, &database, &coordinator, &control,
        analyze, source_id.as_deref(), model.as_deref()).await
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
    range_key: Option<String>,
    source_id: Option<String>,
    model: Option<String>,
    user_prompt: Option<String>,
    background_check: Option<bool>,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main"])?;
    // range_key 已废弃（浏览/监控拆分）：浏览筛选为前端本地状态，监控窗口固定 72h。
    let _ = range_key;
    radar::save_analysis_prefs(
        &database,
        analyze,
        source_id.as_deref(),
        model.as_deref(),
        user_prompt.as_deref(),
        background_check,
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

/// 用户确认额度已重置：写入事件观察期，不判断官方重置或重置卡；绑定具体事件。
#[tauri::command]
pub fn confirm_radar_user_reset(
    event_id: Option<String>,
    expected_revision: Option<i64>,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    let snapshot = radar::confirm_user_reset(&database, event_id.as_deref(), expected_revision)?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
}

/// 撤销人工确认额度已重置；绑定具体事件。
#[tauri::command]
pub fn undo_radar_user_reset(
    event_id: Option<String>,
    expected_revision: Option<i64>,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    let snapshot = radar::undo_user_reset(&database, event_id.as_deref(), expected_revision)?;
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

#[tauri::command]
pub fn list_radar_history(
    cursor: Option<String>,
    limit: Option<i64>,
    window: WebviewWindow,
    database: State<'_, Database>,
) -> Result<RadarHistoryView, String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    radar::list_history(&database, cursor.as_deref(), limit.unwrap_or(20) as usize)
}

#[tauri::command]
pub fn list_radar_posts(
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    cursor: Option<String>,
    limit: Option<i64>,
    window: WebviewWindow,
    database: State<'_, Database>,
) -> Result<Vec<TiboPostView>, String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    radar::list_posts(
        &database,
        from_ms,
        to_ms,
        cursor.as_deref(),
        limit.unwrap_or(80) as usize,
    )
}

#[tauri::command]
pub fn get_radar_post(
    post_id: String,
    window: WebviewWindow,
    database: State<'_, Database>,
) -> Result<TiboPostView, String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    radar::get_post(&database, &post_id)
}

#[tauri::command]
pub fn list_radar_event_analyses(event_id:String,cursor:Option<String>,window:WebviewWindow,database:State<'_,Database>) -> Result<radar::RadarAnalysisPage,String> {
    require_label(&window,&["main","hoverbar-detail"])?;
    radar::list_event_analyses(&database,&event_id,cursor.as_deref(),20)
}
