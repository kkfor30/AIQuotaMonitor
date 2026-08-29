//! 平台模板注册、真实 ViewModel 聚合与 Source adapter 路由。

pub mod balance;
pub mod catalog;
pub mod coding_plan;
pub mod codex;
pub mod deepseek;
pub mod glm;
pub mod grok;
pub mod kimi;
pub mod mimo;
pub mod money;

use crate::domain::{
    AccountSummaryViewModel, CapabilityDisplayValue, CapabilitySnapshotViewModel,
    CredentialInputViewModel, DataFreshness, PlatformAggregateStatus, PlatformSummaryViewModel,
    RefreshHistoryEntryViewModel, SourceState, SourceSummaryViewModel, SourceType, TrendPoint,
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

#[derive(Clone, Copy)]
struct SourceDefinition {
    adapter_id: &'static str,
    source_type: &'static str,
    display_name: &'static str,
}

fn source_definitions(platform_id: &str) -> Vec<SourceDefinition> {
    let one = |adapter_id, source_type, display_name| SourceDefinition {
        adapter_id,
        source_type,
        display_name,
    };
    match platform_id {
        "deepseek" => vec![
            one(deepseek::BALANCE_SOURCE_ID, "api_key", "API 余额"),
            one(deepseek::WEB_SOURCE_ID, "web_session", "网页用量与缓存"),
        ],
        "openai" => vec![one(codex::SOURCE_ID, "oauth", "ChatGPT OAuth 订阅")],
        "kimi" => vec![
            one(coding_plan::KIMI_SOURCE_ID, "api_key", "Coding Plan"),
            one(kimi::BALANCE_SOURCE_ID, "api_key", "个人余额"),
        ],
        "glm" => vec![
            one(coding_plan::GLM_SOURCE_ID, "api_key", "Coding Plan"),
            one(glm::WEB_BALANCE_SOURCE_ID, "web_session", "网页个人余额"),
        ],
        "glm_intl" => vec![one(coding_plan::GLM_INTL_SOURCE_ID, "api_key", "Coding Plan")],
        "minimax" => vec![one(coding_plan::MINIMAX_SOURCE_ID, "api_key", "Token Plan")],
        "minimax_intl" => vec![one(coding_plan::MINIMAX_INTL_SOURCE_ID, "api_key", "Token Plan")],
        "claude_code" => vec![one("claude-code-local", "local_cli", "本地 Claude 订阅")],
        "grok" => vec![one(grok::SOURCE_ID, "local_cli", "本机 Grok CLI")],
        "mimo" => vec![one(mimo::SOURCE_ID, "web_session", "网页会话")],
        "siliconflow" => vec![one(balance::SILICONFLOW_SOURCE_ID, "api_key", "账户余额")],
        "siliconflow_intl" => vec![one(balance::SILICONFLOW_INTL_SOURCE_ID, "api_key", "账户余额")],
        "stepfun" => vec![one(balance::STEPFUN_SOURCE_ID, "api_key", "账户余额")],
        "openrouter" => vec![one(balance::OPENROUTER_SOURCE_ID, "api_key", "账户余额")],
        "novita" => vec![one(balance::NOVITA_SOURCE_ID, "api_key", "账户余额")],
        _ => vec![],
    }
}

pub fn supports_multiple_accounts(platform_id: &str) -> bool {
    let definitions = source_definitions(platform_id);
    !definitions.is_empty() && definitions.iter().all(|source| source.source_type != "local_cli")
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
        template("balance", deepseek::BALANCE_SOURCE_ID, "充值余额", "money"),
        template("today_spend", deepseek::WEB_SOURCE_ID, "今日消费", "money"),
        template("total_spend", deepseek::WEB_SOURCE_ID, "累计消费", "money"),
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

fn grok_templates() -> Vec<CapabilityTemplate> {
    vec![template("quota_window_7d", grok::SOURCE_ID, "周窗口", "percent")]
}

fn balance_platform_templates(source_id: &str) -> Vec<CapabilityTemplate> {
    vec![template("balance", source_id, "账户余额", "money")]
}

fn openai_templates(database: &Database, sources: &[SourceRecord]) -> Result<Vec<CapabilityTemplate>, String> {
    let mut templates = Vec::new();
    for source in sources.iter().filter(|source| source.adapter_id == codex::SOURCE_ID) {
        for snapshot in database.latest_window_snapshots(&source.id)? {
            templates.push(template(
                &snapshot.capability_id,
                &source.id,
                &snapshot.display_name,
                "percent",
            ));
        }
        templates.push(template("credits", &source.id, "Credits", "credits"));
        templates.push(template("plan_level", &source.id, "订阅计划", "text"));
    }
    Ok(templates)
}

fn materialize_templates(
    records: &[SourceRecord],
    base_templates: &[CapabilityTemplate],
) -> Vec<CapabilityTemplate> {
    let mut templates = Vec::new();
    for source in records {
        for base in base_templates.iter().filter(|template| template.source_id == source.adapter_id) {
            templates.push(template(&base.id, &source.id, &base.display_name, &base.kind));
        }
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
                platforms.push(real_platform(
                    database,
                    "openai",
                    display_name,
                    official_url,
                    added.api_base_url.as_deref(),
                    &[],
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
            "grok" => platforms.push(real_platform(
                database,
                "grok",
                display_name,
                official_url,
                added.api_base_url.as_deref(),
                &grok_templates(),
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
            id if balance::source_id_for_platform(id).is_some() => {
                let source_id = balance::source_id_for_platform(id).unwrap_or("");
                platforms.push(real_platform(
                    database,
                    id,
                    display_name,
                    official_url,
                    added.api_base_url.as_deref(),
                    &balance_platform_templates(source_id),
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
            item.supports_multiple_accounts = supports_multiple_accounts(entry.id);
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
        .find(|source| {
            source.account_kind != "additional"
                && source.adapter_id == coding_plan_source_id(platform_id)
                && source.source_type == "api_key"
        })
        .or_else(|| sources.iter().find(|source| source.account_kind != "additional" && source.source_type == "api_key"));
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
    if matches!(platform_id, "deepseek" | "openai") {
        return Ok(());
    }
    let definitions = source_definitions(platform_id);
    if definitions.is_empty() {
        return Err(format!("不支持添加平台：{platform_id}"));
    }
    let account_id = format!("{platform_id}-default").replace('_', "-");
    let local = definitions.iter().all(|source| source.source_type == "local_cli");
    let account_kind = if local { "local" } else { "default" };
    let account_name = if local { "本地账户" } else { "默认账户" };
    for definition in definitions {
        database.ensure_account_source_with_adapter(
            &account_id,
            platform_id,
            account_kind,
            definition.adapter_id,
            definition.adapter_id,
            definition.source_type,
            definition.display_name,
            account_name,
        )?;
    }
    Ok(())
}

pub fn create_additional_account(
    database: &Database,
    platform_id: &str,
) -> Result<(String, Vec<String>), String> {
    if database.user_platform(platform_id)?.is_none() {
        return Err("请先添加平台".into());
    }
    if !supports_multiple_accounts(platform_id) {
        return Err("该平台当前不支持添加其他账号".into());
    }
    let existing_accounts = database
        .list_sources(platform_id)?
        .into_iter()
        .map(|source| source.account_id)
        .collect::<std::collections::HashSet<_>>()
        .len();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let account_id = format!("{platform_id}-account-{now}");
    let account_name = if platform_id == "openai" {
        format!("额外 ChatGPT 账号 {existing_accounts}")
    } else {
        format!("账号 {}", existing_accounts + 1)
    };
    let mut source_ids = Vec::new();
    for definition in source_definitions(platform_id) {
        let source_id = if platform_id == "openai" {
            format!("{}{}-{now}", codex::EXTRA_SOURCE_PREFIX, existing_accounts)
        } else {
            format!("{}--{now}", definition.adapter_id)
        };
        database.ensure_account_source_with_adapter(
            &account_id,
            platform_id,
            "additional",
            &source_id,
            definition.adapter_id,
            definition.source_type,
            definition.display_name,
            &account_name,
        )?;
        source_ids.push(source_id);
    }
    Ok((account_id, source_ids))
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
    let templates = if provider_id == "openai" {
        openai_templates(database, &records)?
    } else {
        materialize_templates(&records, templates)
    };
    let mut sources = Vec::with_capacity(records.len());
    for source in &records {
        let credential_configured = source_configured(database, source);
        let mut state = source_state(&source.state);
        if !credential_configured {
            state = SourceState::AuthRequired;
        } else if source.adapter_id == codex::SOURCE_ID && matches!(state, SourceState::AuthRequired) {
            state = SourceState::Ready;
        }
        let display_name = if source.account_kind == "local" && source.adapter_id == codex::SOURCE_ID {
            "本机 Codex（当前 CLI）".to_string()
        } else {
            source.display_name.clone()
        };
        sources.push(SourceSummaryViewModel {
            source_id: source.id.clone(),
            adapter_id: source.adapter_id.clone(),
            account_id: source.account_id.clone(),
            account_name: source.account_name.clone(),
            account_kind: source.account_kind.clone(),
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
            credential_input: credential_input(&source.adapter_id, &source.source_type),
            supports_interactive_login: is_web_login_source(&source.adapter_id),
            supports_cli_login: source.adapter_id == codex::SOURCE_ID,
            access_mode: access_mode(&source.adapter_id, &source.source_type),
        });
    }

    let mut capabilities = Vec::with_capacity(templates.len());
    for template in &templates {
        let source = records.iter().find(|source| source.id == template.source_id);
        let snapshot = database.latest_snapshot(&template.source_id, &template.id)?;
        capabilities.push(match (source, snapshot) {
            (Some(source), Some(snapshot))
                if template.id != "credits"
                    || (source.state == "ready" && source.last_validated_at == Some(snapshot.captured_at)) =>
            {
                let current = source.last_validated_at == Some(snapshot.captured_at);
                let quota_dropped = template.id.starts_with("quota_window_") && source.state == "ready" && !current;
                if quota_dropped {
                    CapabilitySnapshotViewModel {
                        capability_id: template.id.clone(),
                        source_id: template.source_id.clone(),
                        account_id: source.account_id.clone(),
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
                    }
                } else {
                    let fresh = current || source.state == "ready";
                    CapabilitySnapshotViewModel {
                        capability_id: snapshot.capability_id,
                        source_id: source.id.clone(),
                        account_id: source.account_id.clone(),
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
            }
            _ => CapabilitySnapshotViewModel {
                capability_id: template.id.clone(),
                source_id: template.source_id.clone(),
                account_id: source.map(|item| item.account_id.clone()).unwrap_or_default(),
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
    let platform_status = aggregate_status(&sources, &capabilities);
    let mut accounts = Vec::new();
    for record in &records {
        if accounts.iter().any(|account: &AccountSummaryViewModel| account.account_id == record.account_id) {
            continue;
        }
        let account_sources = sources
            .iter()
            .filter(|source| source.account_id == record.account_id)
            .cloned()
            .collect::<Vec<_>>();
        let source_ids = account_sources
            .iter()
            .map(|source| source.source_id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let account_capabilities = capabilities
            .iter()
            .filter(|capability| source_ids.contains(capability.source_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        accounts.push(AccountSummaryViewModel {
            account_id: record.account_id.clone(),
            display_name: record.account_name.clone(),
            kind: record.account_kind.clone(),
            status: aggregate_status(&account_sources, &account_capabilities),
            source_ids: account_sources.into_iter().map(|source| source.source_id).collect(),
            can_rename: record.account_kind == "additional",
            can_remove: record.account_kind == "additional",
        });
    }
    let configured_count = sources.iter().filter(|source| source.credential_configured).count();
    let access_summary = match provider_id {
        "deepseek" if configured_count == 2 => "API Key + 网页会话".to_string(),
        "deepseek" if sources.iter().any(|source| source.source_id == deepseek::BALANCE_SOURCE_ID && source.credential_configured) => "API Key".to_string(),
        "deepseek" if configured_count > 0 => "网页会话".to_string(),
        "openai" => {
            let local = sources.iter().any(|source| source.account_kind == "local" && source.adapter_id == codex::SOURCE_ID && source.credential_configured);
            let extra = accounts.iter().filter(|account| {
                account.kind == "additional" && account.source_ids.iter().any(|source_id| {
                    sources.iter().any(|source| &source.source_id == source_id && source.credential_configured)
                })
            }).count();
            match (local, extra) {
                (true, 0) => "本机 Codex".to_string(),
                (true, count) => format!("本机 Codex + {count} 个额外账号"),
                (false, count) if count > 0 => format!("{count} 个 ChatGPT 账号"),
                _ => "尚未接入".to_string(),
            }
        }
        "kimi" => kimi_access_summary(&sources),
        "glm" | "glm_intl" => glm_access_summary(&sources),
        "mimo" if configured_count > 0 => "网页会话".to_string(),
        "grok" if configured_count > 0 => "本机 Grok CLI".to_string(),
        "minimax" | "minimax_intl" if configured_count > 0 => "Token Plan".to_string(),
        id if balance::source_id_for_platform(id).is_some() && configured_count > 0 => "API Key".to_string(),
        _ => "尚未接入".to_string(),
    };
    let refresh_history = database
        .refresh_history(provider_id, 8)?
        .into_iter()
        .map(|entry| RefreshHistoryEntryViewModel {
            id: entry.id,
            source_id: entry.source_id,
            source_name: entry.source_name,
            account_id: entry.account_id,
            account_name: entry.account_name,
            status: entry.status,
            finished_at: millis(entry.finished_at),
            error_message: entry.error_message,
        })
        .collect();

    Ok(PlatformSummaryViewModel {
        provider_id: provider_id.into(),
        display_name: display_name.into(),
        aggregate_status: platform_status,
        official_url: Some(official_url.into()),
        api_base_url: api_base_url.map(str::to_string),
        access_summary,
        supports_multiple_accounts: supports_multiple_accounts(provider_id),
        accounts,
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
    if source.adapter_id == codex::SOURCE_ID && source.account_kind == "local" {
        return codex::local_auth_available();
    }
    if source.adapter_id == grok::SOURCE_ID {
        return grok::local_auth_available();
    }
    if source.adapter_id == codex::SOURCE_ID && source.account_kind == "additional" {
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
    let configured_ids = configured
        .iter()
        .map(|source| source.source_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let relevant = capabilities
        .iter()
        .filter(|value| configured_ids.contains(value.source_id.as_str()))
        .collect::<Vec<_>>();
    let has_value = relevant.iter().any(|value| value.freshness != DataFreshness::Missing);
    let all_ready = configured.iter().all(|source| matches!(source.state, SourceState::Ready));
    let all_fresh = relevant.iter().all(|value| {
        value.freshness == DataFreshness::Fresh || missing_capability_ok(value, capabilities)
    });
    if all_ready && all_fresh {
        PlatformAggregateStatus::Healthy
    } else if has_value
        || configured
            .iter()
            .any(|source| matches!(source.state, SourceState::Ready | SourceState::Refreshing))
    {
        PlatformAggregateStatus::Partial
    } else {
        PlatformAggregateStatus::Error
    }
}

fn missing_capability_ok(
    capability: &CapabilitySnapshotViewModel,
    _capabilities: &[CapabilitySnapshotViewModel],
) -> bool {
    if capability.freshness != DataFreshness::Missing {
        return false;
    }
    capability.capability_id == "credits"
        || capability.capability_id == "plan_level"
        || capability.capability_id == "total_spend"
        || capability.capability_id.starts_with("quota_window_")
}

fn access_mode(source_id: &str, source_type: &str) -> String {
    if coding_plan::is_coding_plan_source(source_id) {
        return if matches!(
            source_id,
            coding_plan::KIMI_SOURCE_ID | coding_plan::GLM_SOURCE_ID | coding_plan::GLM_INTL_SOURCE_ID
        ) {
            "coding_plan".into()
        } else {
            "token_plan".into()
        };
    }
    match source_id {
        deepseek::BALANCE_SOURCE_ID | kimi::BALANCE_SOURCE_ID | glm::WEB_BALANCE_SOURCE_ID | mimo::SOURCE_ID => {
            "personal_balance".into()
        }
        deepseek::WEB_SOURCE_ID => "web_usage".into(),
        _ if source_type == "local_cli" || source_type == "oauth" => "local_cli".into(),
        _ => "personal_balance".into(),
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
            help_text: "官方 Coding Plan 不含个人余额。点「网页登录」，看到财务总览后会自动保存，不必把 Cookie 粘贴到输入框。".into(),
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
    let coding = sources.iter().any(|source| source.adapter_id == coding_plan::KIMI_SOURCE_ID && source.credential_configured);
    let balance = sources.iter().any(|source| source.adapter_id == kimi::BALANCE_SOURCE_ID && source.credential_configured);
    match (coding, balance) {
        (true, true) => "Coding Plan + 个人余额".into(),
        (true, false) => "Coding Plan".into(),
        (false, true) => "个人余额".into(),
        (false, false) => "尚未接入".into(),
    }
}

fn glm_access_summary(sources: &[SourceSummaryViewModel]) -> String {
    let coding = sources.iter().any(|source| {
        matches!(
            source.adapter_id.as_str(),
            coding_plan::GLM_SOURCE_ID | coding_plan::GLM_INTL_SOURCE_ID
        ) && source.credential_configured
    });
    let balance = sources.iter().any(|source| source.adapter_id == glm::WEB_BALANCE_SOURCE_ID && source.credential_configured);
    match (coding, balance) {
        (true, true) => "Token Plan + 个人余额".into(),
        (true, false) => "Token Plan".into(),
        (false, true) => "个人余额".into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CapabilityDisplayValue, DataFreshness, SourceState, SourceType};

    fn source(id: &str, configured: bool, state: SourceState) -> SourceSummaryViewModel {
        SourceSummaryViewModel {
            source_id: id.into(),
            adapter_id: id.into(),
            account_id: format!("{id}-account"),
            account_name: "测试账户".into(),
            account_kind: "default".into(),
            source_type: SourceType::ApiKey,
            display_name: id.into(),
            state,
            credential_configured: configured,
            last_validated_at: None,
            last_success_at: None,
            error_code: None,
            error_message: None,
            capability_ids: vec![],
            credential_input: None,
            supports_interactive_login: false,
            supports_cli_login: false,
            access_mode: "token_plan".into(),
        }
    }

    fn capability(id: &str, source_id: &str, freshness: DataFreshness) -> CapabilitySnapshotViewModel {
        CapabilitySnapshotViewModel {
            capability_id: id.into(),
            source_id: source_id.into(),
            account_id: format!("{source_id}-account"),
            display_name: id.into(),
            freshness,
            captured_at: None,
            last_good_at: None,
            value: CapabilityDisplayValue {
                kind: "percent".into(),
                primary: (freshness != DataFreshness::Missing).then(|| "80%".into()),
                secondary: None,
                progress: None,
            },
            trend: vec![],
        }
    }

    #[test]
    fn unconfigured_optional_source_does_not_make_platform_partial() {
        let sources = vec![
            source("glm-coding-plan", true, SourceState::Ready),
            source("glm-web-balance", false, SourceState::AuthRequired),
        ];
        let capabilities = vec![
            capability("quota_window_5h", "glm-coding-plan", DataFreshness::Fresh),
            capability("quota_window_7d", "glm-coding-plan", DataFreshness::Fresh),
            capability("plan_level", "glm-coding-plan", DataFreshness::Fresh),
            capability("balance", "glm-web-balance", DataFreshness::Missing),
        ];
        assert_eq!(aggregate_status(&sources, &capabilities), PlatformAggregateStatus::Healthy);
    }

    #[test]
    fn mixed_configured_source_health_is_partial() {
        let sources = vec![
            source("glm-coding-plan", true, SourceState::Ready),
            source("glm-web-balance", true, SourceState::Error),
        ];
        let capabilities = vec![
            capability("quota_window_5h", "glm-coding-plan", DataFreshness::Fresh),
            capability("balance", "glm-web-balance", DataFreshness::Stale),
        ];
        assert_eq!(aggregate_status(&sources, &capabilities), PlatformAggregateStatus::Partial);
    }

    #[test]
    fn no_configured_source_is_setup_required() {
        let sources = vec![source("glm-coding-plan", false, SourceState::AuthRequired)];
        assert_eq!(aggregate_status(&sources, &[]), PlatformAggregateStatus::SetupRequired);
    }

    #[test]
    fn free_extra_account_missing_windows_keeps_platform_healthy() {
        let sources = vec![
            source("openai-codex-local", true, SourceState::Ready),
            source("openai-codex-extra-1", true, SourceState::Ready),
        ];
        let mut extra_plan = capability("plan_level", "openai-codex-extra-1", DataFreshness::Fresh);
        extra_plan.value.primary = Some("Free".into());
        let capabilities = vec![
            capability("quota_window_5h", "openai-codex-local", DataFreshness::Fresh),
            capability("quota_window_7d", "openai-codex-local", DataFreshness::Fresh),
            capability("plan_level", "openai-codex-local", DataFreshness::Fresh),
            capability("quota_window_5h", "openai-codex-extra-1", DataFreshness::Missing),
            capability("quota_window_7d", "openai-codex-extra-1", DataFreshness::Missing),
            capability("credits", "openai-codex-extra-1", DataFreshness::Missing),
            extra_plan,
        ];
        assert_eq!(aggregate_status(&sources, &capabilities), PlatformAggregateStatus::Healthy);
    }

    #[test]
    fn free_extra_account_with_monthly_window_is_healthy() {
        let sources = vec![
            source("openai-codex-local", true, SourceState::Ready),
            source("openai-codex-extra-1", true, SourceState::Ready),
        ];
        let mut extra_plan = capability("plan_level", "openai-codex-extra-1", DataFreshness::Fresh);
        extra_plan.value.primary = Some("Free".into());
        let capabilities = vec![
            capability("quota_window_5h", "openai-codex-local", DataFreshness::Fresh),
            capability("quota_window_7d", "openai-codex-local", DataFreshness::Fresh),
            capability("plan_level", "openai-codex-local", DataFreshness::Fresh),
            capability("quota_window_5h", "openai-codex-extra-1", DataFreshness::Missing),
            capability("quota_window_7d", "openai-codex-extra-1", DataFreshness::Missing),
            capability("quota_window_30d", "openai-codex-extra-1", DataFreshness::Fresh),
            extra_plan,
        ];
        assert_eq!(aggregate_status(&sources, &capabilities), PlatformAggregateStatus::Healthy);
    }

    #[test]
    fn plus_or_pro_without_5h_window_is_healthy() {
        let sources = vec![source("openai-codex-local", true, SourceState::Ready)];
        let mut plan = capability("plan_level", "openai-codex-local", DataFreshness::Fresh);
        plan.value.primary = Some("Pro".into());
        let capabilities = vec![
            capability("quota_window_5h", "openai-codex-local", DataFreshness::Missing),
            capability("quota_window_7d", "openai-codex-local", DataFreshness::Fresh),
            plan,
        ];
        assert_eq!(aggregate_status(&sources, &capabilities), PlatformAggregateStatus::Healthy);
    }
}
