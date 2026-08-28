//! 平台数据、刷新与凭据 Tauri Commands。前端只接收脱敏 ViewModel。

use crate::domain::PlatformSummaryViewModel;
use crate::providers;
use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
use crate::storage::legacy_import::{self, LegacyConfigInspection, LegacyImportResult};
use crate::storage::vault;
use tauri::State;

#[tauri::command]
pub fn get_platform_summaries(
    database: State<'_, Database>,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    providers::platform_summaries(&database)
}

#[tauri::command]
pub async fn refresh_platform(
    provider_id: String,
    database: State<'_, Database>,
    coordinator: State<'_, RefreshCoordinator>,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    coordinator.refresh_platform(&database, &provider_id).await?;
    providers::platform_summaries(&database)
}

#[tauri::command]
pub async fn save_source_credential(
    source_id: String,
    secret: String,
    database: State<'_, Database>,
    coordinator: State<'_, RefreshCoordinator>,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    let source = database.source(&source_id)?;
    if source.platform_id != "deepseek" {
        return Err("此来源不接受手动凭据".into());
    }
    let output = coordinator.validate_secret(&source, secret.trim()).await?;
    let reference = vault::secret_ref(&source.account_id, &source.id);
    let previous_secret = vault::get(&reference)?;
    vault::set(&reference, secret.trim())?;
    if let Err(error) = database.save_secret_ref(&source.id, &reference) {
        restore_vault(&reference, previous_secret.as_deref());
        return Err(error);
    }
    if let Err(error) = coordinator.persist_validated(&database, &source, &output) {
        restore_vault(&reference, previous_secret.as_deref());
        if source.secret_ref.is_none() {
            let _ = database.clear_secret_ref(&source.id);
        }
        return Err(error);
    }
    providers::platform_summaries(&database)
}

#[tauri::command]
pub fn clear_source_credential(
    source_id: String,
    database: State<'_, Database>,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    let source = database.source(&source_id)?;
    let reference = source
        .secret_ref
        .clone()
        .unwrap_or_else(|| vault::secret_ref(&source.account_id, &source.id));
    let previous_secret = vault::get(&reference)?;
    vault::delete(&reference)?;
    if let Err(error) = database.clear_secret_ref(&source.id) {
        if let Some(secret) = previous_secret.as_deref() {
            let _ = vault::set(&reference, secret);
        }
        return Err(error);
    }
    providers::platform_summaries(&database)
}

#[tauri::command]
pub async fn start_source_login(
    source_id: String,
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("仅主窗口可以打开来源登录".into());
    }
    if source_id != crate::providers::deepseek::WEB_SOURCE_ID {
        return Err("此来源不支持网页登录".into());
    }
    crate::windows::source_login::open(&app).await
}

#[tauri::command]
pub async fn close_source_login(
    source_id: String,
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("仅主窗口可以关闭来源登录".into());
    }
    if source_id != crate::providers::deepseek::WEB_SOURCE_ID {
        return Err("此来源没有网页登录页".into());
    }
    crate::windows::source_login::close(&app)
}

#[tauri::command]
pub fn inspect_legacy_config(
    database: State<'_, Database>,
) -> Result<LegacyConfigInspection, String> {
    legacy_import::inspect(&database)
}

#[tauri::command]
pub async fn import_legacy_config(
    archive_old_file: bool,
    database: State<'_, Database>,
    coordinator: State<'_, RefreshCoordinator>,
) -> Result<LegacyImportResult, String> {
    let legacy = legacy_import::read()?;
    let mut validated = Vec::new();
    if let Some(secret) = legacy.api_key.as_deref() {
        let source = database.source(crate::providers::deepseek::BALANCE_SOURCE_ID)?;
        let output = coordinator.validate_secret(&source, secret).await?;
        validated.push((source, secret.to_string(), output));
    }
    if let Some(secret) = legacy.usage_token.as_deref() {
        let source = database.source(crate::providers::deepseek::WEB_SOURCE_ID)?;
        let output = coordinator.validate_secret(&source, secret).await?;
        validated.push((source, secret.to_string(), output));
    }
    if validated.is_empty() {
        return Err("旧配置中没有可导入的 DeepSeek 凭据".into());
    }

    let mut imported_source_ids = Vec::new();
    for (source, secret, output) in validated {
        let reference = vault::secret_ref(&source.account_id, &source.id);
        vault::set(&reference, &secret)?;
        database.save_secret_ref(&source.id, &reference)?;
        coordinator.persist_validated(&database, &source, &output)?;
        imported_source_ids.push(source.id);
    }
    legacy_import::mark_completed(&database)?;
    let archived_path = if archive_old_file {
        Some(legacy_import::archive(&legacy.path)?.display().to_string())
    } else {
        None
    };
    Ok(LegacyImportResult {
        platforms: providers::platform_summaries(&database)?,
        imported_source_ids,
        archived_path,
    })
}

fn restore_vault(reference: &str, previous: Option<&str>) {
    if let Some(previous) = previous {
        let _ = vault::set(reference, previous);
    } else {
        let _ = vault::delete(reference);
    }
}
