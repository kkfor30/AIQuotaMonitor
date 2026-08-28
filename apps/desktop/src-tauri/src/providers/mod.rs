//! 平台模板注册、真实 ViewModel 聚合与 Source adapter 路由。

pub mod catalog;
pub mod codex;
pub mod deepseek;

use crate::domain::{
    CapabilityDisplayValue, CapabilitySnapshotViewModel, CredentialInputViewModel, DataFreshness,
    PlatformAggregateStatus, PlatformSummaryViewModel, RefreshHistoryEntryViewModel, SourceState,
    SourceSummaryViewModel, SourceType, TrendPoint,
};
use crate::storage::database::Database;
use crate::storage::repository::SourceRecord;
use crate::storage::vault;

struct CapabilityTemplate {
    id: &'static str,
    source_id: &'static str,
    display_name: &'static str,
    kind: &'static str,
}

const DEEPSEEK_CAPABILITIES: &[CapabilityTemplate] = &[
    CapabilityTemplate { id: "balance", source_id: deepseek::BALANCE_SOURCE_ID, display_name: "账户余额", kind: "money" },
    CapabilityTemplate { id: "today_spend", source_id: deepseek::WEB_SOURCE_ID, display_name: "今日消费", kind: "money" },
    CapabilityTemplate { id: "month_spend", source_id: deepseek::WEB_SOURCE_ID, display_name: "本月消费", kind: "money" },
    CapabilityTemplate { id: "model_usage_v4_flash", source_id: deepseek::WEB_SOURCE_ID, display_name: "V4 Flash 用量", kind: "tokens" },
    CapabilityTemplate { id: "model_usage_v4_pro", source_id: deepseek::WEB_SOURCE_ID, display_name: "V4 Pro 用量", kind: "tokens" },
    CapabilityTemplate { id: "request_count", source_id: deepseek::WEB_SOURCE_ID, display_name: "请求数", kind: "tokens" },
    CapabilityTemplate { id: "prompt_tokens", source_id: deepseek::WEB_SOURCE_ID, display_name: "输入 Token", kind: "tokens" },
    CapabilityTemplate { id: "cache_hit_tokens", source_id: deepseek::WEB_SOURCE_ID, display_name: "输入（命中缓存）", kind: "tokens" },
    CapabilityTemplate { id: "cache_miss_tokens", source_id: deepseek::WEB_SOURCE_ID, display_name: "输入（未命中缓存）", kind: "tokens" },
    CapabilityTemplate { id: "response_tokens", source_id: deepseek::WEB_SOURCE_ID, display_name: "输出 Token", kind: "tokens" },
    CapabilityTemplate { id: "cache_hit_rate", source_id: deepseek::WEB_SOURCE_ID, display_name: "缓存命中率", kind: "percent" },
    CapabilityTemplate { id: "usage_trend", source_id: deepseek::WEB_SOURCE_ID, display_name: "近 7 日消费趋势", kind: "trend" },
];

const CODEX_CAPABILITIES: &[CapabilityTemplate] = &[
    CapabilityTemplate { id: "quota_window_5h", source_id: codex::SOURCE_ID, display_name: "5 小时窗口", kind: "percent" },
    CapabilityTemplate { id: "quota_window_7d", source_id: codex::SOURCE_ID, display_name: "7 天窗口", kind: "percent" },
    CapabilityTemplate { id: "credits", source_id: codex::SOURCE_ID, display_name: "Credits 余额", kind: "credits" },
    CapabilityTemplate { id: "plan_level", source_id: codex::SOURCE_ID, display_name: "订阅计划", kind: "text" },
];

pub fn platform_summaries(database: &Database) -> Result<Vec<PlatformSummaryViewModel>, String> {
    let mut platforms = Vec::new();
    for added in database.list_user_platforms()? {
        let entry = catalog::entry(&added.platform_id);
        let official_url = entry.map(|item| item.official_url).unwrap_or("");
        let display_name = if added.display_name.trim().is_empty() {
            entry.map(|item| item.display_name).unwrap_or(added.platform_id.as_str())
        } else {
            added.display_name.as_str()
        };
        match added.platform_id.as_str() {
            "deepseek" => platforms.push(real_platform(
                database,
                "deepseek",
                display_name,
                official_url,
                added.api_base_url.as_deref(),
                DEEPSEEK_CAPABILITIES,
            )?),
            "openai" => platforms.push(real_platform(
                database,
                "openai",
                display_name,
                official_url,
                added.api_base_url.as_deref(),
                CODEX_CAPABILITIES,
            )?),
            id => platforms.push(real_platform(
                database,
                id,
                display_name,
                official_url,
                added.api_base_url.as_deref(),
                &[],
            )?),
        }
    }
    Ok(platforms)
}

pub fn catalog_items(database: &Database) -> Result<Vec<catalog::PlatformCatalogItem>, String> {
    let added = database.list_user_platforms()?;
    Ok(catalog::CATALOG
        .iter()
        .map(|entry| {
            let mut item = catalog::PlatformCatalogItem::from(entry);
            item.added = added.iter().any(|platform| platform.platform_id == entry.id);
            item
        })
        .collect())
}

pub fn setup_view(database: &Database, platform_id: &str) -> Result<catalog::PlatformSetupViewModel, String> {
    let entry = catalog::entry(platform_id).ok_or_else(|| "该平台不在可添加注册表中".to_string())?;
    let added = database.user_platform(platform_id)?;
    let sources = database.list_sources(platform_id)?;
    let api_key_source = sources.iter().find(|source| source.source_type == "api_key");
    Ok(catalog::PlatformSetupViewModel {
        platform_id: entry.id.into(),
        display_name: added
            .as_ref()
            .map(|item| item.display_name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| entry.display_name.into()),
        notes: added.as_ref().map(|item| item.notes.clone()).unwrap_or_default(),
        official_url: entry.official_url.into(),
        api_key_url: entry.api_key_url.map(str::to_string),
        api_base_url: added
            .as_ref()
            .and_then(|item| item.api_base_url.clone())
            .or_else(|| entry.api_base_url.map(str::to_string))
            .unwrap_or_default(),
        official_api_base_url: entry.api_base_url.map(str::to_string).unwrap_or_default(),
        api_endpoint_hint: entry.api_endpoint_hint.into(),
        api_key_source_id: api_key_source.map(|source| source.id.clone()),
        api_key_configured: api_key_source.is_some_and(source_configured),
        needs_api_key: entry.needs_api_key,
        needs_web_login: entry.needs_web_login,
        needs_local_cli: entry.needs_local_cli,
    })
}

pub fn add_platforms(database: &Database, platform_ids: &[String]) -> Result<Vec<PlatformSummaryViewModel>, String> {
    for platform_id in platform_ids {
        let entry = catalog::entry(platform_id).ok_or_else(|| format!("不支持添加平台：{platform_id}"))?;
        match platform_id.as_str() {
            "deepseek" => {}
            "openai" => {}
            "kimi" => database.ensure_account_source("kimi-default", "kimi", "kimi-coding-plan", "api_key", "Coding Plan", "默认账户")?,
            "glm" => database.ensure_account_source("glm-default", "glm", "glm-coding-plan", "api_key", "Coding Plan", "默认账户")?,
            "glm_intl" => database.ensure_account_source("glm-intl-default", "glm_intl", "glm-intl-coding-plan", "api_key", "Coding Plan", "默认账户")?,
            "minimax" => database.ensure_account_source("minimax-default", "minimax", "minimax-coding-plan", "api_key", "Coding Plan", "默认账户")?,
            "minimax_intl" => database.ensure_account_source("minimax-intl-default", "minimax_intl", "minimax-intl-coding-plan", "api_key", "Coding Plan", "默认账户")?,
            "claude_code" => database.ensure_account_source("claude-code-default", "claude_code", "claude-code-local", "local_cli", "本地 Claude 订阅", "本地账户")?,
            "mimo" => database.ensure_account_source("mimo-default", "mimo", "mimo-web-session", "web_session", "网页会话", "默认账户")?,
            _ => return Err(format!("不支持添加平台：{platform_id}")),
        }
        database.add_user_platform(entry.id, entry.display_name, entry.api_base_url)?;
    }
    platform_summaries(database)
}

fn real_platform(
    database: &Database,
    provider_id: &str,
    display_name: &str,
    official_url: &str,
    api_base_url: Option<&str>,
    templates: &[CapabilityTemplate],
) -> Result<PlatformSummaryViewModel, String> {
    let records = database.list_sources(provider_id)?;
    let mut sources = Vec::with_capacity(records.len());
    for source in &records {
        let credential_configured = source_configured(source);
        let mut state = source_state(&source.state);
        if !credential_configured {
            state = SourceState::AuthRequired;
        } else if provider_id == "openai" && matches!(state, SourceState::AuthRequired) {
            state = SourceState::Ready;
        }
        sources.push(SourceSummaryViewModel {
            source_id: source.id.clone(),
            source_type: source_type(&source.source_type),
            display_name: source.display_name.clone(),
            state,
            credential_configured,
            last_validated_at: millis(source.last_validated_at),
            last_success_at: millis(source.last_success_at),
            error_code: source.error_code.clone(),
            error_message: source.error_message.clone(),
            capability_ids: templates
                .iter()
                .filter(|value| value.source_id == source.id)
                .map(|value| value.id.to_string())
                .collect(),
            credential_input: credential_input(&source.id, &source.source_type),
            supports_interactive_login: source.id == deepseek::WEB_SOURCE_ID,
        });
    }

    let mut capabilities = Vec::with_capacity(templates.len());
    for template in templates {
        let source = records.iter().find(|source| source.id == template.source_id);
        let snapshot = database.latest_snapshot(template.source_id, template.id)?;
        capabilities.push(match (source, snapshot) {
            (Some(source), Some(snapshot))
                if template.id != "credits"
                    || (source.state == "ready" && source.last_validated_at == Some(snapshot.captured_at)) =>
            {
                let fresh = source.state == "ready" || source.last_validated_at == Some(snapshot.captured_at);
                CapabilitySnapshotViewModel {
                    capability_id: snapshot.capability_id,
                    source_id: source.id.clone(),
                    display_name: snapshot.display_name,
                    freshness: if fresh { DataFreshness::Fresh } else { DataFreshness::Stale },
                    captured_at: millis(Some(snapshot.captured_at)),
                    last_good_at: if fresh { None } else { millis(Some(snapshot.captured_at)) },
                    value: CapabilityDisplayValue {
                        kind: snapshot.value_kind,
                        primary: snapshot.primary_value,
                        secondary: snapshot.secondary_value,
                        progress: snapshot.progress,
                    },
                    trend: snapshot
                        .trend
                        .into_iter()
                        .filter_map(|point| point.value.parse::<f64>().ok().map(|value| TrendPoint { label: point.label, value }))
                        .collect(),
                }
            }
            _ => CapabilitySnapshotViewModel {
                capability_id: template.id.into(),
                source_id: template.source_id.into(),
                display_name: template.display_name.into(),
                freshness: DataFreshness::Missing,
                captured_at: None,
                last_good_at: None,
                value: CapabilityDisplayValue {
                    kind: template.kind.into(),
                    primary: None,
                    secondary: None,
                    progress: None,
                },
                trend: vec![],
            },
        });
    }
    let aggregate_status = aggregate_status(&sources, &capabilities);
    let configured_count = sources.iter().filter(|source| source.credential_configured).count();
    let access_summary = match provider_id {
        "deepseek" if configured_count == 2 => "API Key + 网页会话".to_string(),
        "deepseek" if sources.iter().any(|source| source.source_id == deepseek::BALANCE_SOURCE_ID && source.credential_configured) => "API Key".to_string(),
        "deepseek" if configured_count > 0 => "网页会话".to_string(),
        "openai" if configured_count > 0 => "本地 Codex OAuth".to_string(),
        _ => "尚未接入".to_string(),
    };
    let refresh_history = database
        .refresh_history(provider_id, 8)?
        .into_iter()
        .map(|entry| RefreshHistoryEntryViewModel {
            id: entry.id,
            source_id: entry.source_id,
            source_name: entry.source_name,
            status: entry.status,
            finished_at: millis(entry.finished_at),
            error_message: entry.error_message,
        })
        .collect();

    Ok(PlatformSummaryViewModel {
        provider_id: provider_id.into(),
        display_name: display_name.into(),
        aggregate_status,
        official_url: Some(official_url.into()),
        api_base_url: api_base_url.map(str::to_string),
        access_summary,
        sources,
        capabilities,
        refresh_history,
    })
}

fn source_configured(source: &SourceRecord) -> bool {
    if source.id == codex::SOURCE_ID {
        codex::local_auth_available()
    } else {
        source
            .secret_ref
            .as_deref()
            .and_then(|reference| vault::get(reference).ok().flatten())
            .is_some()
    }
}

fn aggregate_status(
    sources: &[SourceSummaryViewModel],
    capabilities: &[CapabilitySnapshotViewModel],
) -> PlatformAggregateStatus {
    let configured = sources.iter().filter(|source| source.credential_configured).collect::<Vec<_>>();
    if configured.is_empty() {
        return PlatformAggregateStatus::SetupRequired;
    }
    let has_value = capabilities.iter().any(|value| value.freshness != DataFreshness::Missing);
    let all_ready = configured.iter().all(|source| matches!(source.state, SourceState::Ready));
    let all_fresh = capabilities.iter().all(|value| {
        value.freshness == DataFreshness::Fresh
            || (value.capability_id == "credits" && value.freshness == DataFreshness::Missing)
    });
    if all_ready && all_fresh {
        PlatformAggregateStatus::Healthy
    } else if has_value || configured.iter().any(|source| matches!(source.state, SourceState::Ready | SourceState::Refreshing)) {
        PlatformAggregateStatus::Partial
    } else {
        PlatformAggregateStatus::Error
    }
}

fn credential_input(source_id: &str, source_type: &str) -> Option<CredentialInputViewModel> {
    match source_id {
        deepseek::BALANCE_SOURCE_ID => Some(CredentialInputViewModel {
            label: "DeepSeek API Key".into(),
            placeholder: "sk-…".into(),
            help_text: "先点「验证连接」，通过后再保存。密钥只进入 Windows Credential Manager。".into(),
            secret_kind: "api_key".into(),
        }),
        deepseek::WEB_SOURCE_ID => Some(CredentialInputViewModel {
            label: "DeepSeek 网页 usage token".into(),
            placeholder: "粘贴 Bearer token，或使用网页登录".into(),
            help_text: "用于平台网页内部用量接口；Token 只进入 Windows Credential Manager。".into(),
            secret_kind: "bearer_token".into(),
        }),
        _ if source_type == "api_key" => Some(CredentialInputViewModel {
            label: "API Key".into(),
            placeholder: "sk-…".into(),
            help_text: "先点「验证连接」，通过后再保存。密钥只进入 Windows Credential Manager。".into(),
            secret_kind: "api_key".into(),
        }),
        _ => None,
    }
}

fn source_type(value: &str) -> SourceType {
    match value {
        "web_session" => SourceType::WebSession,
        "local_cli" => SourceType::LocalCli,
        "oauth" => SourceType::OAuth,
        _ => SourceType::ApiKey,
    }
}

fn source_state(value: &str) -> SourceState {
    match value {
        "ready" => SourceState::Ready,
        "refreshing" => SourceState::Refreshing,
        "error" => SourceState::Error,
        _ => SourceState::AuthRequired,
    }
}

fn millis(value: Option<i64>) -> Option<u64> {
    value.and_then(|value| u64::try_from(value).ok())
}
