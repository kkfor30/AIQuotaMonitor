use crate::commands::require_label;
use crate::radar::{self, RadarSnapshot};
use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
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
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main", "hoverbar-detail"])?;
    let snapshot = radar::run_check(
        &database,
        &coordinator,
        analyze,
        range_key.as_deref().unwrap_or("3d"),
        source_id.as_deref(),
        model.as_deref(),
        user_prompt.as_deref(),
    )
    .await?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
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
    let snapshot = radar::translate_post(&database, &coordinator, &post_id, source_id.as_deref()).await?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
}

#[tauri::command]
pub fn save_radar_analysis_prefs(
    analyze: bool,
    range_key: String,
    source_id: Option<String>,
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
        user_prompt.as_deref(),
    )?;
    let snapshot = radar::snapshot(&database)?;
    let _ = app.emit("radar-data-changed", ());
    Ok(snapshot)
}
