use crate::commands::require_label;
use crate::radar::{self, RadarSnapshot};
use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
use tauri::{State, WebviewWindow};

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
    window: WebviewWindow,
    database: State<'_, Database>,
    coordinator: State<'_, RefreshCoordinator>,
) -> Result<RadarSnapshot, String> {
    require_label(&window, &["main"])?;
    radar::run_check(
        &database,
        &coordinator,
        analyze,
        range_key.as_deref().unwrap_or("3d"),
        source_id.as_deref(),
        model.as_deref(),
    )
    .await
}
