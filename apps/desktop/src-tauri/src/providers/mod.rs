//! 平台模板注册、真实 ViewModel 聚合与 Source adapter 路由。

pub mod catalog;
pub mod coding_plan;
pub mod codex;
pub mod deepseek;
pub mod glm;
pub mod kimi;
pub mod mimo;
pub mod money;

use crate::domain::{
    CapabilityDisplayValue, CapabilitySnapshotViewModel, CredentialInputViewModel, DataFreshness,
    PlatformAggregateStatus, PlatformSummaryViewModel, RefreshHistoryEntryViewModel, SourceState,
    SourceSummaryViewModel, SourceType, TrendPoint,
};
use crate::storage::database::Database;
use crate::storage::repository::SourceRecord;
use crate::storage::vault;

struct CapabilityTemplate {
    id: String,
    source_id: String,
    display_name: String,
    kind: String,
}

fn template(id: &str, source_id: &str, display_name: &str, kind: &str) -> CapabilityTemplate {
    CapabilityTemplate {
        id: id.into(),
        source_id: source_id.into(),
        display_name: display_name.into(),
        kind: kind.into(),
    }
}

fn deepseek_templates() -> Vec<CapabilityTemplate> {
    vec![
        template("balance", deepseek::BALANCE_SOURCE_ID, "账户余额", "money"),
        template("today_spend", deepseek::WEB_SOURCE_ID, "今日消费", "money"),
        template("month_spend", deepseek::WEB_SOURCE_ID, "本月消费", "money"),
        template("model_usage_v4_flash", deepseek::WEB_SOURCE_ID, "V4 Flash 用量", "tokens"),
        template("model_usage_v4_pro", deepseek::WEB_SOURCE_ID, "V4 Pro 用量", "tokens"),
        template("request_count", deepseek::WEB_SOURCE_ID, "请求数", "tokens"),
        template("prompt_tokens", deepseek::WEB_SOURCE_ID, "输入 Token", "tokens"),
        template("cache_hit_tokens", deepseek::WEB_SOURCE_ID, "输入（命中缓存）", "tokens"),
        template("cache_miss_tokens", deepseek::WEB_SOURCE_ID, "输入（未命中缓存）", "tokens"),
        template("response_tokens", deepseek::WEB_SOURCE_ID, "输出 Token", "tokens"),
        template("cache_hit_rate", deepseek::WEB_SOURCE_ID, "缓存命中率", "percent"),
        template("usage_trend", deepseek::WEB_SOURCE_ID, "近 7 日消费趋势", "trend"),
    ]
}

fn coding_plan_source_id(platform_id: &str) -> &'static str {
    match platform_id {
        "kimi" => coding_plan::KIMI_SOURCE_ID,
        "glm" => coding_plan::GLM_SOURCE_ID,
        "glm_intl" => coding_plan::GLM_INTL_SOURCE_ID,
        "minimax" => coding_plan::MINIMAX_SOURCE_ID,
        "minimax_intl" => coding_plan::MINIMAX_INTL_SOURCE_ID,
        _ => "",
    }
}

fn coding_plan_templates(source_id: &str) -> Vec<CapabilityTemplate> {
    let weekly = if source_id == coding_plan::KIMI_SOURCE_ID { "周限额" } else { "周窗口" };
    vec![
        template("quota_window_5h", source_id, "5 小时窗口", "percent"),
        template("quota_window_7d", source_id, weekly, "percent"),
        template("plan_level", source_id, "订阅计划", "text"),
    ]
}

fn kimi_templates() -> Vec<CapabilityTemplate> {
    let mut templates = coding_plan_templates(coding_plan::KIMI_SOURCE_ID);
    templates.push(template("balance", kimi::BALANCE_SOURCE_ID, "账户余额", "money"));
    templates
}

fn glm_templates() -> Vec<CapabilityTemplate> {
    let mut templates = coding_plan_templates(coding_plan::GLM_SOURCE_ID);
    templates.push(template("balance", glm::WEB_BALANCE_SOURCE_ID, "账户余额", "money"));
    templates
}

fn mimo_templates() -> Vec<CapabilityTemplate> {
    vec![template("balance", mimo::SOURCE_ID, "账户余额", "money")]
}

fn openai_templates(sources: &[SourceRecord]) -> Vec<CapabilityTemplate> {
    let mut templates = Vec::new();
    for source in sources.iter().filter(|source| codex::is_codex_source(&source.id)) {
        let account = if source.id == codex::SOURCE_ID {
            "本机 Codex".to_string()
        } else {
            source.display_name.clone()
        };
        templates.push(template("quota_window_5h", &source.id, &format!("{account} · 5 小时窗口"), "percent"));
        templates.push(template("quota_window_7d", &source.id, &format!("{account} · 7 天窗口"), "percent"));
        templates.push(template("credits", &source.id, &format!("{account} · Credits"), "credits"));
        templates.push(template("plan_level", &source.id, &format!("{account} · 订阅计划"), "text"));
    }
    templates
}

pub fn platform_summaries(database: &Database) -> Result<Vec<PlatformSummaryViewModel>, String> {
    let mut platforms = Vec::new();
    for added in database.list_user_platforms()? {
        ensure_declared_sources(database, &added.platform_id)?;
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
                &deepseek_templates(),
            )?),
            "openai" => {
                let templates = openai_templates(&database.list_sources("openai")?);
                platforms.push(real_platform(
                    database,
                    "openai",
                    display_name,
                    official_url,
                    added.api_base_url.as_deref(),
                    &templates,
                )?)
            }
            "kimi" => platforms.push(real_platform(
                database,
                "kimi",
                display_name,
                official_url,
                added.api_base_url.as_deref(),
                &kimi_templates(),
            )?),
            "glm" => platforms.push(real_platform(
                database,
                "glm",
                display_name,
                official_url,
                added.api_base_url.as_deref(),
                &glm_templates(),
            )?),
            "mimo" => platforms.push(real_platform(
                database,
                "mimo",
                display_name,
                official_url,
                added.api_base_url.as_deref(),
                &mimo_templates(),
            )?),
            id if coding_plan::is_coding_plan_source(coding_plan_source_id(id)) => {
                let source_id = coding_plan_source_id(id).to_string();
                platforms.push(real_platform(
                    database,
                    id,
                    display_name,
                    official_url,
                    added.api_base_url.as_deref(),
                    &coding_plan_templates(&source_id),
                )?)
            }
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
    if added.is_some() {
        ensure_declared_sources(database, platform_id)?;
    }
    let sources = database.list_sources(platform_id)?;
    let api_key_source = sources
        .iter()
        .find(|source| source.id == coding_plan_source_id(platform_id) && source.source_type == "api_key")
        .or_else(|| sources.iter().find(|source| source.source_type == "api_key"));
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
        api_key_configured: api_key_source.is_some_and(|source| source_configured(database, source)),
        local_cli_source_id: sources
            .iter()
            .find(|source| source.source_type == "local_cli")
            .map(|source| source.id.clone()),
        needs_api_key: entry.needs_api_key,
        needs_web_login: entry.needs_web_login,
        needs_local_cli: entry.needs_local_cli,
    })
}

pub fn add_platforms(database: &Database, platform_ids: &[String]) -> Result<Vec<PlatformSummaryViewModel>, String> {
    for platform_id in platform_ids {
        let entry = catalog::entry(platform_id).ok_or_else(|| format!("不支持添加平台：{platform_id}"))?;
        ensure_declared_sources(database, platform_id)?;
        database.add_user_platform(entry.id, entry.display_name, entry.api_base_url)?;
    }
    platform_summaries(database)
}

fn ensure_declared_sources(database: &Database, platform_id: &str) -> Result<(), String> {
    match platform_id {
        "deepseek" | "openai" => Ok(()),
        "kimi" => {
            database.ensure_account_source(
                "kimi-default",
                "kimi",
                coding_plan::KIMI_SOURCE_ID,
                "api_key",
                "Coding Plan",
                "默认账户",
            )?;
            database.ensure_account_source(
                "kimi-default",
                "kimi",
                kimi::BALANCE_SOURCE_ID,
                "api_key",
                "个人余额",
                "默认账户",
            )
        }
        "glm" => {
            database.ensure_account_source(
                "glm-default",
                "glm",
                coding_plan::GLM_SOURCE_ID,
                "api_key",
                "Coding Plan",
                "默认账户",
            )?;
            database.ensure_account_source(
                "glm-default",
                "glm",
                glm::WEB_BALANCE_SOURCE_ID,
                "web_session",
                "网页个人余额",
                "默认账户",
            )
        }
        "glm_intl" => database.ensure_account_source(
            "glm-intl-default",
            "glm_intl",
            coding_plan::GLM_INTL_SOURCE_ID,
            "api_key",
            "Coding Plan",
            "默认账户",
        ),
        "minimax" => database.ensure_account_source(
            "minimax-default",
            "minimax",
            coding_plan::MINIMAX_SOURCE_ID,
            "api_key",
            "Token Plan",
            "默认账户",
        ),
        "minimax_intl" => database.ensure_account_source(
            "minimax-intl-default",
            "minimax_intl",
            coding_plan::MINIMAX_INTL_SOURCE_ID,
            "api_key",
            "Token Plan",
            "默认账户",
        ),
        "claude_code" => database.ensure_account_source(
            "claude-code-default",
            "claude_code",
            "claude-code-local",
            "local_cli",
            "本地 Claude 订阅",
            "本地账户",
        ),
        "mimo" => database.ensure_account_source(
            "mimo-default",
            "mimo",
            mimo::SOURCE_ID,
            "web_session",
            "网页会话",
            "默认账户",
        ),
        _ => Err(format!("不支持添加平台：{platform_id}")),
    }
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
        let credential_configured = source_configured(database, source);
        let mut state = source_state(&source.state);
        if !credential_configured {
            state = SourceState::AuthRequired;
        } else if provider_id == "openai" && matches!(state, SourceState::AuthRequired) {
            state = SourceState::Ready;
        }
        let display_name = if source.id == codex::SOURCE_ID {
            "本机 Codex（当前 CLI）".to_string()
        } else {
            source.display_name.clone()
        };
        sources.push(SourceSummaryViewModel {
            source_id: source.id.clone(),
            source_type: source_type(&source.source_type),
            display_name,
            state,
            credential_configured,
            last_validated_at: millis(source.last_validated_at),
            last_success_at: millis(source.last_success_at),
            error_code: source.error_code.clone(),
            error_message: source.error_message.clone(),
            capability_ids: templates
                .iter()
                .filter(|value| value.source_id == source.id)
                .map(|value| value.id.clone())
                .collect(),
            credential_input: credential_input(&source.id, &source.source_type),
            supports_interactive_login: is_web_login_source(&source.id),
            supports_cli_login: codex::is_codex_source(&source.id),
        });
    }

    let mut capabilities = Vec::with_capacity(templates.len());
    for template in templates {
        let source = records.iter().find(|source| source.id == template.source_id);
        let snapshot = database.latest_snapshot(&template.source_id, &template.id)?;
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
                capability_id: template.id.clone(),
                source_id: template.source_id.clone(),
                display_name: template.display_name.clone(),
                freshness: DataFreshness::Missing,
                captured_at: None,
                last_good_at: None,
                value: CapabilityDisplayValue {
                    kind: template.kind.clone(),
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
        "openai" => {
            let local = sources.iter().any(|source| source.source_id == codex::SOURCE_ID && source.credential_configured);
            let extra = sources.iter().filter(|source| codex::is_extra_source(&source.source_id) && source.credential_configured).count();
            match (local, extra) {
                (true, 0) => "本机 Codex".to_string(),
                (true, count) => format!("本机 Codex + {count} 个额外账号"),
                (false, count) if count > 0 => format!("{count} 个 ChatGPT 账号"),
                _ => "尚未接入".to_string(),
            }
        }
        "kimi" => kimi_access_summary(&sources),
        "glm" => glm_access_summary(&sources),
        "mimo" if configured_count > 0 => "网页会话".to_string(),
        "glm_intl" | "minimax" | "minimax_intl" if configured_count > 0 => "API Key".to_string(),
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

fn is_web_login_source(source_id: &str) -> bool {
    matches!(
        source_id,
        deepseek::WEB_SOURCE_ID | glm::WEB_BALANCE_SOURCE_ID | mimo::SOURCE_ID
    )
}

fn source_configured(database: &Database, source: &SourceRecord) -> bool {
    if source.id == codex::SOURCE_ID {
        return codex::local_auth_available();
    }
    if codex::is_extra_source(&source.id) {
        return database
            .path()
            .parent()
            .map(|data_dir| codex::extra_source_home(data_dir, &source.id))
            .is_some_and(|home| codex::auth_available_at(Some(&home)));
    }
    source
        .secret_ref
        .as_deref()
        .and_then(|reference| vault::get(reference).ok().flatten())
        .is_some()
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
            || (matches!(value.capability_id.as_str(), "credits" | "plan_level" | "quota_window_7d")
                && value.freshness == DataFreshness::Missing)
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
        kimi::BALANCE_SOURCE_ID => Some(CredentialInputViewModel {
            label: "Moonshot 开放平台 API Key".into(),
            placeholder: "sk-…".into(),
            help_text: "查询个人账户余额，不是 Coding Plan Key。官方接口为 api.moonshot.cn/v1/users/me/balance。先验证再保存。".into(),
            secret_kind: "api_key".into(),
        }),
        glm::WEB_BALANCE_SOURCE_ID => Some(CredentialInputViewModel {
            label: "GLM 网页登录 Cookie".into(),
            placeholder: "粘贴包含 bigmodel_token_production 的 Cookie，或使用网页登录".into(),
            help_text: "官方 Coding Plan 不含个人余额。登录成功后自动验证控制台余额接口；Cookie 只进入 Windows Credential Manager。".into(),
            secret_kind: "cookie".into(),
        }),
        mimo::SOURCE_ID => Some(CredentialInputViewModel {
            label: "MiMo 网页登录 Cookie".into(),
            placeholder: "粘贴包含 serviceToken 的 Cookie，或使用网页登录".into(),
            help_text: "MiMo 没有官方余额接口。登录窗口会读取含 httpOnly 的 Cookie 并验证余额；Cookie 只进入 Windows Credential Manager。".into(),
            secret_kind: "cookie".into(),
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

fn kimi_access_summary(sources: &[SourceSummaryViewModel]) -> String {
    let coding = sources.iter().any(|source| source.source_id == coding_plan::KIMI_SOURCE_ID && source.credential_configured);
    let balance = sources.iter().any(|source| source.source_id == kimi::BALANCE_SOURCE_ID && source.credential_configured);
    match (coding, balance) {
        (true, true) => "Coding Plan + 个人余额".into(),
        (true, false) => "Coding Plan".into(),
        (false, true) => "个人余额".into(),
        (false, false) => "尚未接入".into(),
    }
}

fn glm_access_summary(sources: &[SourceSummaryViewModel]) -> String {
    let coding = sources.iter().any(|source| source.source_id == coding_plan::GLM_SOURCE_ID && source.credential_configured);
    let balance = sources.iter().any(|source| source.source_id == glm::WEB_BALANCE_SOURCE_ID && source.credential_configured);
    match (coding, balance) {
        (true, true) => "API Key + 网页会话".into(),
        (true, false) => "API Key".into(),
        (false, true) => "网页会话".into(),
        (false, false) => "尚未接入".into(),
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
