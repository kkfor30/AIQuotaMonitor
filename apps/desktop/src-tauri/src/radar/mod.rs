//! GPT 重置雷达：Codex Radar 公开 feed 同步与可选 AI 分析。
//! 与平台额度域隔离，失败不改写 Source 聚合状态。

use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
use crate::storage::repository::{RadarAnalysisRecord, RadarCheckRecord, TiboPostRecord};
use crate::storage::vault;
use chrono::{Duration as ChronoDuration, Local, TimeZone};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub const FEED_URL: &str = "https://codex-reset.com/api/feed";
pub const PROMPT_VERSION: &str = "radar-v1";

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
    pub cut: Option<TiboPostView>,
}

#[derive(Debug, Deserialize)]
struct FeedFile {
    stale: Option<bool>,
    tweets: Option<Vec<FeedTweet>>,
}

#[derive(Debug, Deserialize)]
struct FeedTweet {
    id: String,
    url: Option<String>,
    text: Option<String>,
    at: Option<String>,
    kind: Option<String>,
    tibo_lane: Option<String>,
    explicit_reset_claim: Option<bool>,
    reset_verification_status: Option<String>,
    is_reply: Option<bool>,
    replies: Option<i64>,
    reposts: Option<i64>,
    likes: Option<i64>,
}

pub fn snapshot(database: &Database) -> Result<RadarSnapshot, String> {
    let posts = database
        .list_tibo_posts(80)?
        .into_iter()
        .map(to_view)
        .collect::<Vec<_>>();
    let latest = posts.first().cloned();
    let cut = posts.iter().find(|post| is_settled_reset_view(post)).cloned();
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
        cut,
    })
}

pub async fn run_check(
    database: &Database,
    coordinator: &RefreshCoordinator,
    analyze: bool,
    range_key: &str,
    source_id: Option<&str>,
    model: Option<&str>,
) -> Result<RadarSnapshot, String> {
    let started = epoch_ms();
    let id = format!("radar-{started}");
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|error| format!("初始化雷达客户端失败：{error}"))?;
    let (sync_status, parse_status, post_count, error_message) = match fetch_feed(&client).await {
        Ok((posts, stale)) => {
            let count = posts.len() as i64;
            database.replace_tibo_posts(&posts, started)?;
            (
                "success".into(),
                if stale { "stale".into() } else { "success".into() },
                count,
                None,
            )
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

async fn fetch_feed(client: &Client) -> Result<(Vec<TiboPostRecord>, bool), String> {
    let response = client
        .get(FEED_URL)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("无法连接 Codex Radar：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("Codex Radar 返回 HTTP {}", response.status().as_u16()));
    }
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取 Codex Radar 失败：{error}"))?;
    parse_feed(&body)
}

pub fn parse_feed(body: &str) -> Result<(Vec<TiboPostRecord>, bool), String> {
    let feed: FeedFile =
        serde_json::from_str(body).map_err(|_| "Codex Radar 返回格式无法解析".to_string())?;
    let stale = feed.stale.unwrap_or(false);
    let synced_at = epoch_ms();
    let mut posts = Vec::new();
    for tweet in feed.tweets.unwrap_or_default() {
        let text = tweet.text.unwrap_or_default().trim().to_string();
        if text.is_empty() {
            continue;
        }
        let posted_at = parse_time(tweet.at.as_deref()).unwrap_or(synced_at);
        posts.push(TiboPostRecord {
            id: tweet.id,
            url: tweet.url.unwrap_or_default(),
            text,
            posted_at,
            kind: tweet.kind.unwrap_or_else(|| "other".into()),
            tibo_lane: tweet.tibo_lane,
            explicit_reset: tweet.explicit_reset_claim.unwrap_or(false),
            verification_status: tweet.reset_verification_status,
            is_reply: tweet.is_reply.unwrap_or(false),
            replies: tweet.replies.unwrap_or(0),
            reposts: tweet.reposts.unwrap_or(0),
            likes: tweet.likes.unwrap_or(0),
            extra_json: "{}".into(),
            synced_at,
        });
    }
    if posts.is_empty() {
        return Err("Codex Radar 未返回可展示动态".into());
    }
    Ok((posts, stale))
}

fn parse_time(value: Option<&str>) -> Option<i64> {
    let value = value?;
    DateTimeParser::parse(value)
}

struct DateTimeParser;
impl DateTimeParser {
    fn parse(value: &str) -> Option<i64> {
        chrono::DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|time| time.timestamp_millis())
    }
}

fn to_view(post: TiboPostRecord) -> TiboPostView {
    let badge = badge_for(&post);
    let filter = filter_for(&post);
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
    }
}

fn badge_for(post: &TiboPostRecord) -> String {
    if post.explicit_reset {
        return "RESET".into();
    }
    match post.kind.as_str() {
        "banked" => "BANKED".into(),
        "limits" => "LIMITS".into(),
        "candidate" | "signal" => "VERIFYING".into(),
        _ => "NOTE".into(),
    }
}

fn filter_for(post: &TiboPostRecord) -> String {
    if post.kind == "limits" {
        "limits".into()
    } else if post.explicit_reset
        || matches!(post.kind.as_str(), "candidate" | "signal" | "banked")
        || post.tibo_lane.as_deref() == Some("reset_announcement")
    {
        "signal".into()
    } else {
        "other".into()
    }
}

fn is_settled_reset_view(post: &TiboPostView) -> bool {
    if !(post.explicit_reset || post.badge == "RESET") {
        return false;
    }
    let age = epoch_ms().saturating_sub(post.posted_at);
    age > 18 * 60 * 60 * 1000
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
        cut_label: record.cut_post_id.map(|id| format!("切点 {id}")),
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
        "glm-coding-plan" => Some(("GLM", "glm-4.5-flash")),
        "kimi-balance-api" => Some(("Kimi", "kimi-k2-turbo-preview")),
        "minimax-coding-plan" => Some(("MiniMax", "MiniMax-M2.5")),
        _ => None,
    }
}

fn chat_endpoint(source_id: &str, api_base: Option<&str>) -> Option<(String, bool)> {
    match source_id {
        "deepseek-balance-api" => Some((
            format!(
                "{}/v1/chat/completions",
                api_base.unwrap_or("https://api.deepseek.com").trim_end_matches('/')
            ),
            true,
        )),
        "glm-coding-plan" => Some((
            "https://open.bigmodel.cn/api/paas/v4/chat/completions".into(),
            true,
        )),
        "kimi-balance-api" => Some(("https://api.moonshot.cn/v1/chat/completions".into(), true)),
        "minimax-coding-plan" => Some((
            format!(
                "{}/v1/chat/completions",
                api_base.unwrap_or("https://api.minimaxi.com").trim_end_matches('/')
            ),
            true,
        )),
        _ => None,
    }
}

async fn run_analysis(
    database: &Database,
    coordinator: &RefreshCoordinator,
    range_key: &str,
    source_id: Option<&str>,
    model: Option<&str>,
) -> Result<(), String> {
    let posts = database.list_tibo_posts(80)?;
    let views = posts.iter().cloned().map(to_view).collect::<Vec<_>>();
    let cut = views.iter().find(|post| is_settled_reset_view(post)).cloned();
    let window_start = range_start(range_key);
    let selected: Vec<_> = views
        .iter()
        .filter(|post| post.posted_at >= window_start)
        .filter(|post| cut.as_ref().is_none_or(|cut| post.posted_at > cut.posted_at))
        .cloned()
        .collect();
    if selected.is_empty() {
        return Err("当前时间窗内没有新的 Tibo 动态可分析（可能都在上次已落地重置之前）".into());
    }
    let source_id = source_id
        .map(str::to_string)
        .or_else(|| chat_models(database).ok().and_then(|models| models.into_iter().find(|item| item.ready).map(|item| item.source_id)))
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
    let (url, bearer) = chat_endpoint(&source_id, api_base.as_deref()).ok_or_else(|| "该来源没有对话接口".to_string())?;
    let input = selected
        .iter()
        .map(|post| {
            format!(
                "{}\n{}\n{}",
                format_iso(post.posted_at),
                post.url,
                post.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let input_hash = format!("{:x}", simple_hash(&input));
    if let Some(previous) = database.latest_radar_analysis()? {
        if previous.input_hash == input_hash && previous.model.as_deref() == Some(model.as_str()) && previous.error_message.is_none()
        {
            return Ok(());
        }
    }
    let body = json!({
        "model": model,
        "temperature": 0.2,
        "messages": [
            {"role": "system", "content": "You analyze public Tibo/Codex reset posts. Reply with JSON only: {\"conclusion\":\"\",\"confidence\":\"low|medium|high\",\"citations\":[\"\"],\"support\":[\"\"],\"against\":[\"\"],\"uncertainty\":[\"\"]}. Do not treat older already-landed resets as current evidence. Chinese conclusion is allowed."},
            {"role": "user", "content": format!("Last settled reset is excluded. Analyze only these newer posts:\n{input}")}
        ]
    });
    let mut request = coordinator.client().post(&url).header("Content-Type", "application/json");
    request = if bearer {
        request.bearer_auth(&secret)
    } else {
        request.header("Authorization", &secret)
    };
    let response = request
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("分析请求失败：{error}"))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if status.as_u16() == 402 || text.contains("余额不足") || text.contains("额度不足") || text.contains("insufficient") {
        return Err("当前模型额度或余额不足，请更换模型后再分析".into());
    }
    if !status.is_success() {
        return Err(format!("分析接口失败（HTTP {}）", status.as_u16()));
    }
    let parsed = parse_model_json(&text)?;
    database.insert_radar_analysis(&RadarAnalysisRecord {
        id: format!("analysis-{}", epoch_ms()),
        created_at: epoch_ms(),
        range_key: range_key.into(),
        cut_post_id: cut.map(|post| post.id),
        from_posted_at: selected.last().map(|post| post.posted_at),
        to_posted_at: selected.first().map(|post| post.posted_at),
        source_id: Some(source_id),
        model: Some(model),
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
    fn parses_feed_tweets() {
        let body = r#"{"tweets":[{"id":"1","url":"https://x.com/t/1","text":"Hello reset","at":"2026-08-27T16:35:05.000Z","kind":"candidate","explicit_reset_claim":true,"likes":1}]}"#;
        let (posts, stale) = parse_feed(body).expect("feed");
        assert_eq!(posts[0].id, "1");
        assert!(posts[0].explicit_reset);
        assert!(!stale);
    }

    #[test]
    fn old_explicit_reset_is_settled() {
        let post = TiboPostView {
            id: "reset-1".into(),
            url: "https://x.com/t/1".into(),
            text: "reset landed".into(),
            posted_at: epoch_ms() - 19 * 60 * 60 * 1000,
            kind: "candidate".into(),
            badge: "RESET".into(),
            filter: "signal".into(),
            explicit_reset: true,
            is_reply: false,
            replies: 0,
            reposts: 0,
            likes: 0,
            synced_at: epoch_ms(),
        };
        assert!(is_settled_reset_view(&post));
        let recent = TiboPostView {
            posted_at: epoch_ms() - 60 * 60 * 1000,
            ..post
        };
        assert!(!is_settled_reset_view(&recent));
    }
}
