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
pub const PROMPT_VERSION: &str = "radar-v18";
pub const USER_PROMPT_MAX_CHARS: usize = 4000;
const LEGACY_DEFAULT_USER_PROMPT: &str = "若帖子提到仪表盘（dashboard）、里程碑（milestone）、庆祝（celebration）、倒计时，或出现 “Hold on to your Codex” / “抓紧你的 Codex” / “reset will land” 等措辞，视为即将重置的强信号（signal_level=strong），即使没有给出确切时间。
已落地的历史重置只作背景，不能当成否定新一轮重置的证据；普通闲聊回帖应判 none/no_change，不得推进或关闭当前事件。
没有重置相关内容，或只有旧重置而没有新信号时，才使用低把握度。";
const LEGACY_DEFAULT_USER_PROMPT_WITH_TIMEZONE: &str = "若帖子提到仪表盘（dashboard）、里程碑（milestone）、庆祝（celebration）、倒计时，或出现 “Hold on to your Codex” / “抓紧你的 Codex” / “reset will land” 等措辞，视为即将重置的强信号（signal_level=strong），即使没有给出确切时间。
已落地的历史重置只作背景，不能当成否定新一轮重置的证据；普通闲聊回帖应判 none/no_change，不得推进或关闭当前事件。
帖子提及的未标注时区的具体时间多为太平洋时间（OpenAI/旧金山），结论或依据中请换算成北京时间表述，例如「北京时间8月31日06:00」。
没有重置相关内容，或只有旧重置而没有新信号时，才使用低把握度。";
const LEGACY_DEFAULT_USER_PROMPT_V17: &str = "可重点关注 Tibo 原帖中与重置有关的特殊表达，例如：

- “Hold on to your Codex”“reset will land”“full reset”“reset usage”
- “banked reset”“reset available”“one reset per day”“first one will land”
- 仪表盘（dashboard）、里程碑（milestone）、庆祝（celebration）、倒计时、按钮已经按下等暗示性表达

这些措辞需要结合完整上下文理解。你可以继续补充新的 Tibo 用语、隐喻或近期出现的表达方式；普通闲聊中偶然出现相同单词，不代表一定存在重置信号。";
pub const DEFAULT_USER_PROMPT: &str = "请把帖子分成三类信号，不要混用：

【重置卡 banked_reset】
原帖在说可保存、可稍后手动使用的重置次数或重置卡发放/到账。
常见英文：banked reset、one reset per day、first one will land、reset available、reset card。
重置卡不是全局额度自动恢复，但仍是有效重置信号，不得判为 none。

【额度重置 quota_reset】
原帖在说 Codex / ChatGPT Work 等额度窗口实际刷新或恢复。
常见英文：reset all paid Codex/ChatGPT Work usage、full reset、reset usage、usage has reset。

【无信号 none】
普通闲聊、回复、表情，或只是顺口提到 reset，没有重置卡或额度窗口含义。

也可继续关注 Tibo 的特殊表达，例如：
- Hold on to your Codex、reset will land
- 仪表盘（dashboard）、里程碑（milestone）、庆祝（celebration）、倒计时、按钮已经按下

时间请只复述代码给出的 time_claims / resolved_beijing_at，不要自行换算北京时间。
普通闲聊中偶然出现相同单词，不代表一定存在重置信号。";

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
    pub lifecycle_consumed_at: Option<i64>,
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
    /// 本次输入 NEW POSTS 的真实 post_id；“当前判断依据”引用只能来自 citations ∩ newPostIds。
    pub new_post_ids: Vec<String>,
    /// 本次输入 EVENT CONTEXT POSTS 的真实 post_id。
    pub event_context_post_ids: Vec<String>,
    /// 本次输入 HISTORICAL CONTEXT POSTS 的真实 post_id；只允许出现在历史区。
    pub historical_post_ids: Vec<String>,
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
    /// banked_reset | quota_reset | none | unknown
    pub signal_type: Option<String>,
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
    pub user_confirmed_reset_at: Option<i64>,
    /// banked_reset | quota_reset
    pub event_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarAiAssessmentView {
    pub enabled: bool,
    /// disabled | pending | failed | covered | historical
    pub state: String,
    /// 当前（或最近）事件的最新成功分析：回答“为什么认为本轮存在重置信号”。
    pub event_analysis: Option<RadarAnalysisView>,
    /// 最近一次成功增量分析（含 event_relation=none 的无关帖子）：回答“最新帖子是否改变当前判断”。
    pub latest_delta_analysis: Option<RadarAnalysisView>,
    /// 最近一次成功分析（历史）。
    pub history: Option<RadarAnalysisView>,
    /// 最近一次失败检查的分析错误。
    pub latest_error: Option<String>,
}

/// 「最近一次事件」折叠区：仅无活动事件时展示，不冒充当前信号。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarRecentEventView {
    pub id: String,
    pub phase: String,
    pub title: String,
    pub close_reason: Option<String>,
    pub observed_reset_at: Option<i64>,
    pub closed_at: Option<i64>,
    pub post_ids: Vec<String>,
    pub analysis: Option<RadarAnalysisView>,
    pub claimed_landed_at: Option<i64>,
    pub user_confirmed_reset_at: Option<i64>,
    /// observed | user_confirmed | claimed
    pub confirmation_source: Option<String>,
    /// banked_reset | quota_reset
    pub event_type: String,
}

/// 面向用户的综合判断：由 Rust 从活动事件/时间/观察推导，React 只消费不二次判断。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarDecisionView {
    /// no_signal | watching | upcoming | expected_time_passed | landed_claimed | landed_observed | user_confirmed
    pub status: String,
    pub active_event_id: Option<String>,
    pub headline: String,
    pub time_text: String,
    pub verification_hint: Option<String>,
    pub observation_period_text: Option<String>,
    pub expected_at: Option<i64>,
    pub observed_at: Option<i64>,
    pub claimed_at: Option<i64>,
    pub user_confirmed_at: Option<i64>,
    /// landed_observed / user_confirmed 的 24 小时观察期截止时间。
    pub observation_expires_at: Option<i64>,
    /// unknown | expected | passed | claimed | observed | confirmed
    pub time_kind: String,
    pub signal_level: Option<String>,
    /// 最近一次本机观察或用户确认的重置事件；“最近一次重置”唯一来源。
    pub recent_reset: Option<RadarRecentEventView>,
    /// 最近关闭的普通雷达事件（invalid_historical_replay/timeout 等只进历史，不参与“最近一次重置”）。
    pub recent_closed_event: Option<RadarRecentEventView>,
    pub relevant_post_ids: Vec<String>,
    /// 当前判断的关键引用：主分析 citations ∩ newPostIds。
    pub current_key_citation_ids: Vec<String>,
    /// 历史上下文引用（citations ∩ historicalPostIds），只允许出现在历史区。
    pub historical_citation_ids: Vec<String>,
    /// 最近一次被判定与事件无关的新帖时间（用于“最新动态无关”摘要）。
    pub latest_irrelevant_update_at: Option<i64>,
    pub can_confirm_reset: bool,
    pub can_undo_confirm: bool,
    pub pending_unconsumed_count: i64,
    pub source_has_direct_signal: bool,
    pub strip_badge: String,
    pub strip_primary: String,
    pub strip_primary_compact: String,
    pub strip_secondary: String,
    pub recent_summary_text: Option<String>,
    pub delta_impact_text: String,
    /// banked_reset | quota_reset；无当前事件时为 None。
    pub event_type: Option<String>,
}

/// 本次检查组装的三组帖子，供 AI Tab 审计展示。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarAnalysisGroupsView {
    /// live_delta | historical_replay
    pub mode: String,
    pub new_post_ids: Vec<String>,
    pub event_context_ids: Vec<String>,
    pub historical_context_ids: Vec<String>,
    pub pending_unconsumed_count: i64,
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
    pub decision: RadarDecisionView,
    pub quota_verifications: Vec<QuotaVerificationView>,
    pub analysis_groups: RadarAnalysisGroupsView,
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
    /// 最后一次解析到公告的时间；与正文一起持久化，本次未解析到不清空。
    #[serde(default)]
    pub updated_at: Option<i64>,
    /// 当前 CodexRadar 页面是否仍出现该公告；false 时展示“最近公告”。
    #[serde(default)]
    pub is_current: bool,
    /// 距最后一次出现的毫秒数（快照时刻计算）。
    #[serde(default)]
    pub freshness_ms: Option<i64>,
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
    let event_records = database.active_radar_events()?;
    let quota_event = event_records
        .iter()
        .find(|event| event.event_type != "banked_reset");
    let quota_verifications =
        quota_watch::assess_quota_verifications(database, quota_event)?;
    // expiresAt 已过的事件不再作为“当前事件”展示；数据库仍由 reconcile_event_state 正式关闭。
    let now_ms = epoch_ms();
    let live_events: Vec<_> = event_records
        .into_iter()
        .filter(|event| event.expires_at.is_none_or(|at| at > now_ms))
        .collect();
    let active_event = live_events.first().cloned();
    let event_view_data = active_event
        .as_ref()
        .map(|record| event_view(database, record));
    let prefs = load_analysis_prefs(database)?;
    let groups = classify_analysis_posts(database, &posts, &prefs.range_key, &live_events)?;
    let ai_assessment = build_ai_assessment(
        database,
        analysis.as_ref(),
        active_event.as_ref(),
        &groups,
        &checks,
        prefs.analyze,
        &prefs.range_key,
    )?;
    let decision = build_decision(
        database,
        active_event.as_ref(),
        &ai_assessment,
        event_view_data.as_ref(),
        &posts,
        &groups,
        prefs.analyze,
    )?;
    Ok(RadarSnapshot {
        source_status: source_status.into(),
        last_synced_at,
        posts,
        latest,
        checks,
        analysis,
        models: chat_models(database)?,
        analysis_prefs: prefs,
        notice,
        source_assessment,
        event: event_view_data,
        ai_assessment,
        decision,
        quota_verifications,
        analysis_groups: groups_view(&groups),
    })
}

pub fn reconcile_event_state(database: &Database, now: i64) -> Result<(), String> {
    refresh_time_claims(database)?;
    let events = database.active_radar_events()?;
    let quota_event = events
        .iter()
        .find(|event| event.event_type != "banked_reset")
        .cloned();
    quota_watch::materialize_observations(database, quota_event.as_ref())?;
    for mut event in events {
        reconcile_one_event(database, &mut event, now)?;
    }
    Ok(())
}

fn reconcile_one_event(
    database: &Database,
    event: &mut RadarEventRecord,
    now: i64,
) -> Result<(), String> {
    let original = event.clone();
    let post_ids = database.radar_event_post_ids(&event.id)?;
    let claims = database.radar_time_claims_for_posts(&post_ids)?;
    event.expected_at = claims.iter().filter_map(|claim| claim.resolved_at).max();
    if matches!(
        event.phase.as_str(),
        "watching" | "upcoming" | "landed_claimed"
    ) {
        let observed_at = if event.event_type == "banked_reset" {
            quota_watch::banked_reset_observed_at(database, event)?
        } else {
            quota_watch::assess_quota_verifications(database, Some(event))?
                .iter()
                .filter_map(|value| value.last_reset_observed_at)
                .min()
        };
        if let Some(observed_at) = observed_at {
            event.phase = "landed_observed".into();
            event.observed_reset_at = Some(observed_at);
            event.title = event_title_for_phase("landed_observed", &event.event_type);
        }
    }
    if event.phase != "closed" && event.observed_reset_at.is_none() {
        event.title = event_title_for_phase(&event.phase, &event.event_type);
    } else if event.phase == "landed_observed" {
        event.title = event_title_for_phase("landed_observed", &event.event_type);
    }
    event.expires_at = event_expiry(event);
    if event.expires_at.is_some_and(|expires| now >= expires) {
        event.closed_at = Some(now);
        event.close_reason = Some(
            match event.phase.as_str() {
                "watching" if event.user_confirmed_reset_at.is_none() => "timeout_no_signal",
                "upcoming" => "timeout_unverified",
                "landed_claimed" => "claimed_unverified",
                "landed_observed" => "completed",
                _ if event.user_confirmed_reset_at.is_some() => "completed",
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
        || event.user_confirmed_reset_at != original.user_confirmed_reset_at
        || event.title != original.title
    {
        event.state_revision = original.state_revision + 1;
        database.update_radar_event(event)?;
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
    if event.phase != "closed" && event.observed_reset_at.is_none() {
        if let Some(confirmed_at) = event.user_confirmed_reset_at {
            return Some(confirmed_at + 24 * hour);
        }
    }
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

fn event_is_banked(event_type: &str) -> bool {
    event_type == "banked_reset"
}

fn event_title_for_phase(phase: &str, event_type: &str) -> String {
    let banked = event_is_banked(event_type);
    match (phase, banked) {
        ("watching", true) => "重置卡可能即将到账".into(),
        ("watching", false) => "可能即将重置".into(),
        ("upcoming", true) => "重置卡即将到账".into(),
        ("upcoming", false) => "预计即将重置".into(),
        ("landed_claimed", true) => "来源称重置卡已到账".into(),
        ("landed_claimed", false) => "来源称已经重置".into(),
        ("landed_observed", true) => "本机已观察到重置卡到账".into(),
        ("landed_observed", false) => "本机已观察到额度重置".into(),
        (_, true) => "重置卡事件".into(),
        _ => "重置事件".into(),
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
    if let Some(at) = record.user_confirmed_reset_at {
        timeline.push(RadarEventNodeView {
            at,
            kind: "confirmed".into(),
            label: if event_is_banked(&record.event_type) {
                "用户确认重置卡已到账".into()
            } else {
                "用户确认额度已重置".into()
            },
        });
    }
    if let Some(at) = record.observed_reset_at {
        timeline.push(RadarEventNodeView {
            at,
            kind: "observed".into(),
            label: if event_is_banked(&record.event_type) {
                "本机已观察到重置卡到账".into()
            } else {
                "本机已观察到额度重置".into()
            },
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
        user_confirmed_reset_at: record.user_confirmed_reset_at,
        event_type: record.event_type.clone(),
    }
}

/// AI 评估：拆分“事件为什么成立”（eventAnalysis）与“最新帖子是否改变判断”（latestDeltaAnalysis）。
/// `range_key` 为当前所选动态范围：旧范围的成功分析只能标 historical，不得标成当前 covered。
fn build_ai_assessment(
    database: &Database,
    latest_success: Option<&RadarAnalysisView>,
    active_event: Option<&RadarEventRecord>,
    groups: &ClassifiedPosts,
    checks: &[RadarCheckView],
    enabled: bool,
    range_key: &str,
) -> Result<RadarAiAssessmentView, String> {
    let covers_latest = groups.new_posts.is_empty();
    let latest_delta = latest_success.cloned().map(|mut analysis| {
        analysis.covers_latest = covers_latest;
        analysis
    });
    let event_analysis = match active_event {
        Some(event) => database
            .latest_event_radar_analysis(&event.id)?
            .map(|record| analysis_view(database, record, covers_latest)),
        None => None,
    };
    // 错误只认最新一次检查：历史失败记录已被后续成功覆盖，不得继续挂出误导。
    let latest_error = checks
        .first()
        .filter(|check| check.analyze_status.as_deref() == Some("failed"))
        .and_then(|check| check.error_message.clone());
    let latest_failed = checks
        .first()
        .is_some_and(|check| check.analyze_status.as_deref() == Some("failed"));
    let state = if !enabled {
        "disabled"
    } else if latest_failed {
        "failed"
    } else if !groups.new_posts.is_empty() {
        "pending"
    } else if latest_delta.as_ref().is_some_and(|analysis| {
        analysis.analysis_mode.as_deref() == Some("historical_replay")
            || analysis.range_key != range_key
    }) {
        "historical"
    } else if latest_delta.is_some() {
        "covered"
    } else {
        "pending"
    };
    Ok(RadarAiAssessmentView {
        enabled,
        state: state.into(),
        event_analysis,
        latest_delta_analysis: latest_delta,
        history: latest_success.cloned(),
        latest_error,
    })
}

/// 综合判断视图：状态与时间只由代码推导，AI 不参与；expired 事件不作为当前事件。
fn build_decision(
    database: &Database,
    active_event: Option<&RadarEventRecord>,
    ai: &RadarAiAssessmentView,
    event_view_data: Option<&RadarEventView>,
    posts: &[TiboPostView],
    groups: &ClassifiedPosts,
    ai_enabled: bool,
) -> Result<RadarDecisionView, String> {
    let now = epoch_ms();
    // “最近一次重置”只认本机观察/用户确认的事件；invalid_historical_replay、
    // timeout、claimed_unverified 等普通关闭事件只能进 recentClosedEvent（历史/来源声称）。
    let recent_reset_record = database.latest_confirmed_reset_event()?;
    let recent_reset = match recent_reset_record {
        Some(record) => Some(recent_event_view(database, &record)?),
        None => None,
    };
    let recent_closed_record = database.latest_closed_radar_event()?;
    let recent_closed_event = match recent_closed_record {
        // 与 recentReset 同一条时不重复暴露，避免摘要串味。
        Some(record) if recent_reset.as_ref().is_none_or(|reset| reset.id != record.id) => {
            Some(recent_event_view(database, &record)?)
        }
        _ => None,
    };
    let source_has_direct_signal = groups.range_posts.iter().any(|post| {
        post.explicit_reset || post.filter == "signal"
    });
    let pending_unconsumed_count = groups.new_posts.len() as i64;
    let event_analysis = ai.event_analysis.as_ref();
    let latest_delta = ai.latest_delta_analysis.as_ref();
    let mut status = "no_signal";
    let mut headline = "暂无下一轮重置信号".to_string();
    let mut time_kind = "unknown";
    let mut expected_at = None;
    let mut observed_at = None;
    let mut claimed_at = None;
    let mut user_confirmed_at = None;
    let mut observation_expires_at = None;
    let event_type = active_event.map(|event| event.event_type.clone());
    let banked = event_type.as_deref() == Some("banked_reset");
    if let Some(event) = active_event {
        expected_at = event.expected_at;
        claimed_at = event.claimed_landed_at;
        user_confirmed_at = event.user_confirmed_reset_at;
        if event.observed_reset_at.is_some() {
            status = "landed_observed";
            headline = if banked {
                "本机已观察到重置卡到账".into()
            } else {
                "本机已观察到额度重置".into()
            };
            time_kind = "observed";
            observed_at = event.observed_reset_at;
            observation_expires_at = event.expires_at;
        } else if event.user_confirmed_reset_at.is_some() {
            status = "user_confirmed";
            headline = if banked {
                "用户已确认重置卡到账".into()
            } else {
                "用户已确认额度重置".into()
            };
            time_kind = "confirmed";
            observation_expires_at = event.expires_at;
        } else if event_temporal_phase(event, now) == "expected_time_passed" {
            status = "expected_time_passed";
            headline = if banked {
                "预告到账时间已过，等待验证".into()
            } else {
                "预告时间已过，等待验证".into()
            };
            time_kind = "passed";
        } else {
            match event.phase.as_str() {
                "landed_claimed" => {
                    status = "landed_claimed";
                    headline = if banked {
                        "来源称重置卡已到账".into()
                    } else {
                        "来源称已经重置".into()
                    };
                    time_kind = "claimed";
                }
                _ if event.expected_at.is_some() => {
                    status = "upcoming";
                    headline = if banked {
                        "重置卡即将到账".into()
                    } else {
                        "预计即将重置".into()
                    };
                    time_kind = "expected";
                }
                _ => {
                    status = "watching";
                    headline = if banked {
                        "重置卡可能即将到账".into()
                    } else {
                        "可能即将重置".into()
                    };
                    time_kind = "unknown";
                }
            }
        }
    }
    let time_text = decision_time_text(
        status,
        event_type.as_deref(),
        expected_at,
        observed_at,
        claimed_at,
        user_confirmed_at,
    );
    let verification_hint = match status {
        "expected_time_passed" | "landed_claimed" => Some("等待本机检测或用户确认".into()),
        _ => None,
    };
    let observation_period_text = observation_expires_at
        .filter(|_| matches!(status, "landed_observed" | "user_confirmed"))
        .map(|at| format!("24 小时观察期至 {}", format_clock(at)));
    let can_confirm_reset = matches!(
        status,
        "watching" | "upcoming" | "expected_time_passed" | "landed_claimed"
    ) && active_event.is_some_and(|event| {
        event.observed_reset_at.is_none() && event.closed_at.is_none()
    });
    let can_undo_confirm = status == "user_confirmed";
    let relevant_post_ids = event_view_data
        .map(|view| view.post_ids.clone())
        .unwrap_or_default();
    // 当前判断依据引用：主分析 = 有活动事件时 eventAnalysis（回退 latestDelta），无活动事件时 latestDelta；
    // 只取 citations ∩ newPostIds，历史上下文引用单独存放，不进入当前依据。
    let primary_analysis = if active_event.is_some() {
        event_analysis.or(latest_delta)
    } else {
        latest_delta
    };
    let (current_key_citation_ids, historical_citation_ids) = primary_analysis
        .map(|analysis| {
            let current = analysis
                .citations
                .iter()
                .filter(|id| analysis.new_post_ids.contains(id))
                .cloned()
                .collect::<Vec<_>>();
            let historical = analysis
                .citations
                .iter()
                .filter(|id| analysis.historical_post_ids.contains(id))
                .cloned()
                .collect::<Vec<_>>();
            (current, historical)
        })
        .unwrap_or_default();
    let latest_irrelevant_update_at = latest_delta
        .filter(|analysis| analysis.event_relation.as_deref() == Some("none"))
        .and_then(|_| {
            posts
                .first()
                .filter(|post| !relevant_post_ids.contains(&post.id))
                .map(|post| post.posted_at)
        });
    // 摘要文案只从 recentReset 生成；仅有来源声称时不得称“最近一次重置”。
    let recent_summary_text = match &recent_reset {
        Some(reset) => reset
            .observed_reset_at
            .or(reset.user_confirmed_reset_at)
            .map(|at| {
                let source_label = if reset.observed_reset_at.is_some() {
                    "本机观察确认"
                } else {
                    "用户确认"
                };
                format!("最近一次重置于 {} · {}", format_clock(at), source_label)
            }),
        None => recent_closed_event.as_ref().and_then(|recent| {
            recent
                .claimed_landed_at
                .map(|at| format!("最近一次来源声称于 {} · 尚未验证", format_clock(at)))
        }),
    };
    let delta_impact_text = delta_impact_line(ai, ai_enabled, pending_unconsumed_count);
    let (strip_badge, strip_primary, strip_primary_compact, strip_secondary) = strip_copy(
        status,
        event_type.as_deref(),
        ai_enabled,
        source_has_direct_signal,
        pending_unconsumed_count,
        expected_at,
        observed_at,
        claimed_at,
        user_confirmed_at,
        observation_expires_at,
        recent_reset.as_ref(),
        latest_delta,
    );
    Ok(RadarDecisionView {
        status: status.into(),
        active_event_id: active_event.map(|event| event.id.clone()),
        headline,
        time_text,
        verification_hint,
        observation_period_text,
        expected_at,
        observed_at,
        claimed_at,
        user_confirmed_at,
        observation_expires_at,
        time_kind: time_kind.into(),
        signal_level: event_analysis.and_then(|analysis| analysis.signal_level.clone()),
        // 最近一次本机观察/用户确认的重置；“最近一次重置”唯一来源。
        recent_reset,
        // 最近关闭的普通雷达事件（含 invalid_historical_replay 等），只用于历史与来源声称提示。
        recent_closed_event,
        relevant_post_ids,
        // 当前判断依据引用（主分析 citations ∩ newPostIds）。
        current_key_citation_ids,
        // 历史上下文引用，不进入当前依据。
        historical_citation_ids,
        latest_irrelevant_update_at,
        can_confirm_reset,
        can_undo_confirm,
        pending_unconsumed_count,
        source_has_direct_signal,
        strip_badge,
        strip_primary,
        strip_primary_compact,
        strip_secondary,
        recent_summary_text,
        delta_impact_text,
        event_type,
    })
}

fn recent_event_view(
    database: &Database,
    record: &RadarEventRecord,
) -> Result<RadarRecentEventView, String> {
    let post_ids = database.radar_event_post_ids(&record.id).unwrap_or_default();
    let analysis = database
        .latest_event_radar_analysis(&record.id)?
        .map(|item| analysis_view(database, item, true));
    let confirmation_source = if record.observed_reset_at.is_some() {
        Some("observed".into())
    } else if record.user_confirmed_reset_at.is_some() {
        Some("user_confirmed".into())
    } else if record.claimed_landed_at.is_some() {
        Some("claimed".into())
    } else {
        None
    };
    Ok(RadarRecentEventView {
        id: record.id.clone(),
        phase: record.phase.clone(),
        title: record.title.clone(),
        close_reason: record.close_reason.clone(),
        observed_reset_at: record.observed_reset_at,
        closed_at: record.closed_at,
        post_ids,
        analysis,
        claimed_landed_at: record.claimed_landed_at,
        user_confirmed_reset_at: record.user_confirmed_reset_at,
        confirmation_source,
        event_type: record.event_type.clone(),
    })
}

fn decision_time_text(
    status: &str,
    event_type: Option<&str>,
    expected_at: Option<i64>,
    observed_at: Option<i64>,
    claimed_at: Option<i64>,
    user_confirmed_at: Option<i64>,
) -> String {
    let banked = event_type == Some("banked_reset");
    match status {
        "watching" => {
            if banked {
                "到账时间尚未明确".into()
            } else {
                "重置时间尚未明确".into()
            }
        }
        "upcoming" => expected_at.map_or_else(
            || {
                if banked {
                    "到账时间尚未明确".into()
                } else {
                    "重置时间尚未明确".into()
                }
            },
            |at| format!("预计北京时间 {} 左右", format_clock(at)),
        ),
        "expected_time_passed" => expected_at.map_or_else(
            || "原预告时间已过".into(),
            |at| format!("原预告时间 {}", format_clock(at)),
        ),
        "landed_claimed" => claimed_at.map_or_else(
            || {
                if banked {
                    "来源称重置卡已到账".into()
                } else {
                    "来源称额度已重置".into()
                }
            },
            |at| {
                if banked {
                    format!("来源于 {} 声称重置卡已到账", format_clock(at))
                } else {
                    format!("来源于 {} 声称额度已重置", format_clock(at))
                }
            },
        ),
        "landed_observed" => observed_at.map_or_else(
            || {
                if banked {
                    "本机已观察到重置卡到账".into()
                } else {
                    "本机已观察到额度重置".into()
                }
            },
            |at| {
                if banked {
                    format!("本机于 {} 观察到重置卡到账", format_clock(at))
                } else {
                    format!("本机于 {} 观察到额度重置", format_clock(at))
                }
            },
        ),
        "user_confirmed" => user_confirmed_at.map_or_else(
            || {
                if banked {
                    "用户已确认重置卡到账".into()
                } else {
                    "用户已确认额度重置".into()
                }
            },
            |at| {
                if banked {
                    format!("用户于 {} 确认重置卡已到账", format_clock(at))
                } else {
                    format!("用户于 {} 确认额度已重置", format_clock(at))
                }
            },
        ),
        _ => "下一次重置时间暂时无法判断".into(),
    }
}

fn delta_impact_line(ai: &RadarAiAssessmentView, enabled: bool, pending: i64) -> String {
    if !enabled {
        if pending > 0 {
            return format!("AI 未启用 · 此后新增 {pending} 条待分析");
        }
        return "AI 未启用".into();
    }
    if ai.state == "failed" {
        return ai
            .latest_error
            .clone()
            .unwrap_or_else(|| "本次分析失败".into());
    }
    let Some(latest) = &ai.latest_delta_analysis else {
        return if pending > 0 {
            "最新动态尚未分析。".into()
        } else {
            "尚未运行 AI 分析。".into()
        };
    };
    if latest.event_relation.as_deref() == Some("none") {
        return "最新动态已分析，与重置无关，不影响当前判断。".into();
    }
    if pending > 0 {
        return "有更新动态待分析，当前判断可能变化。".into();
    }
    "最新动态已分析，未改变当前判断".into()
}

#[allow(clippy::too_many_arguments)]
fn strip_copy(
    status: &str,
    event_type: Option<&str>,
    ai_enabled: bool,
    source_has_direct_signal: bool,
    pending: i64,
    expected_at: Option<i64>,
    observed_at: Option<i64>,
    claimed_at: Option<i64>,
    user_confirmed_at: Option<i64>,
    observation_expires_at: Option<i64>,
    recent_reset: Option<&RadarRecentEventView>,
    latest_delta: Option<&RadarAnalysisView>,
) -> (String, String, String, String) {
    // “最近重置”只来自本机观察/用户确认的 recentReset，禁止回退来源声称或关闭时间。
    let recent_token = recent_reset.and_then(|item| {
        item.observed_reset_at
            .or(item.user_confirmed_reset_at)
            .map(|at| format!("最近重置于 {}", format_clock(at)))
    });
    let banked = event_type == Some("banked_reset");
    let analyzed = latest_delta.is_some();
    let ai_token = if !ai_enabled {
        if pending > 0 {
            format!("AI 未启用 · 此后新增 {pending} 条待分析")
        } else {
            "AI 未启用".into()
        }
    } else if analyzed {
        "AI 已分析".into()
    } else {
        "AI 尚未分析".into()
    };
    if !ai_enabled && !matches!(status, "landed_observed" | "user_confirmed") {
        if matches!(
            status,
            "watching" | "upcoming" | "expected_time_passed" | "landed_claimed"
        ) {
            let primary = if source_has_direct_signal {
                "来源出现直接重置信号".into()
            } else {
                decision_time_text(
                    status,
                    event_type,
                    expected_at,
                    observed_at,
                    claimed_at,
                    user_confirmed_at,
                )
            };
            let compact = expected_at
                .map(|at| format!("预计 {}", format_clock(at)))
                .unwrap_or_else(|| primary.clone());
            let secondary = expected_at.map_or_else(
                || "等待验证".into(),
                |at| format!("预计 {} · 等待验证", format_clock(at)),
            );
            return ("AI 未启用".into(), primary, compact, secondary);
        }
        if source_has_direct_signal {
            return (
                "AI 未启用".into(),
                "来源出现直接重置信号".into(),
                "来源出现直接重置信号".into(),
                recent_token.unwrap_or_else(|| "AI 尚未分析".into()),
            );
        }
        return (
            "AI 未启用".into(),
            "来源动态已同步".into(),
            "来源动态已同步".into(),
            recent_token.unwrap_or_else(|| "本机未观察到新变化".into()),
        );
    }
    match status {
        "landed_observed" => {
            let primary = decision_time_text(
                status,
                event_type,
                expected_at,
                observed_at,
                claimed_at,
                user_confirmed_at,
            );
            let compact = observed_at
                .map(|at| {
                    if banked {
                        format!("{} 重置卡到账", format_clock(at))
                    } else {
                        format!("{} 额度重置", format_clock(at))
                    }
                })
                .unwrap_or_else(|| primary.clone());
            let secondary = observation_expires_at
                .map(|at| format!("24 小时观察期至 {}", format_clock(at)))
                .unwrap_or_else(|| "等待观察期结束".into());
            ("已观察".into(), primary, compact, secondary)
        }
        "user_confirmed" => {
            let primary = decision_time_text(
                status,
                event_type,
                expected_at,
                observed_at,
                claimed_at,
                user_confirmed_at,
            );
            let compact = user_confirmed_at
                .map(|at| format!("{} 已确认", format_clock(at)))
                .unwrap_or_else(|| primary.clone());
            let secondary = observation_expires_at
                .map(|at| format!("24 小时观察期至 {}", format_clock(at)))
                .unwrap_or_else(|| "等待观察期结束".into());
            ("已确认".into(), primary, compact, secondary)
        }
        "landed_claimed" => {
            let primary = if banked {
                "来源称重置卡已到账".into()
            } else {
                "来源称已经重置".into()
            };
            (
                "待验证".into(),
                primary,
                if banked {
                    "重置卡已到账".into()
                } else {
                    "来源称已经重置".into()
                },
                "等待本机检测或用户确认".into(),
            )
        }
        "expected_time_passed" => (
            "等待验证".into(),
            expected_at.map_or_else(
                || "预告时间已过，等待验证".into(),
                |at| format!("原预告时间 {}", format_clock(at)),
            ),
            expected_at
                .map(|at| format!("原预告 {}", format_clock(at)))
                .unwrap_or_else(|| "预告已过".into()),
            "等待本机检测或用户确认".into(),
        ),
        "upcoming" => {
            let primary = expected_at.map_or_else(
                || {
                    if banked {
                        "重置卡即将到账".into()
                    } else {
                        "预计即将重置".into()
                    }
                },
                |at| format!("预计北京时间 {} 左右", format_clock(at)),
            );
            let compact = expected_at
                .map(|at| format!("预计 {}", format_clock(at)))
                .unwrap_or_else(|| primary.clone());
            (
                "强信号".into(),
                primary,
                compact,
                format!("来源明确预告 · {ai_token} · 等待验证"),
            )
        }
        "watching" => (
            "观察中".into(),
            if banked {
                "到账时间尚未明确".into()
            } else {
                "重置时间尚未明确".into()
            },
            if banked {
                "到账时间尚未明确".into()
            } else {
                "重置时间尚未明确".into()
            },
            format!("{ai_token} · 等待验证"),
        ),
        _ => {
            // 无信号：不重复徽章状态，综合行只给“最近重置”或本机无变化。
            (
                "暂无新信号".into(),
                "下一次重置时间暂时无法判断".into(),
                "下一次时间暂无法判断".into(),
                recent_token.unwrap_or_else(|| "本机未观察到新变化".into()),
            )
        }
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
    match notice {
        Some(notice) => {
            let mut stored = notice.clone();
            stored.updated_at = Some(epoch_ms());
            stored.is_current = true;
            stored.freshness_ms = Some(0);
            let payload = serde_json::to_string(&stored).unwrap_or_else(|_| "{}".into());
            database.set_setting_string("radar_notice_json", &payload)?;
            database.set_setting_string("radar_notice_seen_at", &epoch_ms().to_string())?;
            database.set_setting_string("radar_notice_present_current", "true")
        }
        None => {
            // 本次没有解析到公告：不清空最后一次公告，只标记当前页不再出现。
            database.set_setting_string("radar_notice_present_current", "false")
        }
    }
}

fn load_notice(database: &Database) -> Result<Option<RadarNotice>, String> {
    let Some(raw) = database.setting_string("radar_notice_json")? else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let mut notice: RadarNotice = serde_json::from_str(&raw).unwrap_or(RadarNotice {
        headline: String::new(),
        lead: None,
        items: Vec::new(),
        updated_at: None,
        is_current: false,
        freshness_ms: None,
    });
    if notice.headline.is_empty() {
        return Ok(None);
    }
    let is_current = database
        .setting_string("radar_notice_present_current")?
        .map(|value| value == "true")
        .unwrap_or(false);
    notice.is_current = is_current;
    notice.freshness_ms = notice
        .updated_at
        .map(|at| (epoch_ms().saturating_sub(at)).max(0));
    Ok(Some(notice))
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
        lifecycle_consumed_at: post.lifecycle_consumed_at,
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
        new_post_ids: json_list(&record.new_post_ids_json),
        event_context_post_ids: json_list(&record.event_context_post_ids_json),
        historical_post_ids: json_list(&record.historical_post_ids_json),
        error_message: record.error_message,
        covers_latest,
        event_id: record.event_id,
        analysis_mode: record.analysis_mode,
        event_relation: record.event_relation,
        event_phase: record.event_phase,
        delta_effect: record.delta_effect,
        signal_level: record.signal_level,
        context_status: record.context_status,
        signal_type: Some(record.signal_type).filter(|value| !value.is_empty()),
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

/// 真实 post_id 列表转 JSON 数组字符串（分析输入分组落库）。
fn post_id_list_json<'a, I>(ids: I) -> String
where
    I: Iterator<Item = &'a String>,
{
    let ids: Vec<&str> = ids.map(String::as_str).collect();
    serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into())
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

/// 三组帖子划分：生命周期新增、当前事件上下文、历史上下文。
struct ClassifiedPosts {
    mode: &'static str,
    range_posts: Vec<TiboPostView>,
    new_posts: Vec<TiboPostView>,
    event_context: Vec<TiboPostView>,
    historical: Vec<TiboPostView>,
}

fn groups_view(groups: &ClassifiedPosts) -> RadarAnalysisGroupsView {
    RadarAnalysisGroupsView {
        mode: groups.mode.into(),
        new_post_ids: groups.new_posts.iter().map(|post| post.id.clone()).collect(),
        event_context_ids: groups
            .event_context
            .iter()
            .map(|post| post.id.clone())
            .collect(),
        historical_context_ids: groups.historical.iter().map(|post| post.id.clone()).collect(),
        pending_unconsumed_count: groups.new_posts.len() as i64,
    }
}

fn classify_analysis_posts(
    database: &Database,
    posts: &[TiboPostView],
    range_key: &str,
    active_events: &[RadarEventRecord],
) -> Result<ClassifiedPosts, String> {
    let (window_start, window_end) = range_bounds(range_key);
    let range_posts: Vec<_> = posts
        .iter()
        .filter(|post| post.posted_at >= window_start && post.posted_at <= window_end)
        .cloned()
        .collect();
    let closed_ids = database
        .closed_radar_event_post_ids()?
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let mut event_ids = std::collections::HashSet::new();
    for event in active_events {
        event_ids.extend(database.radar_event_post_ids(&event.id).unwrap_or_default());
    }
    let mut new_posts = Vec::new();
    let mut event_context = Vec::new();
    let mut historical = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for post in posts {
        let in_range = post.posted_at >= window_start && post.posted_at <= window_end;
        if closed_ids.contains(&post.id) {
            if in_range {
                historical.push(post.clone());
                seen.insert(post.id.as_str());
            }
            continue;
        }
        if in_range && post.lifecycle_consumed_at.is_none() {
            new_posts.push(post.clone());
            seen.insert(post.id.as_str());
            continue;
        }
        if event_ids.contains(&post.id) {
            event_context.push(post.clone());
            seen.insert(post.id.as_str());
            continue;
        }
        if in_range && !seen.contains(post.id.as_str()) {
            historical.push(post.clone());
        }
    }
    event_context.retain(|post| !new_posts.iter().any(|item| item.id == post.id));
    let historical_custom = range_key.starts_with("range:") && window_end < start_of_today_ms();
    let mode = if historical_custom {
        "historical_replay"
    } else {
        "live_delta"
    };
    Ok(ClassifiedPosts {
        mode,
        range_posts,
        new_posts,
        event_context,
        historical,
    })
}

fn start_of_today_ms() -> i64 {
    let now = Local::now();
    let naive = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap_or_else(|| now.naive_local());
    Local
        .from_local_datetime(&naive)
        .single()
        .unwrap_or(now)
        .timestamp_millis()
}

/// 本轮分析的新增/上下文分组：NEW POSTS 才能创建或推进事件。
struct DeltaInputs {
    range_key: String,
    /// live_delta | historical_replay
    mode: &'static str,
    delta: Vec<TiboPostView>,
    context: Vec<TiboPostView>,
    historical: Vec<TiboPostView>,
    /// 真实 post_id -> 本次请求别名（新帖 N1..、上下文帖 C1..、历史帖 H1..）。
    aliases: std::collections::HashMap<String, String>,
    event_status: Option<String>,
    time_claims: Vec<RadarTimeClaimRecord>,
    temporal_phase: String,
    valid_until: Option<i64>,
    state_revision: i64,
}

fn delta_post_block(post: &TiboPostView, claims: &[RadarTimeClaimRecord], alias: &str) -> String {
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
    // 不发真实 post_id 与 url：二者都内含长数字编号，防止模型在正文里复述。
    json!({
        "alias": alias,
        "published_beijing_at": format_iso(post.posted_at),
        "text": post.text,
        "time_claims": post_claims,
    })
    .to_string()
}

/// 帖子在本次请求中的短别名；别名必然存在（与 delta/context 同源构建）。
fn post_alias<'a>(aliases: &'a std::collections::HashMap<String, String>, post: &TiboPostView) -> &'a str {
    aliases
        .get(&post.id)
        .map(String::as_str)
        .unwrap_or_default()
}

/// 帖子的自然语言友好标签（北京时间，含“最新/此前”分组语境），用于正文可读化。
fn radar_post_label(post: &TiboPostView, latest: bool) -> String {
    let group = if latest { "最新" } else { "此前" };
    let Some(time) = chrono::DateTime::from_timestamp_millis(post.posted_at) else {
        return format!("时间未知的{group}帖子");
    };
    let beijing = chrono::FixedOffset::east_opt(8 * 3600).expect("UTC+8 is a valid offset");
    let time = time.with_timezone(&beijing);
    format!(
        "{}月{}日 {:02}:{:02} 的{}帖子",
        time.format("%-m"),
        time.format("%-d"),
        time.format("%H"),
        time.format("%M"),
        group
    )
}

/// 确定性可读化：仅替换本次已知帖子的真实 post_id 与别名，不做任何通用长数字清洗，
/// 避免误伤 25M、日期、时间等正文内容。按匹配串长度降序替换，防止 N1 截断 N12。
fn humanize_post_refs(text: &str, inputs: &DeltaInputs) -> String {
    let delta_ids: std::collections::HashSet<&str> =
        inputs.delta.iter().map(|post| post.id.as_str()).collect();
    let mut replacements: Vec<(&str, String)> = Vec::new();
    for post in inputs
        .delta
        .iter()
        .chain(inputs.context.iter())
        .chain(inputs.historical.iter())
    {
        let label = radar_post_label(post, delta_ids.contains(post.id.as_str()));
        replacements.push((post.id.as_str(), label.clone()));
        let alias = post_alias(&inputs.aliases, post);
        if !alias.is_empty() {
            replacements.push((alias, label));
        }
    }
    replacements.sort_by_key(|(needle, _)| std::cmp::Reverse(needle.chars().count()));
    let mut out = text.to_string();
    for (needle, label) in replacements {
        if out.contains(needle) {
            out = out.replace(needle, &label);
        }
    }
    out
}

fn collect_delta_inputs(database: &Database, range_key: &str) -> Result<DeltaInputs, String> {
    let views = database
        .list_tibo_posts(80)?
        .into_iter()
        .map(to_view)
        .collect::<Vec<_>>();
    let now = epoch_ms();
    let events = database.active_radar_events()?;
    let active: Vec<_> = events
        .iter()
        .filter(|item| item.expires_at.is_none_or(|at| at > now))
        .cloned()
        .collect();
    let classified = classify_analysis_posts(database, &views, range_key, &active)?;
    if classified.range_posts.is_empty()
        && classified.event_context.is_empty()
        && classified.new_posts.is_empty()
    {
        return Err("当前时间窗内没有 Tibo 动态可分析".into());
    }
    let delta = classified.new_posts.clone();
    let context = classified.event_context.clone();
    let historical = classified.historical.clone();
    let mut mode = classified.mode;
    if mode == "live_delta" && !delta.is_empty() {
        let newest = delta.iter().map(|post| post.posted_at).max().unwrap_or(now);
        if derived_event_already_expired("watching", newest, None, None, now) {
            mode = "historical_replay";
        }
    }
    let aliases = delta
        .iter()
        .enumerate()
        .map(|(index, post)| (post.id.clone(), format!("N{}", index + 1)))
        .chain(
            context
                .iter()
                .enumerate()
                .map(|(index, post)| (post.id.clone(), format!("C{}", index + 1))),
        )
        .chain(
            historical
                .iter()
                .enumerate()
                .map(|(index, post)| (post.id.clone(), format!("H{}", index + 1))),
        )
        .collect();
    let post_ids = delta
        .iter()
        .chain(context.iter())
        .chain(historical.iter())
        .map(|post| post.id.clone())
        .collect::<Vec<_>>();
    let claims = database.radar_time_claims_for_posts(&post_ids)?;
    let headline_event = active.first();
    let temporal_phase = headline_event.map_or_else(
        || {
            if mode == "historical_replay" {
                "historical".into()
            } else {
                "timeless".into()
            }
        },
        |record| event_temporal_phase(record, now),
    );
    let event_status = if active.is_empty() {
        None
    } else {
        Some(
            json!({
                "events": active.iter().map(|record| json!({
                    "event_id": record.id,
                    "event_type": record.event_type,
                    "state_revision": record.state_revision,
                    "phase": record.phase,
                    "temporal_phase": event_temporal_phase(record, now),
                    "first_signal_at": format_iso(record.first_signal_at),
                    "latest_evidence_at": format_iso(record.latest_evidence_at),
                    "claimed_landed_at": record.claimed_landed_at.map(format_iso),
                    "local_quota_reset_observed_at": record.observed_reset_at.map(format_iso),
                    "user_confirmed_reset_at": record.user_confirmed_reset_at.map(format_iso),
                    "expected_at": record.expected_at.map(format_iso),
                })).collect::<Vec<_>>(),
                "verification_availability": "derived_by_rust",
            })
            .to_string(),
        )
    };
    let valid_until = headline_event
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
        historical,
        aliases,
        event_status,
        time_claims: claims,
        temporal_phase,
        valid_until,
        state_revision: active.iter().map(|record| record.state_revision).sum(),
    })
}

fn derived_event_already_expired(
    phase: &str,
    latest_evidence_at: i64,
    expected_at: Option<i64>,
    claimed_landed_at: Option<i64>,
    now: i64,
) -> bool {
    let dummy = RadarEventRecord {
        id: String::new(),
        phase: phase.into(),
        title: String::new(),
        summary: None,
        first_signal_at: latest_evidence_at,
        latest_evidence_at,
        claimed_landed_at,
        observed_reset_at: None,
        closed_at: None,
        close_reason: None,
        expected_at,
        expires_at: None,
        state_revision: 0,
        user_confirmed_reset_at: None,
        event_type: "quota_reset".into(),
    };
    event_expiry(&dummy).is_some_and(|at| now >= at)
}

async fn run_analysis(
    database: &Database,
    coordinator: &RefreshCoordinator,
    range_key: &str,
    source_id: Option<&str>,
    model: Option<&str>,
) -> Result<(), String> {
    let inputs = collect_delta_inputs(database, range_key)?;
    if inputs.delta.is_empty() && inputs.context.is_empty() && inputs.historical.is_empty() {
        return Ok(());
    }
    // 无新增帖子的 live_delta：默认跳过，不为同样内容重复调用模型；
    // 但最近一次成功分析属于其他动态范围时，对当前范围补一次历史解释
    // （提示词已约束：无新增帖子只生成历史解释，不创建或推进事件），
    // 避免切换范围后无论刷新多少次都只有旧范围分析。
    if inputs.mode != "historical_replay" && inputs.delta.is_empty() {
        let same_range_analyzed = database
            .latest_radar_analysis()?
            .is_some_and(|record| record.range_key == range_key);
        if same_range_analyzed {
            return Ok(());
        }
    }
    let user_prompt = load_analysis_prefs(database)?.user_prompt;
    let joined = |posts: &[TiboPostView]| {
        posts
            .iter()
            .map(|post| delta_post_block(post, &inputs.time_claims, post_alias(&inputs.aliases, post)))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let input_hash = format!("{:x}", simple_hash(&joined(&inputs.delta)));
    // 事件状态参与复用键：阶段/本机观察变化后，旧的结论不应被复用。
    let context_hash = format!(
        "{:x}",
        simple_hash(&format!(
            "{}|{}|{}|{}|{}|{}",
            joined(&inputs.context),
            joined(&inputs.historical),
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
    sections.push(format!(
        "分析时刻（北京时间，代码权威）：{}",
        format_iso(epoch_ms())
    ));
    if let Some(status) = &inputs.event_status {
        sections.push(format!("代码权威事件状态（不得修改）：{status}"));
    }
    if !inputs.delta.is_empty() {
        sections.push(format!(
            "本次新增帖子（NEW POSTS，只有这些可以创建或推进事件）：\n{}",
            inputs
                .delta
                .iter()
                .map(|post| delta_post_block(post, &inputs.time_claims, post_alias(&inputs.aliases, post)))
                .collect::<Vec<_>>()
                .join("\n\n")
        ));
    } else {
        sections.push("本次新增帖子（NEW POSTS）：无".into());
    }
    if !inputs.context.is_empty() {
        sections.push(format!(
            "事件上下文帖子（EVENT CONTEXT，只帮助理解当前事件，不能单独作为新证据）：\n{}",
            inputs
                .context
                .iter()
                .map(|post| delta_post_block(post, &inputs.time_claims, post_alias(&inputs.aliases, post)))
                .collect::<Vec<_>>()
                .join("\n\n")
        ));
    }
    if !inputs.historical.is_empty() {
        sections.push(format!(
            "历史上下文帖子（HISTORICAL CONTEXT，只能生成历史解释，不能推进事件）：\n{}",
            inputs
                .historical
                .iter()
                .map(|post| delta_post_block(post, &inputs.time_claims, post_alias(&inputs.aliases, post)))
                .collect::<Vec<_>>()
                .join("\n\n")
        ));
    }
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
        "content": format!("结合三组帖子判断本次新增帖子；没有新增帖子时只生成历史解释，不得创建或推进事件：\n{input}")
    }));
    let body = json!({
        "model": target.model,
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
        from_posted_at: inputs
            .delta
            .iter()
            .chain(inputs.historical.iter())
            .map(|post| post.posted_at)
            .min(),
        to_posted_at: inputs
            .delta
            .iter()
            .chain(inputs.historical.iter())
            .map(|post| post.posted_at)
            .max(),
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
        new_post_ids_json: post_id_list_json(inputs.delta.iter().map(|post| &post.id)),
        event_context_post_ids_json: post_id_list_json(inputs.context.iter().map(|post| &post.id)),
        historical_post_ids_json: post_id_list_json(inputs.historical.iter().map(|post| &post.id)),
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
        signal_type: parsed
            .signal_type
            .clone()
            .unwrap_or_else(|| "unknown".into()),
    };
    // 事件推进先于落库，得到的 event_id 一并写入分析记录。
    let (event_id, replay) = apply_analysis_to_event(database, inputs, &parsed, &record.id)?;
    let analysis_mode = if replay {
        Some("historical_replay".into())
    } else {
        Some(inputs.mode.into())
    };
    let record = RadarAnalysisRecord {
        event_id,
        analysis_mode,
        ..record
    };
    database.insert_radar_analysis(&record)?;
    if !inputs.delta.is_empty() {
        database.mark_tibo_posts_consumed(
            &inputs
                .delta
                .iter()
                .map(|post| post.id.clone())
                .collect::<Vec<_>>(),
            epoch_ms(),
        )?;
    }
    Ok(())
}

/// 把 AI 的结构化增量输出套到当前事件上。
/// 返回 (event_id, historical_replay)。没有引用真正 NEW POSTS 时不改事件。
fn apply_analysis_to_event(
    database: &Database,
    inputs: &DeltaInputs,
    parsed: &ModelJson,
    analysis_id: &str,
) -> Result<(Option<String>, bool), String> {
    if inputs.mode == "historical_replay" {
        return Ok((None, true));
    }
    let cited_new = cited_new_posts(inputs, parsed);
    let cited_context = cited_context_posts(inputs, parsed);
    if cited_new.is_empty() {
        return Ok((None, false));
    }
    if parsed.signal_level.as_deref() == Some("none")
        || !matches!(
            parsed.event_relation.as_deref(),
            Some("new_event" | "same_event")
        )
    {
        return Ok((None, false));
    }
    let Some(event_type) = event_type_from_signal(parsed.signal_type.as_deref()) else {
        return Ok((None, false));
    };
    if !cited_new.iter().any(|post| post_has_source_signal(post)) {
        return Ok((None, false));
    }
    let newest_posted_at = cited_new.iter().map(|post| post.posted_at).max().unwrap_or(0);
    let expected_at = expected_from_posts(inputs, &cited_new);
    let phase = ai_settable_phase(parsed.event_phase.as_deref()).unwrap_or("watching");
    let claimed_at = (phase == "landed_claimed").then_some(newest_posted_at);
    let now = epoch_ms();
    if derived_event_already_expired(phase, newest_posted_at, expected_at, claimed_at, now) {
        return Ok((None, true));
    }
    match parsed.event_relation.as_deref() {
        Some("new_event") => {
            if let Some(active) = database.active_radar_event_of_type(event_type)? {
                if newest_posted_at <= active.latest_evidence_at {
                    return Ok((Some(active.id), false));
                }
                let mut closed = active;
                closed.phase = "closed".into();
                closed.closed_at = Some(now);
                closed.close_reason = Some("出现新一轮重置信号".into());
                closed.state_revision += 1;
                database.update_radar_event(&closed)?;
            }
            let event_id = create_radar_event(
                database,
                inputs,
                parsed,
                analysis_id,
                &cited_new,
                &cited_context,
            )?;
            Ok((Some(event_id), false))
        }
        Some("same_event") => {
            let Some(active) = database.active_radar_event_of_type(event_type)? else {
                let event_id = create_radar_event(
                    database,
                    inputs,
                    parsed,
                    analysis_id,
                    &cited_new,
                    &cited_context,
                )?;
                return Ok((Some(event_id), false));
            };
            if active.expires_at.is_some_and(|at| now >= at) {
                return Ok((None, true));
            }
            let mut updated = active;
            let has_new_evidence = newest_posted_at > updated.latest_evidence_at;
            let old_phase = updated.phase.clone();
            match parsed.delta_effect.as_deref() {
                Some("cancel") if has_new_evidence => {
                    updated.phase = "closed".into();
                    updated.closed_at = Some(now);
                    updated.close_reason = parsed
                        .conclusion
                        .clone()
                        .or_else(|| Some("信号取消或失效".into()));
                }
                Some("advance_phase") if has_new_evidence => {
                    if let Some(next) = ai_settable_phase(parsed.event_phase.as_deref()) {
                        if phase_rank(next) > phase_rank(&updated.phase) {
                            updated.phase = next.into();
                            if next == "landed_claimed" && updated.claimed_landed_at.is_none() {
                                updated.claimed_landed_at = Some(newest_posted_at);
                            }
                        }
                    }
                }
                _ => {}
            }
            if has_new_evidence {
                updated.latest_evidence_at = newest_posted_at;
                if let Some(basis) = &parsed.analysis_basis {
                    updated.summary = Some(basis.clone());
                }
            }
            if updated.phase != "closed" && updated.observed_reset_at.is_none() {
                updated.title = event_title_for_phase(&updated.phase, &updated.event_type);
            }
            updated.expected_at = expected_at.or(updated.expected_at);
            updated.expires_at = event_expiry(&updated);
            if updated.phase != old_phase || has_new_evidence {
                updated.state_revision += 1;
            }
            database.update_radar_event(&updated)?;
            add_cited_evidence(database, &updated.id, analysis_id, &cited_new, &cited_context)?;
            Ok((Some(updated.id), false))
        }
        _ => Ok((None, false)),
    }
}

fn post_has_source_signal(post: &TiboPostView) -> bool {
    post.explicit_reset || post.filter == "signal" || post.filter == "related"
}

fn cited_new_posts<'a>(inputs: &'a DeltaInputs, parsed: &ModelJson) -> Vec<&'a TiboPostView> {
    let closed: std::collections::HashSet<&str> = inputs
        .historical
        .iter()
        .map(|post| post.id.as_str())
        .collect();
    inputs
        .delta
        .iter()
        .filter(|post| parsed.citations.iter().any(|id| id == &post.id) && !closed.contains(post.id.as_str()))
        .collect()
}

fn cited_context_posts<'a>(inputs: &'a DeltaInputs, parsed: &ModelJson) -> Vec<&'a TiboPostView> {
    inputs
        .context
        .iter()
        .filter(|post| parsed.citations.iter().any(|id| id == &post.id))
        .collect()
}

fn add_cited_evidence(
    database: &Database,
    event_id: &str,
    analysis_id: &str,
    cited_new: &[&TiboPostView],
    cited_context: &[&TiboPostView],
) -> Result<(), String> {
    for post in cited_new {
        database.add_radar_event_evidence(event_id, &post.id, "delta", analysis_id)?;
    }
    for post in cited_context {
        database.add_radar_event_evidence(event_id, &post.id, "context", analysis_id)?;
    }
    Ok(())
}

fn create_radar_event(
    database: &Database,
    inputs: &DeltaInputs,
    parsed: &ModelJson,
    analysis_id: &str,
    cited_new: &[&TiboPostView],
    cited_context: &[&TiboPostView],
) -> Result<String, String> {
    let now = epoch_ms();
    let phase = ai_settable_phase(parsed.event_phase.as_deref()).unwrap_or("watching");
    let newest = cited_new.iter().map(|post| post.posted_at).max().unwrap_or(now);
    let oldest = cited_new.iter().map(|post| post.posted_at).min().unwrap_or(now);
    let expected_at = expected_from_posts(inputs, cited_new);
    let event_type = event_type_from_signal(parsed.signal_type.as_deref()).unwrap_or("quota_reset");
    let mut event = RadarEventRecord {
        id: format!("event-{now}"),
        phase: phase.into(),
        title: event_title_for_phase(phase, event_type),
        summary: parsed.analysis_basis.clone(),
        first_signal_at: oldest,
        latest_evidence_at: newest,
        claimed_landed_at: (phase == "landed_claimed").then_some(newest),
        observed_reset_at: None,
        closed_at: None,
        close_reason: None,
        expected_at,
        expires_at: None,
        state_revision: 1,
        user_confirmed_reset_at: None,
        event_type: event_type.into(),
    };
    event.expires_at = event_expiry(&event);
    database.insert_radar_event(&event)?;
    add_cited_evidence(database, &event.id, analysis_id, cited_new, cited_context)?;
    Ok(event.id)
}

fn expected_from_posts(inputs: &DeltaInputs, posts: &[&TiboPostView]) -> Option<i64> {
    let ids = posts
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

fn event_type_from_signal(value: Option<&str>) -> Option<&'static str> {
    match value? {
        "banked_reset" => Some("banked_reset"),
        "quota_reset" => Some("quota_reset"),
        _ => None,
    }
}

const ANALYSIS_SYSTEM_PROMPT: &str = concat!(
    "You analyze public Tibo/Codex reset-related posts. The product recognizes two distinct reset signal types. ",
    "A banked reset is a saved, one-time reset delivered to an account for later manual use. Wording such as banked reset, reset available, reset card, one reset per day, compensatory reset, or first one will land may announce its grant, delivery, or availability. ",
    "A banked reset is a valid new reset signal even though it does not immediately refresh usage windows. Never classify it as unrelated merely because it is not a global or automatic quota reset. ",
    "A quota reset is an actual refresh or restoration of Codex, ChatGPT Work, or related usage-limit windows, including resetting paid-user usage, restoring rate limits, a full or global reset, or a statement that such a reset was executed. ",
    "Banked-reset delivery is not proof that quota windows have already reset, and an observed quota refresh is not proof that a banked reset was delivered. ",
    "signal_type must be banked_reset, quota_reset, or none. banked reset, one reset per day, first one will land, and reset available are banked_reset. Never output none merely because a banked reset is not a global automatic quota refresh. quota_reset means usage windows actually refresh or restore. ",
    "When a new signal exists, conclusion and analysis_basis must explicitly call it 重置卡 or 额度重置. new_event or same_event requires signal_type banked_reset or quota_reset, at least one NEW POST citation, and must not reuse a closed event. Use upcoming for promised delivery, landed_claimed for claimed delivery, and watching when timing is unclear. ",
    "Input has three groups: ",
    "NEW POSTS are genuinely unconsumed posts and the only posts that may create or advance an event; ",
    "EVENT CONTEXT POSTS are already linked to the current event and must not become new evidence just because they reappear; ",
    "HISTORICAL CONTEXT POSTS are already analyzed, closed-event, or old posts brought in by a wider range and may only produce historical explanation. ",
    "EVENT CONTEXT and HISTORICAL CONTEXT must never be the sole basis of a new event. ",
    "new_event or same_event must cite at least one NEW POST. Closed-event posts must not reactivate an event. ",
    "Do not splice old signal semantics with the timestamp of a new unrelated post to invent a new event. ",
    "Reply with JSON only: {\"conclusion\":\"\",\"analysis_basis\":\"\",\"confidence\":\"low|medium|high\",\"event_relation\":\"new_event|same_event|none\",\"event_phase\":\"watching|upcoming|landed_claimed\",\"delta_effect\":\"reinforce|no_change|weaken|advance_phase|cancel|new_event\",\"signal_level\":\"none|weak|strong\",\"signal_type\":\"banked_reset|quota_reset|none\",\"context_status\":\"complete|context_missing|conflicting\",\"citations\":[\"post_alias\"],\"support\":[\"\"],\"against\":[\"\"],\"uncertainty\":[\"\"]}. ",
    "Write conclusion and analysis_basis in Simplified Chinese. support, against, and uncertainty are expandable details, also in Simplified Chinese. ",
    "Each post is JSON and carries a short alias: N1, N2 … for NEW POSTS, C1, C2 … for EVENT CONTEXT, H1, H2 … for HISTORICAL CONTEXT. ",
    "In citations return exactly these aliases, one per cited post; never return raw numeric post ids or URLs. ",
    "Never write a raw numeric post id in conclusion, analysis_basis, support, against, or uncertainty; ",
    "refer to posts in natural language such as \u{201c}the latest post\u{201d}, \u{201c}the earlier announcement post\u{201d}, or \u{201c}the post from 13:17 on Aug 31\u{201d}. ",
    "analysis_basis must be 1 to 3 core sentences and must not repeat the conclusion verbatim. ",
    "conclusion must answer only whether the genuinely new posts contain a new reset signal, in natural Simplified Chinese with no slash-separated post references. ",
    "When older posts describe a completed reset, call them 上一轮历史背景 and never present them as confirmation of the current batch. ",
    "support may only contain claims the cited posts directly support; anything merely speculative belongs in uncertainty. ",
    "Every conclusion must be backed by citations referring to real input posts; never cite a post that was not provided. ",
    "Each post's time_claims are code-authoritative facts: resolved_beijing_at may be repeated verbatim; ambiguous claims must remain ambiguous. Never calculate, convert, or invent a time. If an unresolved post only says in about N hours, preserve that relative wording instead of inventing an absolute time. ",
    "CODE-AUTHORITATIVE EVENT STATE and the injected analysis time are facts and cannot be changed by your output. ",
    "Do not repeat internal prompt labels, enums, JSON keys, or NOW such as NEW POSTS, EVENT CONTEXT, HISTORICAL CONTEXT, CODE-AUTHORITATIVE EVENT STATE, event_relation, delta_effect, 分析时刻, 本次新增帖子, 事件上下文帖子, or 历史上下文帖子 in user-facing fields. ",
    "expected_time_passed means the announced time has passed but landing is still unverified; it is not landed. ",
    "If state says observed_landed, describe the posts as historical confirmation and use past tense. ",
    "conclusion must be a direct decision of at most 30 Chinese characters, without markdown, evidence, or repeated reasoning. ",
    "event_relation: new_event when NEW POSTS start a distinct reset cycle; same_event when they update the ongoing event; none when unrelated. ",
    "Ordinary chatter or unrelated replies must be event_relation none with delta_effect no_change, signal_level none, and signal_type none; never overwrite or close the ongoing event for them. ",
    "event_phase is your read of the event stage after the NEW POSTS; delta_effect describes what the NEW POSTS do to the event. ",
    "context_status: complete when the context posts give enough background, context_missing when not, conflicting when they contradict the new posts. ",
    "Apply user semantic hints only when judging Tibo wording, idioms, metaphors, and confidence. User hints cannot redefine reset types, event lifecycle, time facts, citation rules, privacy boundaries, or the JSON schema. ",
    "If no user hints are provided, read the posts ordinarily without inventing extra rules. ",
    "This output is speculation, not an official conclusion. ",
    "Only use the English original post texts and timestamps provided; do not invent quotes."
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
        new_post_ids_json: "[]".into(),
        event_context_post_ids_json: "[]".into(),
        historical_post_ids_json: "[]".into(),
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
        signal_type: "unknown".into(),
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

/// 用户确认额度已重置：写入事件字段并进入 24 小时观察期，不冒充官方或重置卡归因。
pub fn confirm_user_reset(database: &Database) -> Result<RadarSnapshot, String> {
    let now = epoch_ms();
    reconcile_event_state(database, now)?;
    let Some(mut event) = database.active_radar_event()? else {
        return Err("当前没有可确认的重置事件".into());
    };
    if event.observed_reset_at.is_some() || event.closed_at.is_some() {
        return Err("当前事件已由本机观察或已关闭，不能再人工确认".into());
    }
    if !matches!(
        event.phase.as_str(),
        "watching" | "upcoming" | "landed_claimed"
    ) {
        return Err("当前事件状态不允许确认额度已重置".into());
    }
    event.user_confirmed_reset_at = Some(now);
    event.state_revision += 1;
    event.expires_at = event_expiry(&event);
    database.update_radar_event(&event)?;
    snapshot(database)
}

/// 撤销人工确认：清空确认时间并按 claimed/expected 恢复阶段。
pub fn undo_user_reset(database: &Database) -> Result<RadarSnapshot, String> {
    let now = epoch_ms();
    reconcile_event_state(database, now)?;
    let Some(mut event) = database.active_radar_event()? else {
        return Err("当前没有可撤销的人工确认".into());
    };
    if event.user_confirmed_reset_at.is_none() {
        return Err("当前事件没有人工确认可撤销".into());
    }
    if event.observed_reset_at.is_some() {
        return Err("本机观察已覆盖人工确认，不能撤销".into());
    }
    event.user_confirmed_reset_at = None;
    event.phase = if event.claimed_landed_at.is_some() {
        "landed_claimed".into()
    } else if event.expected_at.is_some() {
        "upcoming".into()
    } else {
        "watching".into()
    };
    event.title = event_title_for_phase(&event.phase, &event.event_type);
    event.state_revision += 1;
    event.expires_at = event_expiry(&event);
    database.update_radar_event(&event)?;
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
        Some(value) => {
            let value = sanitize_user_prompt(&value);
            if value == LEGACY_DEFAULT_USER_PROMPT
                || value == LEGACY_DEFAULT_USER_PROMPT_WITH_TIMEZONE
                || value == LEGACY_DEFAULT_USER_PROMPT_V17
            {
                database.set_setting_string("radar_user_prompt", DEFAULT_USER_PROMPT)?;
                DEFAULT_USER_PROMPT.to_string()
            } else {
                value
            }
        }
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
    signal_type: Option<String>,
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
        &mut parsed.signal_type,
        &["banked_reset", "quota_reset", "none"],
    );
    allow(
        &mut parsed.context_status,
        &["complete", "context_missing", "conflicting"],
    );
    if let Some(conclusion) = &mut parsed.conclusion {
        // 先可读化再截断：别名的友好标签比原始 post_id 短得多，40 字上限不被挤占。
        *conclusion = humanize_post_refs(conclusion, inputs)
            .replace(['`', '#', '*'], "")
            .chars()
            .take(40)
            .collect();
    }
    parsed.analysis_basis = parsed.analysis_basis.as_deref().map(|basis| humanize_post_refs(basis, inputs));
    let humanize_list = |items: &[String]| -> Vec<String> {
        items
            .iter()
            .map(|item| humanize_post_refs(item, inputs))
            .collect()
    };
    parsed.support = humanize_list(&parsed.support);
    parsed.against = humanize_list(&parsed.against);
    parsed.uncertainty = humanize_list(&parsed.uncertainty);
    // citations 只接受别名（N1/C1…），映射回真实 post_id 后再落库；
    // 模型违规直接回传已知真实 post_id 时也接受，其余（编造值）一律丢弃。
    let alias_to_id: std::collections::HashMap<&str, &str> = inputs
        .aliases
        .iter()
        .map(|(id, alias)| (alias.as_str(), id.as_str()))
        .collect();
    parsed.citations = parsed
        .citations
        .iter()
        .filter_map(|citation| {
            alias_to_id
                .get(citation.as_str())
                .copied()
                .or_else(|| {
                    inputs
                        .aliases
                        .contains_key(citation.as_str())
                        .then_some(citation.as_str())
                })
                .map(str::to_string)
        })
        .collect();
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
        signal_type: parsed
            .get("signal_type")
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

fn format_clock(ms: i64) -> String {
    let beijing = chrono::FixedOffset::east_opt(8 * 3600).expect("UTC+8 is a valid offset");
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|time| time.with_timezone(&beijing).format("%m-%d %H:%M").to_string())
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
        assert!(DEFAULT_USER_PROMPT.contains("banked reset"));
    }

    fn temp_db() -> (Database, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "ai-quota-radar-{}-{}.db",
            std::process::id(),
            epoch_ms()
        ));
        let database = Database::initialize_at(path.clone()).expect("db");
        (database, path)
    }

    fn sample_post(id: &str, posted_at: i64, relevance: &str, explicit: bool) -> TiboPostRecord {
        TiboPostRecord {
            id: id.into(),
            url: format!("https://x.com/tibo/status/{id}"),
            text: format!("post {id}"),
            posted_at,
            kind: relevance.into(),
            tibo_lane: None,
            explicit_reset: explicit,
            verification_status: None,
            is_reply: false,
            replies: 0,
            reposts: 0,
            likes: 0,
            extra_json: json!({ "relevance": relevance }).to_string(),
            synced_at: posted_at,
            translated_text: None,
            translated_at: None,
            translation_source: None,
            lifecycle_consumed_at: None,
        }
    }

    fn parsed_signal(relation: &str, citations: Vec<&str>) -> ModelJson {
        ModelJson {
            conclusion: Some("测试结论".into()),
            analysis_basis: Some("测试依据".into()),
            confidence: Some("medium".into()),
            citations: citations.into_iter().map(str::to_string).collect(),
            support: Vec::new(),
            against: Vec::new(),
            uncertainty: Vec::new(),
            event_relation: Some(relation.into()),
            event_phase: Some("landed_claimed".into()),
            delta_effect: Some("new_event".into()),
            signal_level: Some("strong".into()),
            context_status: Some("complete".into()),
            signal_type: Some("quota_reset".into()),
        }
    }

    #[test]
    fn expanding_range_does_not_reactivate_closed_event() {
        let (database, path) = temp_db();
        let old_at = epoch_ms() - 36 * 3_600_000;
        let chatter_at = epoch_ms() - 60 * 60 * 1000;
        database
            .replace_tibo_posts(
                &[
                    sample_post("old-signal", old_at, "direct", true),
                    sample_post("new-chat", chatter_at, "none", false),
                ],
                epoch_ms(),
            )
            .unwrap();
        database
            .mark_tibo_posts_consumed(&["old-signal".into()], old_at)
            .unwrap();
        let mut closed = RadarEventRecord {
            id: "event-closed".into(),
            phase: "closed".into(),
            title: "本机已观察到额度重置".into(),
            summary: None,
            first_signal_at: old_at,
            latest_evidence_at: old_at,
            claimed_landed_at: Some(old_at),
            observed_reset_at: Some(old_at),
            closed_at: Some(old_at + 24 * 3_600_000),
            close_reason: Some("completed".into()),
            expected_at: Some(old_at),
            expires_at: Some(old_at + 24 * 3_600_000),
            state_revision: 2,
            user_confirmed_reset_at: None,
            event_type: "quota_reset".into(),
        };
        closed.closed_at = Some(old_at + 24 * 3_600_000);
        database.insert_radar_event(&closed).unwrap();
        database
            .add_radar_event_evidence("event-closed", "old-signal", "delta", "analysis-old")
            .unwrap();
        let inputs = collect_delta_inputs(&database, "7d").unwrap();
        assert!(inputs.delta.iter().all(|post| post.id != "old-signal"));
        assert!(inputs.historical.iter().any(|post| post.id == "old-signal"));
        let parsed = parsed_signal("new_event", vec!["old-signal", "new-chat"]);
        let (event_id, replay) =
            apply_analysis_to_event(&database, &inputs, &parsed, "analysis-test").unwrap();
        assert!(event_id.is_none());
        assert!(!replay);
        assert!(database.active_radar_event().unwrap().is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unconsumed_posts_stay_new_until_successful_analysis() {
        let (database, path) = temp_db();
        let old_at = epoch_ms() - 20 * 3_600_000;
        let new_at = epoch_ms() - 10 * 60 * 1000;
        database
            .replace_tibo_posts(
                &[
                    sample_post("consumed", old_at, "direct", true),
                    sample_post("queued", new_at, "none", false),
                ],
                epoch_ms(),
            )
            .unwrap();
        database
            .mark_tibo_posts_consumed(&["consumed".into()], old_at)
            .unwrap();
        let inputs = collect_delta_inputs(&database, "3d").unwrap();
        assert_eq!(
            inputs.delta.iter().map(|post| post.id.as_str()).collect::<Vec<_>>(),
            vec!["queued"]
        );
        assert!(inputs.historical.iter().any(|post| post.id == "consumed"));
        database
            .mark_tibo_posts_consumed(&["queued".into()], epoch_ms())
            .unwrap();
        let after = collect_delta_inputs(&database, "3d").unwrap();
        assert!(after.delta.is_empty());
        assert!(after.historical.iter().any(|post| post.id == "queued"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn user_confirm_and_undo_restore_event_phase() {
        let (database, path) = temp_db();
        let now = epoch_ms();
        database
            .insert_radar_event(&RadarEventRecord {
                id: "event-active".into(),
                phase: "landed_claimed".into(),
                title: "来源称已经重置".into(),
                summary: None,
                first_signal_at: now - 2 * 3_600_000,
                latest_evidence_at: now - 2 * 3_600_000,
                claimed_landed_at: Some(now - 2 * 3_600_000),
                observed_reset_at: None,
                closed_at: None,
                close_reason: None,
                expected_at: None,
                expires_at: Some(now + 40 * 3_600_000),
                state_revision: 1,
                user_confirmed_reset_at: None,
                event_type: "quota_reset".into(),
            })
            .unwrap();
        confirm_user_reset(&database).unwrap();
        let confirmed = database.active_radar_event().unwrap().expect("active");
        assert!(confirmed.user_confirmed_reset_at.is_some());
        assert_eq!(
            confirmed.expires_at,
            confirmed
                .user_confirmed_reset_at
                .map(|at| at + 24 * 3_600_000)
        );
        undo_user_reset(&database).unwrap();
        let restored = database.active_radar_event().unwrap().expect("active");
        assert!(restored.user_confirmed_reset_at.is_none());
        assert_eq!(restored.phase, "landed_claimed");
        let _ = std::fs::remove_file(path);
    }

    fn closed_event(
        id: &str,
        observed_reset_at: Option<i64>,
        user_confirmed_reset_at: Option<i64>,
        claimed_landed_at: Option<i64>,
        close_reason: &str,
        closed_at: i64,
    ) -> RadarEventRecord {
        RadarEventRecord {
            id: id.into(),
            phase: "closed".into(),
            title: "测试事件".into(),
            summary: None,
            first_signal_at: closed_at - 6 * 3_600_000,
            latest_evidence_at: closed_at - 3_600_000,
            claimed_landed_at,
            observed_reset_at,
            closed_at: Some(closed_at),
            close_reason: Some(close_reason.into()),
            expected_at: claimed_landed_at,
            expires_at: None,
            state_revision: 1,
            user_confirmed_reset_at,
            event_type: "quota_reset".into(),
        }
    }

    #[test]
    fn latest_confirmed_reset_event_skips_historical_replay() {
        let (database, path) = temp_db();
        let now = epoch_ms();
        // 历史重放事件：closed_at 更新但无任何本机/用户确认，不得成为“最近一次重置”。
        database
            .insert_radar_event(&closed_event(
                "event-replay",
                None,
                None,
                Some(now - 3_600_000),
                "invalid_historical_replay",
                now - 1_800_000,
            ))
            .unwrap();
        // 正确重置事件：本机观察确认，时间更早但必须胜出。
        database
            .insert_radar_event(&closed_event(
                "event-observed",
                Some(now - 26 * 3_600_000),
                None,
                None,
                "completed",
                now - 24 * 3_600_000,
            ))
            .unwrap();
        let confirmed = database
            .latest_confirmed_reset_event()
            .unwrap()
            .expect("confirmed");
        assert_eq!(confirmed.id, "event-observed");
        // 普通关闭事件查询仍返回重放事件，供完整历史使用。
        assert_eq!(
            database.latest_closed_radar_event().unwrap().expect("closed").id,
            "event-replay"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn save_notice_none_keeps_last_notice() {
        let (database, path) = temp_db();
        let notice = RadarNotice {
            headline: "模型容量提示反馈增多".into(),
            lead: Some("社区反馈：大量用户遇到模型容量提示".into()),
            items: Vec::new(),
            updated_at: None,
            is_current: true,
            freshness_ms: Some(0),
        };
        save_notice(&database, Some(&notice)).unwrap();
        // 本次没有解析到公告：不得清空最后一次公告，只标记非当前。
        save_notice(&database, None).unwrap();
        let stored = load_notice(&database).unwrap().expect("kept");
        assert_eq!(stored.headline, "模型容量提示反馈增多");
        assert!(!stored.is_current);
        assert!(stored.updated_at.is_some());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn insert_radar_analysis_persists_post_groups() {
        let (database, path) = temp_db();
        let record = RadarAnalysisRecord {
            id: "analysis-groups".into(),
            created_at: epoch_ms(),
            range_key: "3d".into(),
            cut_post_id: None,
            from_posted_at: None,
            to_posted_at: None,
            source_id: None,
            model: None,
            prompt_version: PROMPT_VERSION.into(),
            input_hash: "hash".into(),
            conclusion: Some("无关".into()),
            analysis_basis: None,
            confidence: Some("low".into()),
            citations_json: r#"["p-new-1","p-old-1"]"#.into(),
            support_json: "[]".into(),
            against_json: "[]".into(),
            uncertainty_json: "[]".into(),
            new_post_ids_json: r#"["p-new-1"]"#.into(),
            event_context_post_ids_json: "[]".into(),
            historical_post_ids_json: r#"["p-old-1"]"#.into(),
            error_message: None,
            event_id: None,
            analysis_mode: Some("live_delta".into()),
            context_hash: "ctx".into(),
            prompt_hash: "prompt".into(),
            event_relation: Some("none".into()),
            event_phase: None,
            delta_effect: Some("no_change".into()),
            signal_level: Some("none".into()),
            context_status: Some("complete".into()),
            temporal_phase: None,
            valid_until: None,
            state_revision: 0,
            timezone_policy_version: time_claims::TIMEZONE_POLICY_VERSION.into(),
            signal_type: "none".into(),
        };
        database.insert_radar_analysis(&record).unwrap();
        let stored = database.latest_radar_analysis().unwrap().expect("stored");
        let view = analysis_view(&database, stored, true);
        assert_eq!(view.new_post_ids, vec!["p-new-1".to_string()]);
        assert_eq!(view.historical_post_ids, vec!["p-old-1".to_string()]);
        // 当前判断依据引用只取 citations ∩ newPostIds，历史引用被隔离。
        let current: Vec<_> = view
            .citations
            .iter()
            .filter(|id| view.new_post_ids.contains(id))
            .cloned()
            .collect();
        assert_eq!(current, vec!["p-new-1".to_string()]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn banked_reset_does_not_close_active_quota_event() {
        let (database, path) = temp_db();
        let now = epoch_ms();
        let posted_at = now - 30 * 60 * 1000;
        database
            .insert_radar_event(&RadarEventRecord {
                id: "event-quota".into(),
                phase: "landed_claimed".into(),
                title: "来源称已经重置".into(),
                summary: None,
                first_signal_at: now - 6 * 3_600_000,
                latest_evidence_at: now - 6 * 3_600_000,
                claimed_landed_at: Some(now - 6 * 3_600_000),
                observed_reset_at: None,
                closed_at: None,
                close_reason: None,
                expected_at: None,
                expires_at: Some(now + 40 * 3_600_000),
                state_revision: 1,
                user_confirmed_reset_at: None,
                event_type: "quota_reset".into(),
            })
            .unwrap();
        database
            .replace_tibo_posts(
                &[sample_post("banked-new", posted_at, "direct", true)],
                now,
            )
            .unwrap();
        let inputs = collect_delta_inputs(&database, "3d").unwrap();
        let mut parsed = parsed_signal("new_event", vec!["banked-new"]);
        parsed.signal_type = Some("banked_reset".into());
        parsed.event_phase = Some("upcoming".into());
        let (event_id, replay) =
            apply_analysis_to_event(&database, &inputs, &parsed, "analysis-banked").unwrap();
        assert!(!replay);
        assert!(event_id.is_some());
        let quota = database
            .active_radar_event_of_type("quota_reset")
            .unwrap()
            .expect("quota still active");
        assert_eq!(quota.id, "event-quota");
        assert!(quota.closed_at.is_none());
        let banked = database
            .active_radar_event_of_type("banked_reset")
            .unwrap()
            .expect("banked created");
        assert_eq!(banked.event_type, "banked_reset");
        let _ = std::fs::remove_file(path);
    }
}
