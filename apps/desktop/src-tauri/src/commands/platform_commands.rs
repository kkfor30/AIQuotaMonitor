//! 平台数据、刷新与凭据 Tauri Commands。前端只接收脱敏 ViewModel。

use crate::commands::require_label;
use crate::domain::refresh::SourceRefreshOutput;
use crate::domain::PlatformSummaryViewModel;
use crate::providers;
use crate::providers::catalog::{self, PlatformCatalogItem, PlatformSetupViewModel};
use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
use crate::storage::legacy_import::{self, LegacyConfigInspection, LegacyImportResult};
use crate::storage::repository::SourceRecord;
use crate::storage::vault;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State, WebviewWindow};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSetupInput {
    pub platform_id: String,
    pub display_name: String,
    pub notes: String,
    pub api_base_url: String,
    pub source_id: Option<String>,
    pub secret: String,
}

#[tauri::command]
pub fn get_platform_summaries(
    database: State<'_, Database>,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    providers::platform_summaries(&database)
}

#[tauri::command]
pub fn list_platform_catalog(
    database: State<'_, Database>,
) -> Result<Vec<PlatformCatalogItem>, String> {
    providers::catalog_items(&database)
}

#[tauri::command]
pub fn get_platform_setup(
    platform_id: String,
    database: State<'_, Database>,
) -> Result<PlatformSetupViewModel, String> {
    providers::setup_view(&database, &platform_id)
}

#[tauri::command]
pub fn add_user_platforms(
    platform_ids: Vec<String>,
    database: State<'_, Database>,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    providers::add_platforms(&database, &platform_ids)
}

#[tauri::command]
pub fn remove_user_platform(
    platform_id: String,
    window: WebviewWindow,
    database: State<'_, Database>,
    app: tauri::AppHandle,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    require_label(&window, &["main"])?;
    if catalog::entry(&platform_id).is_none() {
        return Err("该平台不在可添加注册表中".into());
    }
    let sources = database.list_sources(&platform_id)?;
    for source in &sources {
        if crate::providers::codex::is_extra_source(&source.id) {
            if let Some(home) = extra_codex_home(&database, &source.id) {
                let _ = crate::providers::codex::logout_cli_at(Some(&home));
                let _ = std::fs::remove_dir_all(home);
            }
        }
    }
    database.remove_user_platform(&platform_id)?;
    for source in &sources {
        let reference = source
            .secret_ref
            .clone()
            .unwrap_or_else(|| vault::secret_ref(&source.account_id, &source.id));
        let _ = vault::delete(&reference);
        if crate::windows::source_login::is_web_login_source(&source.id) {
            crate::windows::source_login::clear_session(&app, &source.id)?;
        }
    }
    providers::platform_summaries(&database)
}

#[tauri::command]
pub fn reveal_source_secret(
    source_id: String,
    window: WebviewWindow,
    database: State<'_, Database>,
) -> Result<String, String> {
    require_label(&window, &["main"])?;
    let source = database.source(&source_id)?;
    if source.source_type == "local_cli" || crate::providers::codex::is_codex_source(&source.id) {
        return Err("此来源没有可查看的密钥".into());
    }
    let reference = source
        .secret_ref
        .clone()
        .unwrap_or_else(|| vault::secret_ref(&source.account_id, &source.id));
    vault::get(&reference)?.ok_or_else(|| "尚未保存凭据".to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointLatencyView {
    pub url: String,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn test_api_endpoints(
    urls: Vec<String>,
    window: WebviewWindow,
) -> Result<Vec<EndpointLatencyView>, String> {
    require_label(&window, &["main"])?;
    if urls.len() > 12 {
        return Err("一次最多测速 12 个地址".into());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|error| format!("创建测速客户端失败：{error}"))?;
    let mut results = Vec::with_capacity(urls.len());
    for url in urls {
        results.push(measure_endpoint(&client, url).await);
    }
    Ok(results)
}

async fn measure_endpoint(client: &reqwest::Client, raw: String) -> EndpointLatencyView {
    let url = match catalog::normalize_api_base_url(&raw) {
        Ok(url) => url,
        Err(error) => {
            return EndpointLatencyView {
                url: raw,
                latency_ms: None,
                error: Some(error),
            };
        }
    };
    let _ = client.get(&url).send().await;
    let started = Instant::now();
    match client.get(&url).send().await {
        Ok(_) => EndpointLatencyView {
            url,
            latency_ms: Some(started.elapsed().as_millis() as u64),
            error: None,
        },
        Err(error) => EndpointLatencyView {
            url,
            latency_ms: None,
            error: Some(if error.is_timeout() {
                "请求超时".into()
            } else if error.is_connect() {
                "连接失败".into()
            } else {
                error.to_string()
            }),
        },
    }
}

#[tauri::command]
pub async fn refresh_platform(
    provider_id: String,
    app: AppHandle,
    database: State<'_, Database>,
    coordinator: State<'_, RefreshCoordinator>,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    coordinator.refresh_platform(&database, &provider_id).await?;
    let _ = app.emit("platform-data-changed", ());
    providers::platform_summaries(&database)
}

#[tauri::command]
pub async fn validate_source_credential(
    source_id: String,
    secret: String,
    api_base_url: Option<String>,
    database: State<'_, Database>,
    coordinator: State<'_, RefreshCoordinator>,
) -> Result<String, String> {
    let source = database.source(&source_id)?;
    if source.source_type != "api_key" && source.source_type != "web_session" {
        return Err("此来源不接受手动凭据".into());
    }
    let api_base_url = match api_base_url.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => Some(catalog::normalize_api_base_url(value)?),
        None => None,
    };
    let output = coordinator
        .validate_secret_at(&source, secret.trim(), api_base_url.as_deref())
        .await?;
    let summary = output
        .capabilities
        .iter()
        .find_map(|capability| capability.primary_value.clone())
        .unwrap_or_else(|| "已通过官方接口验证".into());
    Ok(format!("连接成功：{summary}"))
}

#[tauri::command]
pub async fn save_platform_setup(
    input: PlatformSetupInput,
    database: State<'_, Database>,
    coordinator: State<'_, RefreshCoordinator>,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    let display_name = input.display_name.trim();
    if display_name.is_empty() {
        return Err("请填写供应商名称".into());
    }
    let entry = catalog::entry(&input.platform_id).ok_or_else(|| "该平台不在可添加注册表中".to_string())?;
    let api_base_url = if entry.needs_api_key {
        Some(catalog::normalize_api_base_url(&input.api_base_url)?)
    } else {
        None
    };
    if entry.needs_api_key {
        let source = resolve_api_key_source(&database, &input)?;
        let stored_url = database
            .user_platform(&input.platform_id)?
            .and_then(|platform| platform.api_base_url);
        let new_secret = input.secret.trim();
        let secret_changed = !new_secret.is_empty();
        let url_changed = stored_url.as_deref() != api_base_url.as_deref();
        let already_configured = source.secret_ref.as_deref().and_then(|reference| vault::get(reference).ok().flatten()).is_some();
        if !already_configured && !secret_changed {
            return Err("请填写 API Key".into());
        }
        if secret_changed || url_changed || !already_configured {
            let secret = if secret_changed {
                new_secret.to_string()
            } else {
                let reference = source
                    .secret_ref
                    .clone()
                    .unwrap_or_else(|| vault::secret_ref(&source.account_id, &source.id));
                vault::get(&reference)?.ok_or_else(|| "请填写 API Key".to_string())?
            };
            let output = coordinator
                .validate_secret_at(&source, &secret, api_base_url.as_deref())
                .await?;
            if secret_changed {
                persist_secret(&database, &coordinator, &source, &secret, &output)?;
            } else if let Err(error) = coordinator.persist_validated(&database, &source, &output) {
                return Err(error);
            }
        }
    }
    database.save_user_platform_setup(
        &input.platform_id,
        display_name,
        input.notes.trim(),
        api_base_url.as_deref(),
    )?;
    providers::platform_summaries(&database)
}

#[tauri::command]
pub async fn save_source_credential(
    source_id: String,
    secret: String,
    api_base_url: Option<String>,
    database: State<'_, Database>,
    coordinator: State<'_, RefreshCoordinator>,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    let source = database.source(&source_id)?;
    if source.source_type != "api_key" && source.source_type != "web_session" {
        return Err("此来源不接受手动凭据".into());
    }
    let api_base_url = match api_base_url.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => Some(catalog::normalize_api_base_url(value)?),
        None => None,
    };
    let output = coordinator
        .validate_secret_at(&source, secret.trim(), api_base_url.as_deref())
        .await?;
    persist_secret(&database, &coordinator, &source, secret.trim(), &output)?;
    if source.source_type == "api_key"
        && source.id != crate::providers::kimi::BALANCE_SOURCE_ID
    {
        if let Some(api_base_url) = api_base_url.as_deref() {
            database.save_user_platform_api_base(&source.platform_id, Some(api_base_url))?;
        }
    }
    providers::platform_summaries(&database)
}

#[tauri::command]
pub async fn clear_source_credential(
    source_id: String,
    database: State<'_, Database>,
    app: tauri::AppHandle,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    let source = database.source(&source_id)?;
    let reference = source
        .secret_ref
        .clone()
        .unwrap_or_else(|| vault::secret_ref(&source.account_id, &source.id));
    if source.id == crate::providers::codex::SOURCE_ID {
        crate::providers::codex::logout_cli()?;
        database.clear_secret_ref(&source.id)?;
        return providers::platform_summaries(&database);
    }
    if crate::providers::codex::is_extra_source(&source.id) {
        if let Some(home) = extra_codex_home(&database, &source.id) {
            crate::providers::codex::logout_cli_at(Some(&home))?;
        }
        database.clear_secret_ref(&source.id)?;
        return providers::platform_summaries(&database);
    }
    let previous_secret = vault::get(&reference)?;
    vault::delete(&reference)?;
    if let Err(error) = database.clear_secret_ref(&source.id) {
        if let Some(secret) = previous_secret.as_deref() {
            let _ = vault::set(&reference, secret);
        }
        return Err(error);
    }
    if crate::windows::source_login::is_web_login_source(&source.id) {
        crate::windows::source_login::clear_session(&app, &source.id)?;
    }
    providers::platform_summaries(&database)
}

#[tauri::command]
pub async fn start_source_login(
    source_id: String,
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    database: State<'_, Database>,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("仅主窗口可以打开来源登录".into());
    }
    match source_id.as_str() {
        id if crate::windows::source_login::is_web_login_source(id) => {
            crate::windows::source_login::open(&app, id).await
        }
        id if id == crate::providers::codex::SOURCE_ID => crate::providers::codex::login_cli().await,
        id if crate::providers::codex::is_extra_source(id) => {
            let home = extra_codex_home(&database, id).ok_or_else(|| "无法定位额外账号目录".to_string())?;
            crate::providers::codex::login_cli_at(Some(&home)).await
        }
        _ => Err("此来源不支持登录".into()),
    }
}

#[tauri::command]
pub fn add_codex_account(
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    if database.user_platform("openai")?.is_none() {
        return Err("请先添加 GPT / Codex 平台".into());
    }
    let extras = database
        .list_sources("openai")?
        .into_iter()
        .filter(|source| crate::providers::codex::is_extra_source(&source.id))
        .count();
    let id = format!(
        "{}{}-{}",
        crate::providers::codex::EXTRA_SOURCE_PREFIX,
        extras + 1,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let label = format!("额外 ChatGPT 账号 {}", extras + 1);
    database.ensure_account_source(&id, "openai", &id, "oauth", &label, &label)?;
    let _ = app.emit("platform-data-changed", ());
    providers::platform_summaries(&database)
}

#[tauri::command]
pub fn rename_codex_account(
    source_id: String,
    display_name: String,
    window: WebviewWindow,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    require_label(&window, &["main"])?;
    if !crate::providers::codex::is_extra_source(&source_id) {
        return Err("只能重命名额外 ChatGPT 账号".into());
    }
    let name = normalize_extra_account_name(&display_name)?;
    database.source(&source_id)?;
    database.rename_source(&source_id, &name)?;
    let _ = app.emit("platform-data-changed", ());
    providers::platform_summaries(&database)
}

#[tauri::command]
pub fn remove_codex_account(
    source_id: String,
    database: State<'_, Database>,
    app: AppHandle,
) -> Result<Vec<PlatformSummaryViewModel>, String> {
    if !crate::providers::codex::is_extra_source(&source_id) {
        return Err("只能移除额外 ChatGPT 账号".into());
    }
    let source = database.source(&source_id)?;
    if let Some(home) = extra_codex_home(&database, &source_id) {
        let _ = crate::providers::codex::logout_cli_at(Some(&home));
        let _ = std::fs::remove_dir_all(home);
    }
    database.delete_account(&source.account_id)?;
    let _ = app.emit("platform-data-changed", ());
    providers::platform_summaries(&database)
}

fn normalize_extra_account_name(display_name: &str) -> Result<String, String> {
    let name = display_name.trim();
    if name.is_empty() {
        return Err("账号名称不能为空".into());
    }
    if name.chars().count() > 40 {
        return Err("账号名称最多 40 个字".into());
    }
    if name.chars().any(char::is_control) {
        return Err("账号名称包含无效字符".into());
    }
    Ok(name.to_string())
}

fn extra_codex_home(database: &Database, source_id: &str) -> Option<std::path::PathBuf> {
    let data_dir = database.path().parent()?;
    crate::providers::codex::is_extra_source(source_id)
        .then(|| crate::providers::codex::extra_source_home(data_dir, source_id))
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
    if !crate::windows::source_login::is_web_login_source(&source_id) {
        return Err("此来源没有网页登录页".into());
    }
    crate::windows::source_login::close(&app, &source_id)
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

fn resolve_api_key_source(database: &Database, input: &PlatformSetupInput) -> Result<SourceRecord, String> {
    let source = if let Some(source_id) = input.source_id.as_deref().filter(|value| !value.is_empty()) {
        database.source(source_id)?
    } else {
        database
            .list_sources(&input.platform_id)?
            .into_iter()
            .find(|source| source.source_type == "api_key")
            .ok_or_else(|| "该平台没有 API Key 来源".to_string())?
    };
    if source.platform_id != input.platform_id {
        return Err("来源与平台不匹配".into());
    }
    if source.source_type != "api_key" {
        return Err("此来源不接受手动凭据".into());
    }
    Ok(source)
}

fn persist_secret(
    database: &Database,
    coordinator: &RefreshCoordinator,
    source: &SourceRecord,
    secret: &str,
    output: &SourceRefreshOutput,
) -> Result<(), String> {
    let reference = vault::secret_ref(&source.account_id, &source.id);
    let previous_secret = vault::get(&reference)?;
    vault::set(&reference, secret)?;
    if let Err(error) = database.save_secret_ref(&source.id, &reference) {
        restore_vault(&reference, previous_secret.as_deref());
        return Err(error);
    }
    if let Err(error) = coordinator.persist_validated(database, source, output) {
        restore_vault(&reference, previous_secret.as_deref());
        if source.secret_ref.is_none() {
            let _ = database.clear_secret_ref(&source.id);
        }
        return Err(error);
    }
    Ok(())
}
