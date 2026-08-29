//! GPT 重置雷达：从 CodexRadar 公开首页同步 Tibo 动态，并保留可选 AI 分析。
//! 与平台额度域隔离，失败不改写 Source 聚合状态。

mod codexradar;

use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
use crate::storage::repository::{RadarAnalysisRecord, RadarCheckRecord, TiboPostRecord};
use crate::storage::vault;
use chrono::{Duration as ChronoDuration, Local, TimeZone};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::error::Error;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const FEED_URL: &str = "https://codexradar.com/";
pub const PROMPT_VERSION: &str = "radar-v3";
pub const USER_PROMPT_MAX_CHARS: usize = 4000;
pub const DEFAULT_USER_PROMPT: &str = "若帖子提到仪表盘（dashboard）、里程碑（milestone）、庆祝（celebration）、倒计时，或出现 “Hold on to your Codex” / “抓紧你的 Codex” 等措辞，视为即将重置的强信号，把握度应偏高，即使没有给出确切时间。\n已落地的历史重置只作背景，不能当成否定新一轮重置的证据。\n没有重置相关内容，或只有旧重置而没有新信号时，才使用低把握度。";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TiboPostView {
    pub id: String,
    pub url: String,
    pub text: String,
    pub posted_at: i64,
    pub kind: String,
    pub badge: String,
    pub filter: String,
    pub explicit_reset: bool,
    pub is_reply: bool,
    pub replies: i64,
    pub reposts: i64,
    pub likes: i64,
    pub synced_at: i64,
    pub translated_text: Option<String>,
    pub translated_at: Option<i64>,
    pub translation_source: Option<String>,
    pub summary: Option<String>,
    pub analysis: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarCheckView {
    pub id: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub status: String,
    pub sync_status: Option<String>,
    pub parse_status: Option<String>,
    pub analyze_status: Option<String>,
    pub error_message: Option<String>,
    pub post_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarAnalysisView {
    pub id: String,
    pub created_at: i64,
    pub range_key: String,
    pub cut_post_id: Option<String>,
    pub cut_label: Option<String>,
    pub source_id: Option<String>,
    pub model: Option<String>,
    pub conclusion: Option<String>,
    pub confidence: Option<String>,
    pub citations: Vec<String>,
    pub support: Vec<String>,
    pub against: Vec<String>,
    pub uncertainty: Vec<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarModelOption {
    pub source_id: String,
    pub platform_id: String,
    pub display_name: String,
    pub model: String,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarSnapshot {
    pub source_status: String,
    pub last_synced_at: Option<i64>,
    pub posts: Vec<TiboPostView>,
    pub latest: Option<TiboPostView>,
    pub checks: Vec<RadarCheckView>,
    pub analysis: Option<RadarAnalysisView>,
    pub models: Vec<RadarModelOption>,
    pub analysis_prefs: RadarAnalysisPrefs,
    pub notice: Option<RadarNotice>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarAnalysisPrefs {
    pub analyze: bool,
    pub range_key: String,
    pub source_id: Option<String>,
    pub user_prompt: String,
    pub default_user_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarNotice {
    pub headline: String,
    pub lead: Option<String>,
    pub items: Vec<String>,
}

pub fn snapshot(database: &Database) -> Result<RadarSnapshot, String> {
    let posts = database
        .list_tibo_posts(80)?
        .into_iter()
        .map(to_view)
        .collect::<Vec<_>>();
    let latest = posts.first().cloned();
    let last_synced_at = posts.first().map(|post| post.synced_at);
    let checks: Vec<_> = database
        .list_radar_checks(8)?
        .into_iter()
        .map(check_view)
        .collect();
    let source_status = if posts.is_empty() {
        "missing"
    } else if checks.first().is_some_and(|check| {
        check.status == "failed" || check.parse_status.as_deref() == Some("stale")
    }) {
        "stale"
    } else {
        "fresh"
    };
    Ok(RadarSnapshot {
        source_status: source_status.into(),
        last_synced_at,
        posts,
        latest,
        checks,
        analysis: database.latest_radar_analysis()?.map(analysis_view),
        models: chat_models(database)?,
        analysis_prefs: load_analysis_prefs(database)?,
        notice: load_notice(database)?,
    })
}

pub async fn run_check(
    database: &Database,
    coordinator: &RefreshCoordinator,
    analyze: bool,
    range_key: &str,
    source_id: Option<&str>,
    model: Option<&str>,
    user_prompt: Option<&str>,
) -> Result<RadarSnapshot, String> {
    let _ = save_analysis_prefs(database, analyze, range_key, source_id, user_prompt);
    let started = epoch_ms();
    let id = format!("radar-{started}");
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("初始化雷达客户端失败：{error}"))?;
    let (sync_status, parse_status, post_count, error_message) = match fetch_feed(&client).await {
        Ok((posts, notice)) => {
            let count = posts.len() as i64;
            database.replace_tibo_posts(&posts, started)?;
            save_notice(database, notice.as_ref())?;
            ("success".into(), "success".into(), count, None)
        }
        Err(error) => ("failed".into(), "failed".into(), 0, Some(error)),
    };

    let mut analyze_status = if analyze { "skipped".to_string() } else { "off".into() };
    let mut analyze_error = None;
    if analyze && error_message.is_none() {
        match run_analysis(database, coordinator, range_key, source_id, model).await {
            Ok(()) => analyze_status = "success".into(),
            Err(error) => {
                analyze_status = "failed".into();
                analyze_error = Some(error);
            }
        }
    }

    let failed = error_message.is_some() || analyze_status == "failed";
    database.insert_radar_check(&RadarCheckRecord {
        id,
        started_at: started,
        finished_at: Some(epoch_ms()),
        status: if failed { "failed".into() } else { "success".into() },
        sync_status: Some(sync_status),
        parse_status: Some(parse_status),
        analyze_status: Some(analyze_status),
        error_message: error_message.or(analyze_error),
        post_count,
    })?;
    snapshot(database)
}

/// 单条 Tibo 动态的中文翻译：调用已接入的对话模型，结果写回 SQLite 缓存。
pub async fn translate_post(
    database: &Database,
    coordinator: &RefreshCoordinator,
    post_id: &str,
    source_id: Option<&str>,
) -> Result<RadarSnapshot, String> {
    let post = database
        .list_tibo_posts(80)?
        .into_iter()
        .find(|post| post.id == post_id)
        .ok_or_else(|| format!("雷达动态 {post_id} 不存在"))?;
    let target = resolve_chat_target(database, source_id, None)?;
    let body = json!({
        "model": target.model,
        "temperature": 0.1,
        "messages": [
            {"role": "system", "content": "Translate the user's English post into Simplified Chinese. Output only the translation itself, keep numbers, URLs and code unchanged."},
            {"role": "user", "content": post.text}
        ]
    });
    let text = send_chat(coordinator.client(), &target, &body).await?;
    let translated = extract_chat_text(&text)?;
    if translated.is_empty() {
        return Err("模型未返回可用的翻译".into());
    }
    let source_label = chat_target(&target.source_id)
        .map(|(display, _)| format!("{display} · {}", target.model))
        .unwrap_or_else(|| target.model.clone());
    database.update_tibo_translation(post_id, &translated, epoch_ms(), &source_label)?;
    snapshot(database)
}

/// 可用对话模型解析：优先用户指定来源，否则取第一个已就绪的 API Key 来源。
struct ChatTarget {
    source_id: String,
    model: String,
    secret: String,
    api_base: Option<String>,
}

fn resolve_chat_target(
    database: &Database,
    source_id: Option<&str>,
    model: Option<&str>,
) -> Result<ChatTarget, String> {
    let source_id = source_id
        .map(str::to_string)
        .or_else(|| {
            chat_models(database)
                .ok()
                .and_then(|models| models.into_iter().find(|item| item.ready).map(|item| item.source_id))
        })
        .ok_or_else(|| "请先接入可用于对话的 API Key（DeepSeek / GLM / Kimi 开放平台 / MiniMax）".to_string())?;
    let source = database.source(&source_id)?;
    let secret = source
        .secret_ref
        .as_deref()
        .and_then(|reference| vault::get(reference).ok().flatten())
        .ok_or_else(|| "所选模型没有可用凭据".to_string())?;
    let model = model
        .map(str::to_string)
        .or_else(|| chat_target(&source_id).map(|(_, model)| model.to_string()))
        .ok_or_else(|| "该来源不支持对话分析".to_string())?;
    let api_base = database.user_platform(&source.platform_id)?.and_then(|item| item.api_base_url);
    if chat_endpoint_candidates(&source_id, api_base.as_deref()).is_empty() {
        return Err("该来源没有对话接口".into());
    }
    Ok(ChatTarget {
        source_id,
        model,
        secret,
        api_base,
    })
}

/// 从 OpenAI 兼容响应中取出 assistant 文本。
fn extract_chat_text(body: &str) -> Result<String, String> {
    let value: Value = serde_json::from_str(body).map_err(|_| "对话返回不是 JSON".to_string())?;
    Ok(value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string())
}

async fn fetch_feed(client: &Client) -> Result<(Vec<TiboPostRecord>, Option<RadarNotice>), String> {
    let response = client
        .get(FEED_URL)
        .header("Accept", "text/html")
        .header(
            "User-Agent",
            "AIQuotaMonitor/0.1 (desktop; Tibo radar sync; +https://codexradar.com/)",
        )
        .send()
        .await
        .map_err(|error| format!("无法连接 CodexRadar：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("CodexRadar 返回 HTTP {}", response.status().as_u16()));
    }
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取 CodexRadar 失败：{error}"))?;
    let synced_at = epoch_ms();
    codexradar::parse_page(&body, synced_at)
}

fn save_notice(database: &Database, notice: Option<&RadarNotice>) -> Result<(), String> {
    let payload = match notice {
        Some(notice) => serde_json::to_string(notice).unwrap_or_else(|_| "{}".into()),
        None => String::new(),
    };
    database.set_setting_string("radar_notice_json", &payload)
}

fn load_notice(database: &Database) -> Result<Option<RadarNotice>, String> {
    let Some(raw) = database.setting_string("radar_notice_json")? else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    Ok(serde_json::from_str(&raw).ok())
}

fn to_view(post: TiboPostRecord) -> TiboPostView {
    let extra = extra_object(&post.extra_json);
    let badge = extra_string(&extra, "signalLabel")
        .or_else(|| post.tibo_lane.clone())
        .map(|value| display_signal_label(&value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| badge_for(&post));
    let filter = extra_string(&extra, "relevance")
        .map(|value| match value.as_str() {
            "none" => "none".into(),
            "indirect" => "related".into(),
            _ => "signal".into(),
        })
        .unwrap_or_else(|| filter_for(&post));
    TiboPostView {
        id: post.id,
        url: post.url,
        text: post.text,
        posted_at: post.posted_at,
        kind: post.kind,
        badge,
        filter,
        explicit_reset: post.explicit_reset,
        is_reply: post.is_reply,
        replies: post.replies,
        reposts: post.reposts,
        likes: post.likes,
        synced_at: post.synced_at,
        translated_text: post.translated_text,
        translated_at: post.translated_at,
        translation_source: post.translation_source,
        summary: extra_string(&extra, "summary"),
        analysis: extra_string(&extra, "analysis"),
    }
}

fn extra_object(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or(Value::Null)
}

fn extra_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// 把来源里残留的英文 lane / 标签转成中文；已经是中文的原样保留。
pub(super) fn display_signal_label(raw: &str) -> String {
    let normalized = raw.trim().replace('-', "_").replace(' ', "_").to_ascii_lowercase();
    match normalized.as_str() {
        "reset_related" | "related" | "direct" | "signal" | "reset" | "verifying" => "重置相关".into(),
        "reset_announcement" | "announcement" => "重置公告".into(),
        "none" | "no_signal" | "no_reset" => "无重置信号".into(),
        "indirect" => "间接相关".into(),
        "limits" | "limit" => "限制".into(),
        "banked" | "landed" => "已落地".into(),
        "note" => "动态".into(),
        _ => raw.trim().to_string(),
    }
}

fn badge_for(post: &TiboPostRecord) -> String {
    if post.explicit_reset {
        return "重置相关".into();
    }
    match post.kind.as_str() {
        "none" => "无重置信号".into(),
        "indirect" => "间接相关".into(),
        "banked" => "已落地".into(),
        "limits" => "限制".into(),
        "candidate" | "signal" | "direct" | "reset" => "重置相关".into(),
        _ => "动态".into(),
    }
}

fn filter_for(post: &TiboPostRecord) -> String {
    match post.kind.as_str() {
        "none" => "none".into(),
        "indirect" => "related".into(),
        "limits" => "related".into(),
        _ if post.explicit_reset
            || matches!(post.kind.as_str(), "candidate" | "signal" | "banked" | "direct" | "reset")
            || matches!(
                post.tibo_lane.as_deref(),
                Some("reset_announcement" | "重置公告" | "重置相关")
            ) =>
        {
            "signal".into()
        }
        _ => "none".into(),
    }
}

fn check_view(check: RadarCheckRecord) -> RadarCheckView {
    RadarCheckView {
        id: check.id,
        started_at: check.started_at,
        finished_at: check.finished_at,
        status: check.status,
        sync_status: check.sync_status,
        parse_status: check.parse_status,
        analyze_status: check.analyze_status,
        error_message: check.error_message,
        post_count: check.post_count,
    }
}

fn analysis_view(record: RadarAnalysisRecord) -> RadarAnalysisView {
    RadarAnalysisView {
        id: record.id,
        created_at: record.created_at,
        range_key: record.range_key,
        cut_post_id: record.cut_post_id.clone(),
        cut_label: None,
        source_id: record.source_id,
        model: record.model,
        conclusion: record.conclusion,
        confidence: record.confidence,
        citations: json_list(&record.citations_json),
        support: json_list(&record.support_json),
        against: json_list(&record.against_json),
        uncertainty: json_list(&record.uncertainty_json),
        error_message: record.error_message,
    }
}

fn json_list(value: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(value).unwrap_or_default()
}

fn chat_models(database: &Database) -> Result<Vec<RadarModelOption>, String> {
    let mut options = Vec::new();
    for platform in database.list_user_platforms()? {
        for source in database.list_sources(&platform.platform_id)? {
            if source.source_type != "api_key" {
                continue;
            }
            let Some((display, model)) = chat_target(&source.id) else {
                continue;
            };
            let ready = source
                .secret_ref
                .as_deref()
                .and_then(|reference| vault::get(reference).ok().flatten())
                .is_some()
                && source.state != "error";
            options.push(RadarModelOption {
                source_id: source.id,
                platform_id: platform.platform_id.clone(),
                display_name: display.into(),
                model: model.into(),
                ready,
            });
        }
    }
    Ok(options)
}

fn chat_target(source_id: &str) -> Option<(&'static str, &'static str)> {
    match source_id {
        "deepseek-balance-api" => Some(("DeepSeek", "deepseek-chat")),
        "glm-coding-plan" => Some(("GLM 国内", "glm-4.5-flash")),
        "glm-intl-coding-plan" => Some(("GLM 国际", "glm-4.5-flash")),
        "kimi-balance-api" => Some(("Kimi 开放平台", "kimi-k2-turbo-preview")),
        "kimi-coding-plan" => Some(("Kimi Coding", "kimi-k2-turbo-preview")),
        "minimax-coding-plan" => Some(("MiniMax", "MiniMax-M2.5")),
        "minimax-intl-coding-plan" => Some(("MiniMax 国际", "MiniMax-M2.5")),
        _ => None,
    }
}

fn glm_source(source_id: &str) -> bool {
    matches!(source_id, "glm-coding-plan" | "glm-intl-coding-plan")
}

fn trim_api_base(api_base: Option<&str>, fallback: &str) -> String {
    api_base
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .trim_end_matches('/')
        .to_string()
}

fn join_chat_url(base: &str, path: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        return base.to_string();
    }
    if base.ends_with("/api/coding/paas/v4") || base.ends_with("/api/paas/v4") || base.ends_with("/v1") {
        return format!("{base}/chat/completions");
    }
    format!("{base}{path}")
}

fn chat_endpoint_candidates(source_id: &str, api_base: Option<&str>) -> Vec<(String, bool)> {
    match source_id {
        "deepseek-balance-api" => vec![(
            join_chat_url(&trim_api_base(api_base, "https://api.deepseek.com"), "/v1/chat/completions"),
            true,
        )],
        "glm-coding-plan" => glm_chat_candidates(api_base, false),
        "glm-intl-coding-plan" => glm_chat_candidates(api_base, true),
        "kimi-balance-api" => vec![("https://api.moonshot.cn/v1/chat/completions".into(), true)],
        "kimi-coding-plan" => vec![(
            join_chat_url(&trim_api_base(api_base, "https://api.kimi.com/coding"), "/v1/chat/completions"),
            true,
        )],
        "minimax-coding-plan" => vec![(
            join_chat_url(&trim_api_base(api_base, "https://api.minimaxi.com"), "/v1/chat/completions"),
            true,
        )],
        "minimax-intl-coding-plan" => vec![(
            join_chat_url(&trim_api_base(api_base, "https://api.minimax.io"), "/v1/chat/completions"),
            true,
        )],
        _ => Vec::new(),
    }
}

fn glm_chat_candidates(api_base: Option<&str>, intl: bool) -> Vec<(String, bool)> {
    let default = if intl { "https://api.z.ai" } else { "https://open.bigmodel.cn" };
    let base = trim_api_base(api_base, default);
    let coding = join_chat_url(&base, "/api/coding/paas/v4/chat/completions");
    let paas = join_chat_url(&base, "/api/paas/v4/chat/completions");
    let mut urls = vec![(coding.clone(), true)];
    if paas != coding {
        urls.push((paas, true));
    }
    urls
}

async fn run_analysis(
    database: &Database,
    coordinator: &RefreshCoordinator,
    range_key: &str,
    source_id: Option<&str>,
    model: Option<&str>,
) -> Result<(), String> {
    match run_analysis_inner(database, coordinator.client(), range_key, source_id, model).await {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = persist_failed_analysis(database, range_key, source_id, model, &error);
            Err(error)
        }
    }
}

async fn run_analysis_inner(
    database: &Database,
    client: &Client,
    range_key: &str,
    source_id: Option<&str>,
    model: Option<&str>,
) -> Result<(), String> {
    let posts = database.list_tibo_posts(80)?;
    let views = posts.into_iter().map(to_view).collect::<Vec<_>>();
    let window_start = range_start(range_key);
    let selected: Vec<_> = views
        .into_iter()
        .filter(|post| post.posted_at >= window_start)
        .collect();
    if selected.is_empty() {
        return Err("当前时间窗内没有 Tibo 动态可分析".into());
    }
    let target = resolve_chat_target(database, source_id, model)?;
    let input = selected
        .iter()
        .map(|post| format!("{}\n{}\n{}", format_iso(post.posted_at), post.url, post.text))
        .collect::<Vec<_>>()
        .join("\n\n");
    let input_hash = format!("{:x}", simple_hash(&input));
    let user_prompt = load_analysis_prefs(database)?.user_prompt;
    let mut messages = vec![json!({"role": "system", "content": ANALYSIS_SYSTEM_PROMPT})];
    if !user_prompt.is_empty() {
        messages.push(json!({
            "role": "system",
            "content": format!("User semantic hints for judging upcoming-reset signals and confidence:\n{user_prompt}")
        }));
    }
    messages.push(json!({
        "role": "user",
        "content": format!("Analyze every post in the selected time window. Do not skip any of them:\n{input}")
    }));
    let body = json!({
        "model": target.model,
        "temperature": 0.1,
        "stream": false,
        "messages": messages
    });
    let text = send_chat(client, &target, &body).await?;
    let parsed = parse_model_json(&text)?;
    database.insert_radar_analysis(&RadarAnalysisRecord {
        id: format!("analysis-{}", epoch_ms()),
        created_at: epoch_ms(),
        range_key: range_key.into(),
        cut_post_id: None,
        from_posted_at: selected.last().map(|post| post.posted_at),
        to_posted_at: selected.first().map(|post| post.posted_at),
        source_id: Some(target.source_id),
        model: Some(target.model),
        prompt_version: PROMPT_VERSION.into(),
        input_hash,
        conclusion: parsed.conclusion,
        confidence: parsed.confidence,
        citations_json: serde_json::to_string(&parsed.citations).unwrap_or_else(|_| "[]".into()),
        support_json: serde_json::to_string(&parsed.support).unwrap_or_else(|_| "[]".into()),
        against_json: serde_json::to_string(&parsed.against).unwrap_or_else(|_| "[]".into()),
        uncertainty_json: serde_json::to_string(&parsed.uncertainty).unwrap_or_else(|_| "[]".into()),
        error_message: None,
    })
}

const ANALYSIS_SYSTEM_PROMPT: &str = concat!(
    "You analyze public Tibo/Codex reset-related posts in the selected time window. ",
    "Reply with JSON only: {\"conclusion\":\"\",\"confidence\":\"low|medium|high\",\"citations\":[\"\"],\"support\":[\"\"],\"against\":[\"\"],\"uncertainty\":[\"\"]}. ",
    "Write conclusion in Simplified Chinese. ",
    "Apply the user's semantic hints when judging upcoming-reset signals and confidence. ",
    "If no user hints are provided, read the posts ordinarily without inventing extra rules. ",
    "This output is speculation, not an official conclusion. ",
    "Only use the English original posts, timestamps and URLs provided; do not invent quotes."
);

fn persist_failed_analysis(
    database: &Database,
    range_key: &str,
    source_id: Option<&str>,
    model: Option<&str>,
    error: &str,
) -> Result<(), String> {
    database.insert_radar_analysis(&RadarAnalysisRecord {
        id: format!("analysis-{}", epoch_ms()),
        created_at: epoch_ms(),
        range_key: range_key.into(),
        cut_post_id: None,
        from_posted_at: None,
        to_posted_at: None,
        source_id: source_id.map(str::to_string),
        model: model.map(str::to_string),
        prompt_version: PROMPT_VERSION.into(),
        input_hash: String::new(),
        conclusion: None,
        confidence: None,
        citations_json: "[]".into(),
        support_json: "[]".into(),
        against_json: "[]".into(),
        uncertainty_json: "[]".into(),
        error_message: Some(error.to_string()),
    })
}

struct ChatRequestError {
    fatal: bool,
    message: String,
}

async fn send_chat(client: &Client, target: &ChatTarget, body: &Value) -> Result<String, String> {
    let candidates = chat_endpoint_candidates(&target.source_id, target.api_base.as_deref());
    if candidates.is_empty() {
        return Err("该来源没有对话接口".into());
    }
    let mut last_error = "对话请求失败".to_string();
    for (url, default_bearer) in &candidates {
        let styles: Vec<bool> = if glm_source(&target.source_id) {
            vec![true, false]
        } else {
            vec![*default_bearer]
        };
        for bearer in styles {
            match post_chat_once(client, url, bearer, &target.secret, body).await {
                Ok(text) => return Ok(text),
                Err(error) if error.fatal => return Err(error.message),
                Err(error) => last_error = error.message,
            }
        }
    }
    Err(last_error)
}

async fn post_chat_once(
    client: &Client,
    url: &str,
    bearer: bool,
    secret: &str,
    body: &Value,
) -> Result<String, ChatRequestError> {
    let mut last_transport = None;
    for attempt in 0..2 {
        let mut request = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("User-Agent", "AIQuotaMonitor/0.1 (desktop; radar analysis)")
            .timeout(Duration::from_secs(90))
            .version(reqwest::Version::HTTP_11);
        request = if bearer {
            request.bearer_auth(secret.trim())
        } else {
            request.header("Authorization", secret.trim())
        };
        let response = match request.json(body).send().await {
            Ok(response) => response,
            Err(error) => {
                last_transport = Some(error);
                if attempt == 0 {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    continue;
                }
                break;
            }
        };
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if status.as_u16() == 402 || quota_exhausted(&text) {
            return Err(ChatRequestError {
                fatal: true,
                message: "当前模型额度或余额不足，请更换模型后再试".into(),
            });
        }
        if !status.is_success() {
            let detail = chat_error_detail(&text);
            return Err(ChatRequestError {
                fatal: false,
                message: format!("对话接口失败（HTTP {}）{}：{url}", status.as_u16(), detail),
            });
        }
        if !text.trim_start().starts_with('{') {
            return Err(ChatRequestError {
                fatal: false,
                message: format!("对话接口未返回 JSON：{url}"),
            });
        }
        return Ok(text);
    }
    Err(ChatRequestError {
        fatal: false,
        message: format!(
            "分析请求失败：{}",
            last_transport
                .as_ref()
                .map(describe_reqwest)
                .unwrap_or_else(|| "无法连接对话接口".into())
        ),
    })
}

fn quota_exhausted(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    text.contains("余额不足") || text.contains("额度不足") || lowered.contains("insufficient")
}

fn chat_error_detail(text: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        let message = value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .or_else(|| value.get("msg").and_then(Value::as_str))
            .or_else(|| value.get("message").and_then(Value::as_str))
            .unwrap_or("")
            .trim();
        if !message.is_empty() {
            return format!("：{message}");
        }
    }
    String::new()
}

fn describe_reqwest(error: &reqwest::Error) -> String {
    let kind = if error.is_timeout() {
        "超时"
    } else if error.is_connect() {
        "无法连接"
    } else {
        "网络错误"
    };
    let url = error.url().map(|item| format!(" {}", item)).unwrap_or_default();
    let mut chain = Vec::new();
    let mut current: Option<&dyn Error> = Some(error);
    while let Some(item) = current {
        chain.push(item.to_string());
        current = item.source();
    }
    format!("{kind}{url}：{}", chain.join("；"))
}

pub fn save_analysis_prefs(
    database: &Database,
    analyze: bool,
    range_key: &str,
    source_id: Option<&str>,
    user_prompt: Option<&str>,
) -> Result<(), String> {
    let range_key = match range_key {
        "today" | "7d" => range_key,
        _ => "3d",
    };
    database.set_setting_bool("radar_analyze", analyze)?;
    database.set_setting_string("radar_range_key", range_key)?;
    database.set_setting_string("radar_source_id", source_id.unwrap_or(""))?;
    if let Some(user_prompt) = user_prompt {
        database.set_setting_string("radar_user_prompt", &sanitize_user_prompt(user_prompt))?;
    }
    Ok(())
}

fn load_analysis_prefs(database: &Database) -> Result<RadarAnalysisPrefs, String> {
    let stored = database.setting_string("radar_user_prompt")?;
    let user_prompt = match stored {
        None => DEFAULT_USER_PROMPT.to_string(),
        Some(value) => sanitize_user_prompt(&value),
    };
    Ok(RadarAnalysisPrefs {
        analyze: database.setting_bool("radar_analyze")?,
        range_key: database
            .setting_string("radar_range_key")?
            .filter(|value| matches!(value.as_str(), "today" | "3d" | "7d"))
            .unwrap_or_else(|| "3d".into()),
        source_id: database
            .setting_string("radar_source_id")?
            .filter(|value| !value.is_empty()),
        user_prompt,
        default_user_prompt: DEFAULT_USER_PROMPT.into(),
    })
}

pub(crate) fn sanitize_user_prompt(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|ch| *ch == '\n' || *ch == '\t' || !ch.is_control())
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.chars().count() <= USER_PROMPT_MAX_CHARS {
        return trimmed.to_string();
    }
    trimmed.chars().take(USER_PROMPT_MAX_CHARS).collect()
}

struct ModelJson {
    conclusion: Option<String>,
    confidence: Option<String>,
    citations: Vec<String>,
    support: Vec<String>,
    against: Vec<String>,
    uncertainty: Vec<String>,
}

fn parse_model_json(body: &str) -> Result<ModelJson, String> {
    let value: Value = serde_json::from_str(body).map_err(|_| "分析返回不是 JSON".to_string())?;
    let content = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or(body);
    let trimmed = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let parsed: Value =
        serde_json::from_str(trimmed).map_err(|_| "模型未返回可解析的分析 JSON".to_string())?;
    Ok(ModelJson {
        conclusion: parsed.get("conclusion").and_then(Value::as_str).map(str::to_string),
        confidence: parsed.get("confidence").and_then(Value::as_str).map(str::to_string),
        citations: string_list(&parsed, "citations"),
        support: string_list(&parsed, "support"),
        against: string_list(&parsed, "against"),
        uncertainty: string_list(&parsed, "uncertainty"),
    })
}

fn string_list(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn range_start(range_key: &str) -> i64 {
    let now = Local::now();
    match range_key {
        "today" => {
            let naive = now.date_naive().and_hms_opt(0, 0, 0).unwrap_or_else(|| now.naive_local());
            Local
                .from_local_datetime(&naive)
                .single()
                .unwrap_or(now)
                .timestamp_millis()
        }
        "3d" => (now - ChronoDuration::days(3)).timestamp_millis(),
        _ => (now - ChronoDuration::days(7)).timestamp_millis(),
    }
}

fn format_iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|time| time.to_rfc3339())
        .unwrap_or_else(|| ms.to_string())
}

fn simple_hash(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_signal_labels_become_chinese() {
        assert_eq!(display_signal_label("reset_related"), "重置相关");
        assert_eq!(display_signal_label("reset_announcement"), "重置公告");
        assert_eq!(display_signal_label("RESET"), "重置相关");
        assert_eq!(display_signal_label("无重置信号"), "无重置信号");
        assert_eq!(display_signal_label("间接相关"), "间接相关");
    }

    #[test]
    fn glm_chat_uses_coding_plan_endpoint() {
        let urls = glm_chat_candidates(Some("https://open.bigmodel.cn"), false);
        assert_eq!(urls[0].0, "https://open.bigmodel.cn/api/coding/paas/v4/chat/completions");
        assert!(urls.iter().any(|(url, _)| url == "https://open.bigmodel.cn/api/paas/v4/chat/completions"));
        let intl = glm_chat_candidates(Some("https://api.z.ai"), true);
        assert_eq!(intl[0].0, "https://api.z.ai/api/coding/paas/v4/chat/completions");
    }

    #[test]
    fn user_prompt_is_trimmed_and_capped() {
        assert_eq!(sanitize_user_prompt("  dashboard  "), "dashboard");
        assert_eq!(sanitize_user_prompt("a\u{0000}b"), "ab");
        let long: String = "测".repeat(USER_PROMPT_MAX_CHARS + 8);
        assert_eq!(sanitize_user_prompt(&long).chars().count(), USER_PROMPT_MAX_CHARS);
        assert!(DEFAULT_USER_PROMPT.contains("Hold on to your Codex"));
        assert!(DEFAULT_USER_PROMPT.contains("仪表盘"));
    }
}
