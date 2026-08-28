//! 平台模板注册、真实 ViewModel 聚合与 Source adapter 路由。

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
    Ok(vec![
        real_platform(database, "deepseek", "DeepSeek", "https://platform.deepseek.com", DEEPSEEK_CAPABILITIES)?,
        real_platform(database, "openai", "GPT / Codex", "https://chatgpt.com/codex", CODEX_CAPABILITIES)?,
        setup_required_placeholder("claude_code", "Claude Code"),
        setup_required_placeholder("glm", "GLM"),
        setup_required_placeholder("kimi", "Kimi"),
        setup_required_placeholder("mimo", "MiMo"),
        setup_required_placeholder("minimax", "MiniMax"),
    ])
}

fn real_platform(
    database: &Database,
    provider_id: &str,
    display_name: &str,
    official_url: &str,
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
            credential_input: credential_input(&source.id),
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

fn credential_input(source_id: &str) -> Option<CredentialInputViewModel> {
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

fn setup_required_placeholder(provider_id: &str, display_name: &str) -> PlatformSummaryViewModel {
    PlatformSummaryViewModel {
        provider_id: provider_id.into(),
        display_name: display_name.into(),
        aggregate_status: PlatformAggregateStatus::SetupRequired,
        official_url: None,
        access_summary: "尚未接入".into(),
        sources: vec![],
        capabilities: vec![],
        refresh_history: vec![],
    }
}
