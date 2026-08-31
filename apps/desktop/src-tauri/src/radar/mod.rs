//! GPT 重置雷达：从 CodexRadar 公开首页同步 Tibo 动态，并保留可选 AI 分析。
//! 与平台额度域隔离，失败不改写 Source 聚合状态。
//! 三路证据并行：CodexRadar 来源判断 / AI 增量分析 / 本机额度观察（quota_watch）。

mod codexradar;
mod quota_watch;
mod time_claims;

use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
use crate::storage::repository::{
    RadarAnalysisRecord, RadarCheckRecord, RadarEventRecord, RadarTimeClaimRecord, SourceRecord,
    TiboPostRecord,
};
use crate::storage::vault;
use chrono::{Duration as ChronoDuration, Local, NaiveDate, TimeZone};
use quota_watch::QuotaVerificationView;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::error::Error;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;

pub const FEED_URL: &str = "https://codexradar.com/";

/// 检查取消控制：cancel() 使代号 +1 并唤醒等待者；
/// 运行中的检查在 await 点（抓取/模型请求）被丢弃，不写入检查与分析记录。
#[derive(Default)]
pub struct RadarControl {
    pub(crate) generation: AtomicU64,
    notify: Notify,
    /// 全局「检查进行中」标志：跨窗口共享 loading 态；同时拒绝并发检查。
    running: std::sync::atomic::AtomicBool,
}

impl RadarControl {
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.notify.notify_waiters();
    }

    /// 尝试占用运行槽：已在检查中返回 false（拒绝并发）。
    pub(crate) fn try_begin(&self) -> bool {
        self.running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn finish(&self) {
        self.running.store(false, Ordering::Release);
    }

    /// 等待本代检查被取消；代号已变立即返回（覆盖取消发生在注册前的竞态）。
    pub(crate) async fn wait_cancelled(&self, my_generation: u64) {
        loop {
            if self.generation.load(Ordering::Relaxed) != my_generation {
                return;
            }
            self.notify.notified().await;
        }
    }
}
pub const PROMPT_VERSION: &str = "radar-v11";
pub const USER_PROMPT_MAX_CHARS: usize = 4000;
pub const DEFAULT_USER_PROMPT: &str = "若帖子提到仪表盘（dashboard）、里程碑（milestone）、庆祝（celebration）、倒计时，或出现 “Hold on to your Codex” / “抓紧你的 Codex” / “reset will land” 等措辞，视为即将重置的强信号（signal_level=strong），即使没有给出确切时间。
已落地的历史重置只作背景，不能当成否定新一轮重置的证据；普通闲聊回帖应判 none/no_change，不得推进或关闭当前事件。
没有重置相关内容，或只有旧重置而没有新信号时，才使用低把握度。";

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
    pub analysis_basis: Option<String>,
    pub confidence: Option<String>,
    pub citations: Vec<String>,
    pub support: Vec<String>,
    pub against: Vec<String>,
    pub uncertainty: Vec<String>,
    pub error_message: Option<String>,
    pub covers_latest: bool,
    pub event_id: Option<String>,
    /// delta | rebuild
    pub analysis_mode: Option<String>,
    /// new_event | same_event | none
    pub event_relation: Option<String>,
    /// watching | upcoming | landed_claimed | landed_observed | closed
    pub event_phase: Option<String>,
    /// reinforce | no_change | weaken | advance_phase | cancel | new_event
    pub delta_effect: Option<String>,
    /// none | weak | strong
    pub signal_level: Option<String>,
    /// complete | context_missing | conflicting
    pub context_status: Option<String>,
    /// before_expected | expected_time_passed | claimed_landed | observed_landed | historical | timeless
    pub temporal_phase: Option<String>,
    pub valid_until: Option<i64>,
    pub time_claims: Vec<RadarTimeClaimView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarTimeClaimView {
    pub post_id: String,
    pub raw_text: String,
    pub parse_status: String,
    pub timezone_kind: Option<String>,
    pub timezone_assumed: bool,
    pub resolved_at: Option<i64>,
    pub precision: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarSourceAssessmentView {
    pub headline: Option<String>,
    pub lead: Option<String>,
    pub last_synced_at: Option<i64>,
    /// fresh | stale | missing
    pub freshness: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarEventNodeView {
    pub at: i64,
    /// signal | claimed | observed | closed
    pub kind: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarEventView {
    pub id: String,
    /// watching | upcoming | landed_claimed | landed_observed | closed
    pub phase: String,
    pub title: String,
    pub summary: Option<String>,
    pub first_signal_at: i64,
    pub latest_evidence_at: i64,
    pub claimed_landed_at: Option<i64>,
    pub observed_reset_at: Option<i64>,
    pub closed_at: Option<i64>,
    pub expected_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub state_revision: i64,
    pub temporal_status: String,
    pub timeline: Vec<RadarEventNodeView>,
    pub post_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarAiAssessmentView {
    pub enabled: bool,
    /// current | disabled | pending | failed | not_analyzed
    pub state: String,
    /// 当前事件匹配的最新成功分析。
    pub current: Option<RadarAnalysisView>,
    /// 最近一次成功分析（历史）。
    pub history: Option<RadarAnalysisView>,
    /// 最近一次失败检查的分析错误。
    pub latest_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarModelOption {
    pub source_id: String,
    pub platform_id: String,
    pub display_name: String,
    pub model: String,
    pub ready: bool,
    /// 用户通过「验证连接」添加的自定义模型；默认模型为 false。
    pub custom: bool,
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
    pub source_assessment: RadarSourceAssessmentView,
    pub event: Option<RadarEventView>,
    pub ai_assessment: RadarAiAssessmentView,
    pub quota_verifications: Vec<QuotaVerificationView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarAnalysisPrefs {
    pub analyze: bool,
    pub range_key: String,
    pub source_id: Option<String>,
    pub model: Option<String>,
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
    let analysis = database.latest_radar_analysis()?.map(|record| {
        let covers_latest = match record.to_posted_at {
            Some(to) => posts.first().map_or(true, |post| post.posted_at <= to),
            None => true,
        };
        analysis_view(database, record, covers_latest)
    });
    let notice = load_notice(database)?;
    let source_assessment = RadarSourceAssessmentView {
        headline: notice.as_ref().map(|item| item.headline.clone()),
        lead: notice.as_ref().and_then(|item| item.lead.clone()),
        last_synced_at,
        freshness: source_status.to_string(),
    };
    let event_record = database.active_radar_event()?;
    let quota_verifications =
        quota_watch::assess_quota_verifications(database, event_record.as_ref())?;
    let event_view_data = event_record
        .as_ref()
        .map(|record| event_view(database, record));
    let ai_assessment =
        build_ai_assessment(database, event_record.as_ref(), analysis.as_ref(), &checks);
    Ok(RadarSnapshot {
        source_status: source_status.into(),
        last_synced_at,
        posts,
        latest,
        checks,
        analysis,
        models: chat_models(database)?,
        analysis_prefs: load_analysis_prefs(database)?,
        notice,
        source_assessment,
        event: event_view_data,
        ai_assessment,
        quota_verifications,
    })
}

pub fn reconcile_event_state(database: &Database, now: i64) -> Result<(), String> {
    refresh_time_claims(database)?;
    let initial = database.active_radar_event()?;
    quota_watch::materialize_observations(database, initial.as_ref())?;
    let Some(mut event) = database.active_radar_event()? else {
        return Ok(());
    };
    let original = event.clone();
    let post_ids = database.radar_event_post_ids(&event.id)?;
    let claims = database.radar_time_claims_for_posts(&post_ids)?;
    event.expected_at = claims.iter().filter_map(|claim| claim.resolved_at).max();
    let verifications = quota_watch::assess_quota_verifications(database, Some(&event))?;
    if matches!(
        event.phase.as_str(),
        "watching" | "upcoming" | "landed_claimed"
    ) {
        if let Some(observed_at) = verifications
            .iter()
            .filter_map(|value| value.last_reset_observed_at)
            .min()
        {
            event.phase = "landed_observed".into();
            event.observed_reset_at = Some(observed_at);
            event.title = "本机已观察到额度刷新".into();
        }
    }
    event.expires_at = event_expiry(&event);
    if event.expires_at.is_some_and(|expires| now >= expires) {
        event.closed_at = Some(now);
        event.close_reason = Some(
            match event.phase.as_str() {
                "watching" => "timeout_no_signal",
                "upcoming" => "timeout_unverified",
                "landed_claimed" => "claimed_unverified",
                "landed_observed" => "completed",
                _ => "timeout",
            }
            .into(),
        );
        event.phase = "closed".into();
    }
    if event.phase != original.phase
        || event.expected_at != original.expected_at
        || event.expires_at != original.expires_at
        || event.observed_reset_at != original.observed_reset_at
        || event.closed_at != original.closed_at
    {
        event.state_revision = original.state_revision + 1;
        database.update_radar_event(&event)?;
    }
    Ok(())
}

pub fn reconcile_event_state_now(database: &Database) -> Result<(), String> {
    reconcile_event_state(database, epoch_ms())
}

fn refresh_time_claims(database: &Database) -> Result<(), String> {
    for post in database.list_tibo_posts(80)? {
        let parsed = time_claims::parse_post_time_claims(&post.id, &post.text, post.posted_at);
        let records = parsed
            .into_iter()
            .map(|claim| RadarTimeClaimRecord {
                post_id: claim.post_id,
                raw_text: claim.raw_text,
                clock_hour: claim.clock_hour.map(i64::from),
                clock_minute: claim.clock_minute.map(i64::from),
                date_relation: claim.date_relation,
                timezone_kind: claim.timezone_kind,
                timezone_assumed: claim.timezone_assumed,
                parse_status: claim.parse_status,
                resolved_at: claim.resolved_at,
                precision: claim.precision,
                parser_version: time_claims::TIMEZONE_POLICY_VERSION.into(),
            })
            .collect::<Vec<_>>();
        database.replace_radar_time_claims(&post.id, &records)?;
    }
    Ok(())
}

fn event_expiry(event: &RadarEventRecord) -> Option<i64> {
    let hour = 3_600_000;
    match event.phase.as_str() {
        "watching" => Some(event.latest_evidence_at + 48 * hour),
        "upcoming" => Some(
            event
                .expected_at
                .map_or(event.latest_evidence_at + 72 * hour, |at| at + 24 * hour),
        ),
        "landed_claimed" => {
            Some(event.claimed_landed_at.unwrap_or(event.latest_evidence_at) + 48 * hour)
        }
        "landed_observed" => {
            Some(event.observed_reset_at.unwrap_or(event.latest_evidence_at) + 24 * hour)
        }
        _ => event.expires_at,
    }
}

fn event_temporal_phase(event: &RadarEventRecord, now: i64) -> String {
    if event.closed_at.is_some() || event.phase == "closed" {
        return "historical".into();
    }
    if event.phase == "landed_observed" {
        return "observed_landed".into();
    }
    if event.phase == "landed_claimed" {
        return "claimed_landed".into();
    }
    match event.expected_at {
        Some(at) if now >= at => "expected_time_passed".into(),
        Some(_) => "before_expected".into(),
        None => "timeless".into(),
    }
}

/// 事件视图：从事件记录推导时间线节点，并挂上事件关联原帖。
fn event_view(database: &Database, record: &RadarEventRecord) -> RadarEventView {
    let mut timeline = vec![RadarEventNodeView {
        at: record.first_signal_at,
        kind: "signal".into(),
        label: "首次信号".into(),
    }];
    if let Some(at) = record.claimed_landed_at {
        timeline.push(RadarEventNodeView {
            at,
            kind: "claimed".into(),
            label: "来源称已落地".into(),
        });
    }
    if let Some(at) = record.observed_reset_at {
        timeline.push(RadarEventNodeView {
            at,
            kind: "observed".into(),
            label: "本机观察到刷新".into(),
        });
    }
    if let Some(at) = record.closed_at {
        timeline.push(RadarEventNodeView {
            at,
            kind: "closed".into(),
            label: record
                .close_reason
                .clone()
                .unwrap_or_else(|| "事件关闭".into()),
        });
    }
    let post_ids = database
        .radar_event_post_ids(&record.id)
        .unwrap_or_default();
    RadarEventView {
        id: record.id.clone(),
        phase: record.phase.clone(),
        title: record.title.clone(),
        summary: record.summary.clone(),
        first_signal_at: record.first_signal_at,
        latest_evidence_at: record.latest_evidence_at,
        claimed_landed_at: record.claimed_landed_at,
        observed_reset_at: record.observed_reset_at,
        closed_at: record.closed_at,
        expected_at: record.expected_at,
        expires_at: record.expires_at,
        state_revision: record.state_revision,
        temporal_status: event_temporal_phase(record, epoch_ms()),
        timeline,
        post_ids,
    }
}

/// AI 评估：当前事件匹配分析 / 历史成功分析 / 最近失败，派生 UI 状态。
fn build_ai_assessment(
    database: &Database,
    event: Option<&RadarEventRecord>,
    latest_success: Option<&RadarAnalysisView>,
    checks: &[RadarCheckView],
) -> RadarAiAssessmentView {
    let enabled = load_analysis_prefs(database)
        .map(|prefs| prefs.analyze)
        .unwrap_or(false);
    let newest_post_at = database
        .list_tibo_posts(1)
        .ok()
        .and_then(|posts| posts.first().map(|post| post.posted_at));
    let covers = |to_posted_at: Option<i64>| match (to_posted_at, newest_post_at) {
        (Some(covered_to), Some(newest)) => newest <= covered_to,
        _ => true,
    };
    let current = event
        .and_then(|record| database.latest_event_radar_analysis(&record.id).ok())
        .flatten()
        .map(|record| {
            let covered_to = record.to_posted_at;
            analysis_view(database, record, covers(covered_to))
        });
    let latest_error = checks
        .iter()
        .find(|check| check.analyze_status.as_deref() == Some("failed"))
        .and_then(|check| check.error_message.clone());
    let state = if !enabled {
        "disabled"
    } else if current.is_some() {
        let covered_to = database
            .latest_event_radar_analysis(event.expect("current implies event").id.as_str())
            .ok()
            .flatten()
            .and_then(|record| record.to_posted_at);
        match newest_post_at {
            Some(posted_at) if posted_at > covered_to.unwrap_or(0) => "pending",
            _ => "current",
        }
    } else if latest_error.is_some() {
        "failed"
    } else {
        "not_analyzed"
    };
    RadarAiAssessmentView {
        enabled,
        state: state.into(),
        current,
        history: latest_success.cloned(),
        latest_error,
    }
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
    let _ = save_analysis_prefs(database, analyze, range_key, source_id, model, user_prompt);
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

    // 提示词与缓存键组装前先把时间声明、额度观察和事件超时协调到最新状态。
    reconcile_event_state(database, epoch_ms())?;

    let mut analyze_status = if analyze {
        "skipped".to_string()
    } else {
        "off".into()
    };
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
        status: if failed {
            "failed".into()
        } else {
            "success".into()
        },
        sync_status: Some(sync_status),
        parse_status: Some(parse_status),
        analyze_status: Some(analyze_status),
        error_message: error_message.or(analyze_error),
        post_count,
    })?;
    reconcile_event_state(database, epoch_ms())?;
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
    let source_label = chat_target(&target.adapter_id)
        .map(|(display, _)| format!("{display} · {}", target.model))
        .unwrap_or_else(|| target.model.clone());
    database.update_tibo_translation(post_id, &translated, epoch_ms(), &source_label)?;
    snapshot(database)
}

/// 可用对话模型解析：优先用户指定来源，否则取第一个已就绪的 API Key 来源。
struct ChatTarget {
    source_id: String,
    adapter_id: String,
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
            chat_models(database).ok().and_then(|models| {
                models
                    .into_iter()
                    .find(|item| item.ready)
                    .map(|item| item.source_id)
            })
        })
        .ok_or_else(|| {
            "请先接入可用于对话的 API Key（DeepSeek / GLM / Kimi 开放平台 / MiniMax）".to_string()
        })?;
    let source = database.source(&source_id)?;
    let secret = source
        .secret_ref
        .as_deref()
        .and_then(|reference| vault::get(reference).ok().flatten())
        .ok_or_else(|| "所选模型没有可用凭据".to_string())?;
    let model = model
        .map(str::to_string)
        .or_else(|| chat_target(&source.adapter_id).map(|(_, model)| model.to_string()))
        .ok_or_else(|| "该来源不支持对话分析".to_string())?;
    let api_base = database
        .user_platform(&source.platform_id)?
        .and_then(|item| item.api_base_url);
    if chat_endpoint_candidates(&source.adapter_id, api_base.as_deref()).is_empty() {
        return Err("该来源没有对话接口".into());
    }
    Ok(ChatTarget {
        source_id,
        adapter_id: source.adapter_id,
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
        return Err(format!(
            "CodexRadar 返回 HTTP {}",
            response.status().as_u16()
        ));
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
    let normalized = raw
        .trim()
        .replace('-', "_")
        .replace(' ', "_")
        .to_ascii_lowercase();
    match normalized.as_str() {
        "reset_related" | "related" | "direct" | "signal" | "reset" | "verifying" => {
            "重置相关".into()
        }
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
            || matches!(
                post.kind.as_str(),
                "candidate" | "signal" | "banked" | "direct" | "reset"
            )
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

fn analysis_view(
    database: &Database,
    record: RadarAnalysisRecord,
    covers_latest: bool,
) -> RadarAnalysisView {
    let time_claims = record
        .event_id
        .as_deref()
        .and_then(|event_id| database.radar_event_post_ids(event_id).ok())
        .and_then(|ids| database.radar_time_claims_for_posts(&ids).ok())
        .unwrap_or_default()
        .into_iter()
        .map(time_claim_view)
        .collect();
    RadarAnalysisView {
        id: record.id,
        created_at: record.created_at,
        range_key: record.range_key,
        cut_post_id: record.cut_post_id.clone(),
        cut_label: None,
        source_id: record.source_id,
        model: record.model,
        conclusion: record.conclusion,
        analysis_basis: record.analysis_basis,
        confidence: record.confidence,
        citations: json_list(&record.citations_json),
        support: json_list(&record.support_json),
        against: json_list(&record.against_json),
        uncertainty: json_list(&record.uncertainty_json),
        error_message: record.error_message,
        covers_latest,
        event_id: record.event_id,
        analysis_mode: record.analysis_mode,
        event_relation: record.event_relation,
        event_phase: record.event_phase,
        delta_effect: record.delta_effect,
        signal_level: record.signal_level,
        context_status: record.context_status,
        temporal_phase: record.temporal_phase,
        valid_until: record.valid_until,
        time_claims,
    }
}

fn time_claim_view(record: RadarTimeClaimRecord) -> RadarTimeClaimView {
    RadarTimeClaimView {
        post_id: record.post_id,
        raw_text: record.raw_text,
        parse_status: record.parse_status,
        timezone_kind: record.timezone_kind,
        timezone_assumed: record.timezone_assumed,
        resolved_at: record.resolved_at,
        precision: record.precision,
    }
}

fn json_list(value: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(value).unwrap_or_default()
}

fn chat_models(database: &Database) -> Result<Vec<RadarModelOption>, String> {
    let custom_models = database.list_radar_custom_models()?;
    let mut options = Vec::new();
    for platform in database.list_user_platforms()? {
        for source in database.list_sources(&platform.platform_id)? {
            if source.source_type != "api_key" {
                continue;
            }
            let ready = source_ready(&source);
            let display = chat_target(&source.adapter_id).map(|(name, _)| name);
            match chat_target(&source.adapter_id) {
                Some((_, model)) => {
                    options.push(RadarModelOption {
                        source_id: source.id.clone(),
                        platform_id: platform.platform_id.clone(),
                        display_name: display_name_for(&source, display.unwrap_or("对话来源")),
                        model: model.into(),
                        ready,
                        custom: false,
                    });
                }
                None => {
                    // 支持对话端点但无可靠默认模型（如 Kimi：平台模型名变动频繁）：
                    // 发布空 model 占位，自定义模型面板可选该来源；分析模型下拉会过滤空项。
                    if !chat_endpoint_candidates(&source.adapter_id, None).is_empty() {
                        options.push(RadarModelOption {
                            source_id: source.id.clone(),
                            platform_id: platform.platform_id.clone(),
                            display_name: platform.display_name.clone(),
                            model: String::new(),
                            ready,
                            custom: false,
                        });
                    }
                }
            }
            for custom in custom_models
                .iter()
                .filter(|item| item.source_id == source.id)
            {
                options.push(RadarModelOption {
                    source_id: source.id.clone(),
                    platform_id: platform.platform_id.clone(),
                    display_name: display_name_for(&source, display.unwrap_or("对话来源")),
                    model: custom.model.clone(),
                    ready,
                    custom: true,
                });
            }
        }
    }
    Ok(options)
}

fn source_ready(source: &SourceRecord) -> bool {
    source
        .secret_ref
        .as_deref()
        .and_then(|reference| vault::get(reference).ok().flatten())
        .is_some()
        && source.state != "error"
}

fn display_name_for(source: &SourceRecord, display: &str) -> String {
    if source.account_kind == "additional" {
        format!("{display} · {}", source.account_name)
    } else {
        display.into()
    }
}

fn chat_target(source_id: &str) -> Option<(&'static str, &'static str)> {
    match source_id {
        "deepseek-balance-api" => Some(("DeepSeek", "deepseek-chat")),
        "glm-coding-plan" => Some(("GLM 国内", "glm-4.5-flash")),
        "glm-intl-coding-plan" => Some(("GLM 国际", "glm-4.5-flash")),
        // Kimi：moonshot.cn 域上 kimi-latest/kimi-k2-turbo-preview 均不存在（404），
        // 平台模型名变动频繁，默认不进模型列表，需要时用自定义模型填账户实际可用的名字。
        "kimi-balance-api" => None,
        "kimi-coding-plan" => None,
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
    if base.ends_with("/api/coding/paas/v4")
        || base.ends_with("/api/paas/v4")
        || base.ends_with("/v1")
    {
        return format!("{base}/chat/completions");
    }
    format!("{base}{path}")
}

fn chat_endpoint_candidates(source_id: &str, api_base: Option<&str>) -> Vec<(String, bool)> {
    match source_id {
        "deepseek-balance-api" => vec![(
            join_chat_url(
                &trim_api_base(api_base, "https://api.deepseek.com"),
                "/v1/chat/completions",
            ),
            true,
        )],
        "glm-coding-plan" => glm_chat_candidates(api_base, false),
        "glm-intl-coding-plan" => glm_chat_candidates(api_base, true),
        "kimi-balance-api" => vec![("https://api.moonshot.cn/v1/chat/completions".into(), true)],
        "kimi-coding-plan" => vec![(
            join_chat_url(
                &trim_api_base(api_base, "https://api.kimi.com/coding"),
                "/v1/chat/completions",
            ),
            true,
        )],
        "minimax-coding-plan" => vec![(
            join_chat_url(
                &trim_api_base(api_base, "https://api.minimaxi.com"),
                "/v1/chat/completions",
            ),
            true,
        )],
        "minimax-intl-coding-plan" => vec![(
            join_chat_url(
                &trim_api_base(api_base, "https://api.minimax.io"),
                "/v1/chat/completions",
            ),
            true,
        )],
        _ => Vec::new(),
    }
}

fn glm_chat_candidates(api_base: Option<&str>, intl: bool) -> Vec<(String, bool)> {
    let default = if intl {
        "https://api.z.ai"
    } else {
        "https://open.bigmodel.cn"
    };
    let base = trim_api_base(api_base, default);
    let coding = join_chat_url(&base, "/api/coding/paas/v4/chat/completions");
    let paas = join_chat_url(&base, "/api/paas/v4/chat/completions");
    let mut urls = vec![(coding.clone(), true)];
    if paas != coding {
        urls.push((paas, true));
    }
    urls
}

/// 本轮分析的新增/上下文分组：用户所选范围是「本次新增」，
/// 当前活动事件已关联的旧原帖是「事件上下文」，两组输入在提示词里明确分隔。
struct DeltaInputs {
    range_key: String,
    /// delta | rebuild
    mode: &'static str,
    delta: Vec<TiboPostView>,
    context: Vec<TiboPostView>,
    /// 活动事件状态摘要（阶段/首信号/本机是否已观察到重置），随输入发给模型。
    event_status: Option<String>,
    time_claims: Vec<RadarTimeClaimRecord>,
    temporal_phase: String,
    valid_until: Option<i64>,
    state_revision: i64,
}

fn delta_post_block(post: &TiboPostView, claims: &[RadarTimeClaimRecord]) -> String {
    let post_claims = claims
        .iter()
        .filter(|claim| claim.post_id == post.id)
        .map(|claim| {
            json!({
                "raw": claim.raw_text,
                "status": claim.parse_status,
                "timezone": claim.timezone_kind,
                "timezone_assumed": claim.timezone_assumed,
                "resolved_beijing_at": claim.resolved_at.map(format_iso),
                "precision": claim.precision,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "post_id": post.id,
        "published_beijing_at": format_iso(post.posted_at),
        "url": post.url,
        "text": post.text,
        "time_claims": post_claims,
    })
    .to_string()
}

fn collect_delta_inputs(database: &Database, range_key: &str) -> Result<DeltaInputs, String> {
    let views = database
        .list_tibo_posts(80)?
        .into_iter()
        .map(to_view)
        .collect::<Vec<_>>();
    let (window_start, window_end) = range_bounds(range_key);
    let delta: Vec<_> = views
        .iter()
        .filter(|post| post.posted_at >= window_start && post.posted_at <= window_end)
        .cloned()
        .collect();
    if delta.is_empty() {
        return Err("当前时间窗内没有 Tibo 动态可分析".into());
    }
    let event = database.active_radar_event()?;
    let delta_ids: std::collections::HashSet<&str> =
        delta.iter().map(|post| post.id.as_str()).collect();
    let context: Vec<TiboPostView> = match &event {
        Some(record) => database
            .radar_event_post_ids(&record.id)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| views.iter().find(|post| post.id == id).cloned())
            .filter(|post| !delta_ids.contains(post.id.as_str()))
            .take(12)
            .collect(),
        None => Vec::new(),
    };
    let closed_ids = database
        .closed_radar_event_post_ids()?
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let mode = if event.is_none() && delta.iter().all(|post| closed_ids.contains(&post.id)) {
        "historical_replay"
    } else {
        "live_delta"
    };
    let post_ids = delta
        .iter()
        .chain(context.iter())
        .map(|post| post.id.clone())
        .collect::<Vec<_>>();
    let claims = database.radar_time_claims_for_posts(&post_ids)?;
    let now = epoch_ms();
    let temporal_phase = event.as_ref().map_or_else(
        || {
            if mode == "historical_replay" {
                "historical".into()
            } else {
                "timeless".into()
            }
        },
        |record| event_temporal_phase(record, now),
    );
    let event_status = event.as_ref().map(|record| {
        json!({
            "event_id": record.id,
            "state_revision": record.state_revision,
            "phase": record.phase,
            "temporal_phase": temporal_phase,
            "first_signal_at": format_iso(record.first_signal_at),
            "latest_evidence_at": format_iso(record.latest_evidence_at),
            "claimed_landed_at": record.claimed_landed_at.map(format_iso),
            "local_quota_reset_observed_at": record.observed_reset_at.map(format_iso),
            "expected_at": record.expected_at.map(format_iso),
            "verification_availability": "derived_by_rust",
        })
        .to_string()
    });
    let valid_until = event
        .as_ref()
        .and_then(|record| {
            if temporal_phase == "before_expected" {
                record.expected_at
            } else if temporal_phase == "expected_time_passed" {
                Some(now + 6 * 3_600_000)
            } else {
                None
            }
        })
        .or_else(|| {
            claims
                .iter()
                .any(|claim| claim.parse_status == "ambiguous")
                .then_some(now + 6 * 3_600_000)
        });
    Ok(DeltaInputs {
        range_key: range_key.into(),
        mode,
        delta,
        context,
        event_status,
        time_claims: claims,
        temporal_phase,
        valid_until,
        state_revision: event.as_ref().map_or(0, |record| record.state_revision),
    })
}

async fn run_analysis(
    database: &Database,
    coordinator: &RefreshCoordinator,
    range_key: &str,
    source_id: Option<&str>,
    model: Option<&str>,
) -> Result<(), String> {
    let inputs = collect_delta_inputs(database, range_key)?;
    let user_prompt = load_analysis_prefs(database)?.user_prompt;
    let joined = |posts: &[TiboPostView]| {
        posts
            .iter()
            .map(|post| delta_post_block(post, &inputs.time_claims))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let input_hash = format!("{:x}", simple_hash(&joined(&inputs.delta)));
    // 事件状态参与复用键：阶段/本机观察变化后，旧的结论不应被复用。
    let context_hash = format!(
        "{:x}",
        simple_hash(&format!(
            "{}|{}|{}|{}|{}",
            joined(&inputs.context),
            inputs.event_status.as_deref().unwrap_or(""),
            inputs.mode,
            inputs.temporal_phase,
            time_claims::TIMEZONE_POLICY_VERSION,
        ))
    );
    let prompt_hash = format!("{:x}", simple_hash(&user_prompt));
    let target = resolve_chat_target(database, source_id, model)?;
    if database
        .find_reusable_radar_analysis(
            &input_hash,
            &context_hash,
            &prompt_hash,
            Some(&target.source_id),
            Some(&target.model),
            PROMPT_VERSION,
            &inputs.temporal_phase,
            inputs.state_revision,
            time_claims::TIMEZONE_POLICY_VERSION,
            epoch_ms(),
        )?
        .is_some()
    {
        // 新增输入、事件上下文、提示词与模型完全一致：复用成功结果，不再调用模型，也不再推进事件。
        return Ok(());
    }
    match run_analysis_inner(
        database,
        coordinator.client(),
        &inputs,
        &target,
        &user_prompt,
        &input_hash,
        &context_hash,
        &prompt_hash,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = persist_failed_analysis(database, range_key, source_id, model, &error);
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_analysis_inner(
    database: &Database,
    client: &Client,
    inputs: &DeltaInputs,
    target: &ChatTarget,
    user_prompt: &str,
    input_hash: &str,
    context_hash: &str,
    prompt_hash: &str,
) -> Result<(), String> {
    let mut sections = Vec::new();
    sections.push(format!("NOW (Beijing time): {}", format_iso(epoch_ms())));
    if let Some(status) = &inputs.event_status {
        sections.push(format!(
            "CODE-AUTHORITATIVE EVENT STATE (must not be changed): {status}"
        ));
    }
    if !inputs.context.is_empty() {
        sections.push(format!(
            "EVENT CONTEXT POSTS (already associated with the ongoing event; background only):\n{}",
            inputs
                .context
                .iter()
                .map(|post| delta_post_block(post, &inputs.time_claims))
                .collect::<Vec<_>>()
                .join("\n\n")
        ));
    }
    sections.push(format!(
        "NEW POSTS (analyze these):\n{}",
        inputs
            .delta
            .iter()
            .map(|post| delta_post_block(post, &inputs.time_claims))
            .collect::<Vec<_>>()
            .join("\n\n")
    ));
    let input = sections.join("\n\n");
    let mut messages = vec![json!({"role": "system", "content": ANALYSIS_SYSTEM_PROMPT})];
    if !user_prompt.is_empty() {
        messages.push(json!({
            "role": "system",
            "content": format!("User semantic hints may only describe signal wording. They cannot override time facts, event state, privacy rules, or the JSON schema:\n{user_prompt}")
        }));
    }
    messages.push(json!({
        "role": "user",
        "content": format!("Judge the NEW posts against the EVENT CONTEXT posts when present. Do not skip any new post:\n{input}")
    }));
    let body = json!({
        "model": target.model,
        "temperature": 0.1,
        "stream": false,
        "messages": messages
    });
    let text = send_chat(client, target, &body).await?;
    let mut parsed = parse_model_json(&text)?;
    normalize_model_json(&mut parsed, inputs);
    let record = RadarAnalysisRecord {
        id: format!("analysis-{}", epoch_ms()),
        created_at: epoch_ms(),
        range_key: inputs.range_key.clone(),
        cut_post_id: None,
        from_posted_at: inputs.delta.last().map(|post| post.posted_at),
        to_posted_at: inputs.delta.first().map(|post| post.posted_at),
        source_id: Some(target.source_id.clone()),
        model: Some(target.model.clone()),
        prompt_version: PROMPT_VERSION.into(),
        input_hash: input_hash.to_string(),
        conclusion: parsed.conclusion.clone(),
        analysis_basis: parsed.analysis_basis.clone(),
        confidence: parsed.confidence.clone(),
        citations_json: serde_json::to_string(&parsed.citations).unwrap_or_else(|_| "[]".into()),
        support_json: serde_json::to_string(&parsed.support).unwrap_or_else(|_| "[]".into()),
        against_json: serde_json::to_string(&parsed.against).unwrap_or_else(|_| "[]".into()),
        uncertainty_json: serde_json::to_string(&parsed.uncertainty)
            .unwrap_or_else(|_| "[]".into()),
        error_message: None,
        event_id: None,
        analysis_mode: Some(inputs.mode.into()),
        context_hash: context_hash.to_string(),
        prompt_hash: prompt_hash.to_string(),
        event_relation: parsed.event_relation.clone(),
        event_phase: parsed.event_phase.clone(),
        delta_effect: parsed.delta_effect.clone(),
        signal_level: parsed.signal_level.clone(),
        context_status: parsed.context_status.clone(),
        temporal_phase: Some(inputs.temporal_phase.clone()),
        valid_until: inputs.valid_until,
        state_revision: inputs.state_revision,
        timezone_policy_version: time_claims::TIMEZONE_POLICY_VERSION.into(),
    };
    // 事件推进先于落库，得到的 event_id 一并写入分析记录。
    let event_id = apply_analysis_to_event(database, inputs, &parsed, &record.id)?;
    let record = RadarAnalysisRecord { event_id, ..record };
    database.insert_radar_analysis(&record)?;
    Ok(())
}

/// 把 AI 的结构化增量输出套到当前事件上，返回本条分析应关联的事件 id。
/// 普通无关帖子（none / no_change）不推进、不关闭事件。
fn apply_analysis_to_event(
    database: &Database,
    inputs: &DeltaInputs,
    parsed: &ModelJson,
    analysis_id: &str,
) -> Result<Option<String>, String> {
    if inputs.mode == "historical_replay" {
        return Ok(None);
    }
    let newest_posted_at = inputs
        .delta
        .iter()
        .map(|post| post.posted_at)
        .max()
        .unwrap_or(0);
    let meaningful = parsed.signal_level.as_deref() != Some("none");
    match parsed.event_relation.as_deref() {
        Some("new_event") => {
            if let Some(active) = database.active_radar_event()? {
                if !meaningful || newest_posted_at <= active.latest_evidence_at {
                    return Ok(Some(active.id));
                }
                let mut closed = active;
                closed.phase = "closed".into();
                closed.closed_at = Some(epoch_ms());
                closed.close_reason = Some("出现新一轮重置信号".into());
                closed.state_revision += 1;
                database.update_radar_event(&closed)?;
            }
            if !meaningful {
                return Ok(None);
            }
            let event_id = create_radar_event(database, inputs, parsed, analysis_id)?;
            Ok(Some(event_id))
        }
        Some("same_event") => {
            let Some(active) = database.active_radar_event()? else {
                let event_id = create_radar_event(database, inputs, parsed, analysis_id)?;
                return Ok(Some(event_id));
            };
            let mut updated = active;
            let has_new_evidence = newest_posted_at > updated.latest_evidence_at;
            let old_phase = updated.phase.clone();
            // 阶段单调向前：模型输出不得把 landed_observed 拉回早期阶段。
            match parsed.delta_effect.as_deref() {
                Some("cancel") if has_new_evidence && meaningful => {
                    updated.phase = "closed".into();
                    updated.closed_at = Some(epoch_ms());
                    updated.close_reason = parsed
                        .conclusion
                        .clone()
                        .or_else(|| Some("信号取消或失效".into()));
                }
                Some("advance_phase") if has_new_evidence => {
                    if let Some(phase) = ai_settable_phase(parsed.event_phase.as_deref()) {
                        if phase_rank(phase) > phase_rank(&updated.phase) {
                            updated.phase = phase.into();
                            if phase == "landed_claimed" && updated.claimed_landed_at.is_none() {
                                updated.claimed_landed_at = Some(epoch_ms());
                            }
                        }
                    }
                }
                // reinforce / no_change / weaken：保留既有阶段
                _ => {}
            }
            if has_new_evidence
                && (meaningful || parsed.delta_effect.as_deref() == Some("advance_phase"))
            {
                updated.latest_evidence_at = newest_posted_at;
            }
            // 已落地事件的事实标题不再被模型的对旧帖复述覆盖（避免未来时态回退）。
            if has_new_evidence && updated.phase != "landed_observed" {
                if let Some(conclusion) = &parsed.conclusion {
                    updated.title = conclusion.clone();
                }
            }
            if has_new_evidence {
                if let Some(basis) = &parsed.analysis_basis {
                    updated.summary = Some(basis.clone());
                }
            }
            updated.expected_at = expected_from_inputs(inputs).or(updated.expected_at);
            updated.expires_at = event_expiry(&updated);
            if updated.phase != old_phase || has_new_evidence {
                updated.state_revision += 1;
            }
            database.update_radar_event(&updated)?;
            for post in &inputs.delta {
                database.add_radar_event_evidence(&updated.id, &post.id, "delta", analysis_id)?;
            }
            Ok(Some(updated.id))
        }
        // none 或缺失：普通无关动态，不推进事件
        _ => Ok(None),
    }
}

fn create_radar_event(
    database: &Database,
    inputs: &DeltaInputs,
    parsed: &ModelJson,
    analysis_id: &str,
) -> Result<String, String> {
    let now = epoch_ms();
    let phase = ai_settable_phase(parsed.event_phase.as_deref()).unwrap_or("watching");
    let mut event = RadarEventRecord {
        id: format!("event-{now}"),
        phase: phase.into(),
        title: parsed
            .conclusion
            .clone()
            .unwrap_or_else(|| "新重置信号".into()),
        summary: parsed.analysis_basis.clone(),
        first_signal_at: inputs
            .delta
            .last()
            .map(|post| post.posted_at)
            .unwrap_or(now),
        latest_evidence_at: now,
        claimed_landed_at: (phase == "landed_claimed").then_some(now),
        observed_reset_at: None,
        closed_at: None,
        close_reason: None,
        expected_at: expected_from_inputs(inputs),
        expires_at: None,
        state_revision: 1,
    };
    event.expires_at = event_expiry(&event);
    database.insert_radar_event(&event)?;
    for post in &inputs.context {
        database.add_radar_event_evidence(&event.id, &post.id, "context", analysis_id)?;
    }
    for post in &inputs.delta {
        database.add_radar_event_evidence(&event.id, &post.id, "delta", analysis_id)?;
    }
    Ok(event.id)
}

fn expected_from_inputs(inputs: &DeltaInputs) -> Option<i64> {
    let ids = inputs
        .delta
        .iter()
        .map(|post| post.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    inputs
        .time_claims
        .iter()
        .filter(|claim| ids.contains(claim.post_id.as_str()))
        .filter_map(|claim| claim.resolved_at)
        .max()
}

/// 事件阶段次序：单调推进用。
fn phase_rank(phase: &str) -> u8 {
    match phase {
        "watching" => 0,
        "upcoming" => 1,
        "landed_claimed" => 2,
        "landed_observed" => 3,
        _ => 4,
    }
}

/// AI 可以设置的事件阶段：landed_observed 只能来自本机额度观察，closed 只能由取消逻辑设置。
fn ai_settable_phase(value: Option<&str>) -> Option<&'static str> {
    match value? {
        "watching" => Some("watching"),
        "upcoming" => Some("upcoming"),
        "landed_claimed" => Some("landed_claimed"),
        _ => None,
    }
}

const ANALYSIS_SYSTEM_PROMPT: &str = concat!(
    "You analyze public Tibo/Codex reset-related posts. NEW POSTS are the batch to judge; ",
    "EVENT CONTEXT POSTS (when present) are previously associated originals of an ongoing reset event, background only. ",
    "Reply with JSON only: {\"conclusion\":\"\",\"analysis_basis\":\"\",\"confidence\":\"low|medium|high\",\"event_relation\":\"new_event|same_event|none\",\"event_phase\":\"watching|upcoming|landed_claimed\",\"delta_effect\":\"reinforce|no_change|weaken|advance_phase|cancel|new_event\",\"signal_level\":\"none|weak|strong\",\"context_status\":\"complete|context_missing|conflicting\",\"citations\":[\"post_id\"],\"support\":[\"\"],\"against\":[\"\"],\"uncertainty\":[\"\"]}. ",
    "Write conclusion and analysis_basis in Simplified Chinese. ",
    "Each post is JSON. Its time_claims are code-authoritative facts: resolved_beijing_at may be repeated verbatim; ambiguous claims must remain ambiguous. Never calculate, convert, or invent a time. ",
    "CODE-AUTHORITATIVE EVENT STATE and NOW are facts and cannot be changed by your output. ",
    "expected_time_passed means the announced time has passed but landing is still unverified; never call it landed without a source claim or local observation in state. ",
    "If state says observed_landed, describe the posts as historical confirmation and use past tense. ",
    "conclusion must be a direct decision of at most 40 Chinese characters, without markdown, evidence, or repeated reasoning. ",
    "analysis_basis must contain the reasoning separately in 1 to 3 concise sentences and must not repeat the conclusion verbatim. ",
    "event_relation: new_event when the new posts start a distinct reset cycle; same_event when they update the ongoing event; none when unrelated. ",
    "Ordinary chatter or unrelated replies must be event_relation none with delta_effect no_change and signal_level none; never overwrite or close the ongoing event for them. ",
    "event_phase is your read of the event stage after the new posts; delta_effect describes what the new posts do to the event. ",
    "context_status: complete when the context posts give enough background, context_missing when not, conflicting when they contradict the new posts. ",
    "Apply user semantic hints only when judging signal wording and confidence. ",
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
        analysis_basis: None,
        confidence: None,
        citations_json: "[]".into(),
        support_json: "[]".into(),
        against_json: "[]".into(),
        uncertainty_json: "[]".into(),
        error_message: Some(error.to_string()),
        event_id: None,
        analysis_mode: None,
        context_hash: String::new(),
        prompt_hash: String::new(),
        event_relation: None,
        event_phase: None,
        delta_effect: None,
        signal_level: None,
        context_status: None,
        temporal_phase: None,
        valid_until: None,
        state_revision: 0,
        timezone_policy_version: time_claims::TIMEZONE_POLICY_VERSION.into(),
    })
}

struct ChatRequestError {
    fatal: bool,
    message: String,
}

async fn send_chat(client: &Client, target: &ChatTarget, body: &Value) -> Result<String, String> {
    let candidates = chat_endpoint_candidates(&target.adapter_id, target.api_base.as_deref());
    if candidates.is_empty() {
        return Err("该来源没有对话接口".into());
    }
    let mut last_error = "对话请求失败".to_string();
    for (url, default_bearer) in &candidates {
        let styles: Vec<bool> = if glm_source(&target.adapter_id) {
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
    let url = error
        .url()
        .map(|item| format!(" {}", item))
        .unwrap_or_default();
    let mut chain = Vec::new();
    let mut current: Option<&dyn Error> = Some(error);
    while let Some(item) = current {
        chain.push(item.to_string());
        current = item.source();
    }
    format!("{kind}{url}：{}", chain.join("；"))
}

/// 用户在主窗口手动确认「这是我手动使用的重置卡」：只更新指定 quota_reset_observations
/// 记录的归因为 user_confirmed，不修改任何额度快照，也不推进事件阶段。
pub fn confirm_quota_change(
    database: &Database,
    observation_id: i64,
    confirmed_at: i64,
) -> Result<RadarSnapshot, String> {
    quota_watch::save_confirmation(database, observation_id, confirmed_at)?;
    snapshot(database)
}

pub fn save_analysis_prefs(
    database: &Database,
    analyze: bool,
    range_key: &str,
    source_id: Option<&str>,
    model: Option<&str>,
    user_prompt: Option<&str>,
) -> Result<(), String> {
    let range_key = if is_valid_range_key(range_key) {
        range_key
    } else {
        "3d"
    };
    database.set_setting_bool("radar_analyze", analyze)?;
    database.set_setting_string("radar_range_key", range_key)?;
    database.set_setting_string("radar_source_id", source_id.unwrap_or(""))?;
    database.set_setting_string(
        "radar_model",
        model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(""),
    )?;
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
            .filter(|value| is_valid_range_key(value))
            .unwrap_or_else(|| "3d".into()),
        source_id: database
            .setting_string("radar_source_id")?
            .filter(|value| !value.is_empty()),
        model: database
            .setting_string("radar_model")?
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

/// 校验用户输入的自定义模型名：只允许模型 ID 常见字符，拒绝控制字符与空白变体。
fn sanitize_custom_model(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("模型名称不能为空".into());
    }
    if trimmed.len() > 128 {
        return Err("模型名称过长（最多 128 字符）".into());
    }
    let valid = trimmed.chars().all(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '/' | ':' | '@' | '+')
    });
    if !valid {
        return Err("模型名称只能包含字母、数字和 . - _ / : @ +".into());
    }
    Ok(trimmed.to_string())
}

/// 验证连接：用所选来源凭据向对话端点发一次极小请求，证明该模型名对此凭据真实可用。
pub async fn test_chat_model(
    database: &Database,
    coordinator: &RefreshCoordinator,
    source_id: &str,
    model: &str,
) -> Result<(), String> {
    let model = sanitize_custom_model(model)?;
    let target = resolve_chat_target(database, Some(source_id), Some(&model))?;
    let body = json!({
        "model": target.model,
        "messages": [{"role": "user", "content": "ping"}]
    });
    send_chat(coordinator.client(), &target, &body).await?;
    Ok(())
}

/// 保存自定义模型：校验来源存在且具备对话接口，入库去重后返回新快照。
pub fn add_custom_model(
    database: &Database,
    source_id: &str,
    model: &str,
) -> Result<RadarSnapshot, String> {
    let model = sanitize_custom_model(model)?;
    let source = database.source(source_id)?;
    if source.source_type != "api_key" {
        return Err("只有 API Key 来源支持自定义模型".into());
    }
    let api_base = database
        .user_platform(&source.platform_id)?
        .and_then(|item| item.api_base_url);
    if chat_endpoint_candidates(&source.adapter_id, api_base.as_deref()).is_empty() {
        return Err("该来源没有对话接口".into());
    }
    if chat_target(&source.adapter_id).is_some_and(|(_, default)| default == model) {
        return Err("该模型已是此来源的默认模型，无需重复添加".into());
    }
    database.add_radar_custom_model(source_id, &model)?;
    snapshot(database)
}

/// 删除自定义模型；默认模型不在库中，删除是空操作但仍返回快照保持前端一致。
pub fn delete_custom_model(
    database: &Database,
    source_id: &str,
    model: &str,
) -> Result<RadarSnapshot, String> {
    let model = sanitize_custom_model(model)?;
    database.delete_radar_custom_model(source_id, &model)?;
    snapshot(database)
}

struct ModelJson {
    conclusion: Option<String>,
    analysis_basis: Option<String>,
    confidence: Option<String>,
    citations: Vec<String>,
    support: Vec<String>,
    against: Vec<String>,
    uncertainty: Vec<String>,
    event_relation: Option<String>,
    event_phase: Option<String>,
    delta_effect: Option<String>,
    signal_level: Option<String>,
    context_status: Option<String>,
}

fn normalize_model_json(parsed: &mut ModelJson, inputs: &DeltaInputs) {
    let allow = |value: &mut Option<String>, values: &[&str]| {
        if value
            .as_deref()
            .is_some_and(|current| !values.contains(&current))
        {
            *value = None;
        }
    };
    allow(&mut parsed.confidence, &["low", "medium", "high"]);
    allow(
        &mut parsed.event_relation,
        &["new_event", "same_event", "none"],
    );
    allow(
        &mut parsed.event_phase,
        &["watching", "upcoming", "landed_claimed"],
    );
    allow(
        &mut parsed.delta_effect,
        &[
            "reinforce",
            "no_change",
            "weaken",
            "advance_phase",
            "cancel",
            "new_event",
        ],
    );
    allow(&mut parsed.signal_level, &["none", "weak", "strong"]);
    allow(
        &mut parsed.context_status,
        &["complete", "context_missing", "conflicting"],
    );
    if let Some(conclusion) = &mut parsed.conclusion {
        *conclusion = conclusion
            .replace(['`', '#', '*'], "")
            .chars()
            .take(40)
            .collect();
    }
    let ids = inputs
        .delta
        .iter()
        .chain(inputs.context.iter())
        .map(|post| post.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    parsed
        .citations
        .retain(|citation| ids.contains(citation.as_str()));
}

/// 去掉思考型模型输出中的 <think>…</think> 段（大小写不敏感）；未闭合时丢弃其后全部内容。
fn strip_think_blocks(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::new();
    let mut cursor = 0usize;
    while let Some(rel) = lower[cursor..].find("<think>") {
        let open = cursor + rel;
        out.push_str(&text[cursor..open]);
        match lower[open..].find("</think>") {
            Some(close_rel) => cursor = open + close_rel + "</think>".len(),
            None => return out,
        }
    }
    out.push_str(&text[cursor..]);
    out
}

/// 提取第一个括号平衡的 JSON 对象：容忍围栏、前后说明文字；字符串内的括号不参与计数。
fn extract_json_object(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let start = chars.iter().position(|ch| *ch == '{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for index in start..chars.len() {
        let ch = chars[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(chars[start..=index].iter().collect());
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_model_json(body: &str) -> Result<ModelJson, String> {
    let value: Value = serde_json::from_str(body).map_err(|_| "分析返回不是 JSON".to_string())?;
    let content = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or(body);
    // 思考型模型（如 GLM 系列）会把推理包在 <think> 段或围栏/闲话里；
    // 先剥思考段，再从剩余文本中提取第一个括号平衡的 JSON 对象。
    let cleaned = strip_think_blocks(content);
    let json_text =
        extract_json_object(&cleaned).ok_or_else(|| "模型未返回可解析的分析 JSON".to_string())?;
    let parsed: Value =
        serde_json::from_str(&json_text).map_err(|_| "模型未返回可解析的分析 JSON".to_string())?;
    Ok(ModelJson {
        conclusion: parsed
            .get("conclusion")
            .and_then(Value::as_str)
            .map(str::to_string),
        analysis_basis: parsed
            .get("analysis_basis")
            .and_then(Value::as_str)
            .map(str::to_string),
        confidence: parsed
            .get("confidence")
            .and_then(Value::as_str)
            .map(str::to_string),
        citations: string_list(&parsed, "citations"),
        support: string_list(&parsed, "support"),
        against: string_list(&parsed, "against"),
        uncertainty: string_list(&parsed, "uncertainty"),
        event_relation: parsed
            .get("event_relation")
            .and_then(Value::as_str)
            .map(str::to_string),
        event_phase: parsed
            .get("event_phase")
            .and_then(Value::as_str)
            .map(str::to_string),
        delta_effect: parsed
            .get("delta_effect")
            .and_then(Value::as_str)
            .map(str::to_string),
        signal_level: parsed
            .get("signal_level")
            .and_then(Value::as_str)
            .map(str::to_string),
        context_status: parsed
            .get("context_status")
            .and_then(Value::as_str)
            .map(str::to_string),
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

/// 时间范围解析：`Nd` 表示过去 N 天（1..=365），覆盖 3d/7d 快捷档。
fn parse_range_days(range_key: &str) -> Option<u32> {
    let days = range_key.strip_suffix('d')?.parse::<u32>().ok()?;
    (1..=365).contains(&days).then_some(days)
}

/// 自定义日期区间：`range:YYYY-MM-DD:YYYY-MM-DD`，包含起止两天。
fn parse_custom_range(range_key: &str) -> Option<(i64, i64)> {
    let rest = range_key.strip_prefix("range:")?;
    let (start, end) = rest.split_once(':')?;
    let start = NaiveDate::parse_from_str(start, "%Y-%m-%d").ok()?;
    let end = NaiveDate::parse_from_str(end, "%Y-%m-%d").ok()?;
    if end < start {
        return None;
    }
    let midnight_ms = |date: NaiveDate| -> Option<i64> {
        let naive = date.and_hms_opt(0, 0, 0)?;
        Local
            .from_local_datetime(&naive)
            .single()
            .map(|time| time.timestamp_millis())
    };
    let start_ms = midnight_ms(start)?;
    // 结束日当天 23:59:59.999：下一天 0 点减 1 毫秒
    let end_ms = midnight_ms(end.succ_opt()?)? - 1;
    Some((start_ms, end_ms))
}

fn is_valid_range_key(range_key: &str) -> bool {
    range_key == "today"
        || parse_range_days(range_key).is_some()
        || parse_custom_range(range_key).is_some()
}

/// 时间范围换算成 [起始毫秒, 结束毫秒]（含端点）；无上限用 i64::MAX。
fn range_bounds(range_key: &str) -> (i64, i64) {
    if let Some(bounds) = parse_custom_range(range_key) {
        return bounds;
    }
    let now = Local::now();
    if range_key == "today" {
        let naive = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap_or_else(|| now.naive_local());
        let start = Local
            .from_local_datetime(&naive)
            .single()
            .unwrap_or(now)
            .timestamp_millis();
        return (start, i64::MAX);
    }
    let days = parse_range_days(range_key).unwrap_or(7);
    (
        (now - ChronoDuration::days(days as i64)).timestamp_millis(),
        i64::MAX,
    )
}

/// 模型输入统一使用北京时间（UTC+8）：帖子时间戳在此确定性换算，
/// 原帖文本内提及的未标注时区时间（多为太平洋时间）才交给模型按系统提示词换算。
fn format_iso(ms: i64) -> String {
    let beijing = chrono::FixedOffset::east_opt(8 * 3600).expect("UTC+8 is a valid offset");
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|time| time.with_timezone(&beijing).to_rfc3339())
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
        assert_eq!(
            urls[0].0,
            "https://open.bigmodel.cn/api/coding/paas/v4/chat/completions"
        );
        assert!(urls
            .iter()
            .any(|(url, _)| url == "https://open.bigmodel.cn/api/paas/v4/chat/completions"));
        let intl = glm_chat_candidates(Some("https://api.z.ai"), true);
        assert_eq!(
            intl[0].0,
            "https://api.z.ai/api/coding/paas/v4/chat/completions"
        );
    }

    #[test]
    fn extracts_json_from_think_fence_and_prose() {
        let raw = "<think>让我想想 {这里有大括号的推理}</think>好的，结果如下：```json
{\"conclusion\":\"测试\",\"event_relation\":\"same_event\"}
``` 以上。";
        let text = strip_think_blocks(raw);
        let json = extract_json_object(&text).expect("should extract");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(
            parsed.get("conclusion").and_then(serde_json::Value::as_str),
            Some("测试")
        );
        // 字符串里的括号不参与配对
        let tricky = "{\"text\":\"包含 } 的大括号\"}";
        assert_eq!(extract_json_object(tricky).as_deref(), Some(tricky));
    }

    #[test]
    fn user_prompt_is_trimmed_and_capped() {
        assert_eq!(sanitize_user_prompt("  dashboard  "), "dashboard");
        assert_eq!(sanitize_user_prompt("a\u{0000}b"), "ab");
        let long: String = "测".repeat(USER_PROMPT_MAX_CHARS + 8);
        assert_eq!(
            sanitize_user_prompt(&long).chars().count(),
            USER_PROMPT_MAX_CHARS
        );
        assert!(DEFAULT_USER_PROMPT.contains("Hold on to your Codex"));
        assert!(DEFAULT_USER_PROMPT.contains("仪表盘"));
    }
}
