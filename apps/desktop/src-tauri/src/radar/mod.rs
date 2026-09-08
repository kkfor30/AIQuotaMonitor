//! GPT 重置雷达：从 CodexRadar 公开首页同步 Tibo 动态，并保留可选 AI 分析。
//! 与平台额度域隔离，失败不改写 Source 聚合状态。
//! 三路证据并行：CodexRadar 来源判断 / AI 增量分析 / 本机额度观察（quota_watch）。

mod application;
mod codexradar;
mod quota_watch;
mod repair;
mod time_claims;
mod willcodex;

use crate::refresh::RefreshCoordinator;
use crate::storage::database::Database;
use crate::storage::repository::{
    RadarAnalysisRecord, RadarChatEndpointRecord, RadarCheckRecord, RadarEventRecord,
    RadarTimeClaimRecord, SourceRecord, TiboPostRecord,
};
use crate::storage::vault;
use quota_watch::{BankedGrantView, QuotaVerificationView};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::error::Error;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Notify;
use tauri::{Emitter, Manager};

pub const FEED_URL: &str = "https://codexradar.com/";
pub const WILLCODEX_FEED_URL: &str = "https://www.willcodexquotareset.com/api/forecast";
pub const RADAR_SOURCE_CODEXRADAR: &str = "codexradar";
pub const RADAR_SOURCE_WILLCODEX: &str = "willcodex";
/// 监控/分析候选窗口：最近 72 小时内尚未消费的材料作为新事件候选。
/// 浏览筛选（当天/3 天/自定义）只影响前端展示，不改变监控范围。
pub const MONITOR_WINDOW_MS: i64 = 72 * 3_600_000;
/// 分析记录的输入范围标签：浏览/分析拆分后所有新分析都是固定监控窗口。
pub const MONITOR_RANGE_KEY: &str = "monitor:72h";
/// 后台自动检查固定间隔：15 分钟；应用内部调度，不依赖外部计划任务。
pub const RADAR_BACKGROUND_INTERVAL_MS: i64 = 15 * 60_000;

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
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
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
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.generation.load(Ordering::Relaxed) != my_generation {
                return;
            }
            notified.await;
        }
    }
}

struct CheckGuard<'a> {
    control: &'a RadarControl,
    app: &'a tauri::AppHandle,
}

impl Drop for CheckGuard<'_> {
    fn drop(&mut self) {
        self.control.finish();
        let _ = self.app.emit("radar-data-changed", ());
        let _ = self.app.emit("radar-check-finished", ());
    }
}

/// All entry points share cancellation, scheduling and lock lifetime.
pub async fn run_controlled_check(
    app: &tauri::AppHandle,
    database: &Database,
    coordinator: &RefreshCoordinator,
    control: &RadarControl,
    analyze: bool,
    source_id: Option<&str>,
    model: Option<&str>,
) -> Result<RadarSnapshot, String> {
    if !control.try_begin() {
        return Err("已有检查正在进行".into());
    }
    let guard = CheckGuard { control, app };
    let generation = control.generation.load(Ordering::Acquire);
    let started = epoch_ms();
    set_background_next_check_at(database, started + RADAR_BACKGROUND_INTERVAL_MS)?;
    let _ = app.emit("radar-check-started", ());
    let result = tokio::select! {
        result = run_check(database, coordinator, analyze, source_id, model, None, Some(app)) => result,
        _ = control.wait_cancelled(generation) => {
            database.insert_radar_check(&RadarCheckRecord {
                id: format!("radar-cancel-{started}"), started_at: started,
                finished_at: Some(epoch_ms()), status: "cancelled".into(),
                sync_status: None, parse_status: None, analyze_status: Some("cancelled".into()),
                error_message: None, post_count: 0,
            })?;
            Err("已终止本次检查".into())
        },
    };
    drop(guard);
    result.map(|mut value| { value.check_running = false; value })
}
pub const PROMPT_VERSION: &str = "radar-v21";
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
const LEGACY_DEFAULT_USER_PROMPT_V18: &str = "请把帖子分成三类信号，不要混用：

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
const LEGACY_DEFAULT_USER_PROMPT_V18_NO_INTERNAL_FIELDS: &str = "请把帖子分成三类信号，不要混用：

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

普通闲聊中偶然出现相同单词，不代表一定存在重置信号。";
pub const DEFAULT_USER_PROMPT: &str = "请把帖子分成三类信号，不要混用：

【重置卡 banked_reset】
原帖在说可保存、可稍后手动使用的重置次数或重置卡发放/到账。
常见英文：banked reset、one reset per day、first one will land、reset available、reset card。
重置卡不是全局额度自动恢复，但仍是有效重置信号，不得判为 none。
中文只写「重置卡」，不要写成「银行重置」；此处 bank 是积存，不是银行。

【额度重置 quota_reset】
原帖在说 Codex / ChatGPT Work 等额度窗口实际刷新或恢复。
常见英文：reset all paid Codex/ChatGPT Work usage、full reset、reset usage、usage has reset。

【无信号 none】
普通闲聊、回复、表情，或只是顺口提到 reset，没有重置卡或额度窗口含义。

也可继续关注 Tibo 的特殊表达，例如：
- Hold on to your Codex、reset will land
- 仪表盘（dashboard）、里程碑（milestone）、庆祝（celebration）、倒计时、按钮已经按下

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
    pub context: Option<String>,
    pub source: Option<String>,
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

/// 单个雷达来源（codexradar / willcodex）的同步状态；一个来源失败不影响另一来源。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarSourceStatusView {
    pub source_id: String,
    pub display_name: String,
    pub last_check_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub last_error: Option<String>,
    /// fresh | stale | missing：按最近成功时间与两倍后台间隔判断，不由 AI 状态或帖子年龄决定。
    pub freshness: String,
}

/// 历史记录：额度重置观察事实（独立保存后与事件关联）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaObservationHistoryView {
    pub id: i64,
    pub account_id: String,
    pub account_name: String,
    pub classification: String,
    pub observed_at: i64,
    pub temporal_correlation: String,
    pub user_confirmed_at: Option<i64>,
}

/// 历史记录：本机重置卡数量变化观察。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BankedObservationHistoryView {
    pub account_id: String,
    pub account_name: String,
    pub previous_count: i64,
    pub current_count: i64,
    pub observed_at: i64,
    /// grant = 数量增加（到账）；drop = 数量减少（不能单独断言已使用）。
    pub kind: String,
}

/// 历史记录页的单个事件：事件本体 + 证据 + 观察 + 当时的分析。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarHistoryEventView {
    pub event: RadarEventView,
    pub evidence_posts: Vec<TiboPostView>,
    pub analyses: Vec<RadarAnalysisView>,
    pub analyses_next_cursor: Option<String>,
    pub quota_observations: Vec<QuotaObservationHistoryView>,
    pub banked_observations: Vec<BankedObservationHistoryView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarHistoryView {
    pub events: Vec<RadarHistoryEventView>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
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
    /// 关闭原因（timeout_unverified/completed/出现新一轮重置信号等），历史页结果标签用。
    pub close_reason: Option<String>,
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
    /// 最近一次本机观察到的重置卡发放（数量增加）；不得写入 recent_reset。
    pub recent_banked_grant: Option<BankedGrantView>,
    /// 最近一次本机观察到的重置卡数量减少；不得单独断言已使用，也不得写入 recent_reset。
    pub recent_banked_decrease: Option<BankedGrantView>,
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
    pub other_active_event_ids: Vec<String>,
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
    /// platform = 平台中心 API Key；endpoint = 雷达独立对话接入。
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarChatEndpointView {
    pub id: String,
    pub source_id: String,
    pub display_name: String,
    pub api_base_url: String,
    pub models: Vec<String>,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarSnapshot {
    pub check_running: bool,
    pub source_status: String,
    /// 分来源同步状态：一个来源失败不清除另一来源的成功数据。
    pub sources: Vec<RadarSourceStatusView>,
    pub last_synced_at: Option<i64>,
    pub posts: Vec<TiboPostView>,
    pub latest: Option<TiboPostView>,
    pub checks: Vec<RadarCheckView>,
    pub analysis: Option<RadarAnalysisView>,
    pub models: Vec<RadarModelOption>,
    pub analysis_prefs: RadarAnalysisPrefs,
    pub notice: Option<RadarNotice>,
    /// 用户隐藏 CodexRadar 公告细条；不影响同步，主窗口与悬浮页共用。
    pub notice_hidden: bool,
    pub source_assessment: RadarSourceAssessmentView,
    pub event: Option<RadarEventView>,
    pub ai_assessment: RadarAiAssessmentView,
    pub decision: RadarDecisionView,
    pub quota_verifications: Vec<QuotaVerificationView>,
    pub analysis_groups: RadarAnalysisGroupsView,
    pub chat_endpoints: Vec<RadarChatEndpointView>,
    /// 事件维度的历史记录（含落地观察、当时的分析与技术检查在 UI 次级展示）。
    pub history: RadarHistoryView,
    /// All live events; `decision.activeEventId` is the primary judgment.
    pub active_events: Vec<RadarEventView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarAnalysisPrefs {
    pub analyze: bool,
    pub source_id: Option<String>,
    pub model: Option<String>,
    pub user_prompt: String,
    pub default_user_prompt: String,
    /// 后台自动检查（固定 15 分钟间隔）：应用运行期间独立检查来源并按需分析；默认开启。
    pub background_check: bool,
    /// 下次后台检查到期时间（epoch 毫秒）；仅展示用。
    pub background_next_at: Option<i64>,
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
    let sources = source_status_views(database, &checks)?;
    let source_status = overall_source_status(&sources, posts.is_empty(), &checks);
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
    let active_event = primary_live_event(&live_events).cloned();
    let active_event_views: Vec<_> = live_events.iter().map(|record| event_view(database, record)).collect();
    let event_view_data = active_event
        .as_ref()
        .map(|record| event_view(database, record));
    let prefs = load_analysis_prefs(database)?;
    let groups = classify_analysis_posts(database, &posts, &live_events)?;
    let ai_assessment = build_ai_assessment(
        database,
        analysis.as_ref(),
        active_event.as_ref(),
        &groups,
        &checks,
        prefs.analyze,
    )?;
    let mut decision = build_decision(
        database,
        active_event.as_ref(),
        &ai_assessment,
        event_view_data.as_ref(),
        &posts,
        &groups,
        prefs.analyze,
        &checks,
        &sources,
    )?;
    decision.other_active_event_ids = live_events
        .iter()
        .filter(|event| Some(event.id.as_str()) != decision.active_event_id.as_deref())
        .map(|event| event.id.clone())
        .collect();
    let history = history_view(database)?;
    Ok(RadarSnapshot {
        check_running: false,
        source_status: source_status.into(),
        sources,
        last_synced_at,
        posts,
        latest,
        checks,
        analysis,
        models: chat_models(database)?,
        analysis_prefs: prefs,
        notice,
        notice_hidden: database.setting_bool("radar_notice_hidden")?,
        source_assessment,
        event: event_view_data,
        ai_assessment,
        decision,
        quota_verifications,
        analysis_groups: groups_view(&groups),
        chat_endpoints: chat_endpoint_views(database)?,
        history,
        active_events: active_event_views,
    })
}

/// 分来源状态视图：优先读 radar_source_status（v15）；从未检查过的来源按 missing 展示。
fn source_status_views(
    database: &Database,
    checks: &[RadarCheckView],
) -> Result<Vec<RadarSourceStatusView>, String> {
    let records = database.radar_source_statuses()?;
    let mut views = Vec::new();
    for (source_id, display_name) in [
        (RADAR_SOURCE_CODEXRADAR, "CodexRadar"),
        (RADAR_SOURCE_WILLCODEX, "WillCodex"),
    ] {
        let record = records.iter().find(|item| item.source_id == source_id);
        let last_success_at = record.and_then(|item| item.last_success_at);
        let last_check_at = record.and_then(|item| item.last_check_at);
        let last_error = record.and_then(|item| item.last_error.clone());
        let freshness = match last_success_at {
            Some(at) if last_error.is_none() && epoch_ms() - at <= 2 * RADAR_BACKGROUND_INTERVAL_MS => "fresh",
            Some(_) => "stale",
            None => "missing",
        };
        views.push(RadarSourceStatusView {
            source_id: source_id.into(),
            display_name: display_name.into(),
            last_check_at,
            last_success_at,
            // 拉取失败保留上次成功结果；错误只在最近一次检查失败时挂出，不覆盖成功时间。
            last_error: last_error.filter(|_| {
                last_success_at.is_none_or(|at| last_check_at.is_some_and(|check| check >= at))
            }),
            freshness: freshness.into(),
        });
    }
    // v15 之前的旧库：来源状态表为空时按检查日志回退，避免升级后首轮全部显示“从未检查”。
    if views.iter().all(|view| view.last_check_at.is_none()) {
        if let Some(latest) = checks.first() {
            for view in &mut views {
                view.last_check_at = Some(latest.started_at);
                view.freshness = "unknown".into();
            }
        }
    }
    Ok(views)
}

/// 整体来源状态：任一来源 fresh 即 fresh；有数据但无来源 fresh 为 stale；无数据 missing。
fn overall_source_status(
    sources: &[RadarSourceStatusView],
    no_posts: bool,
    checks: &[RadarCheckView],
) -> &'static str {
    if sources.iter().any(|source| source.last_error.is_some()) {
        return "stale";
    }
    if sources.iter().any(|source| source.freshness == "fresh") {
        return "fresh";
    }
    if no_posts {
        // 从未同步成功且最近一次检查失败：保持既有 failed/stale 语义供 UI 提示。
        return if checks.first().is_some_and(|check| check.status == "failed") {
            "stale"
        } else {
            "missing"
        };
    }
    "stale"
}

/// 历史记录视图：事件（活动在前）+ 证据帖 + 关联观察 + 当时的分析。
fn history_view(database: &Database) -> Result<RadarHistoryView, String> {
    history_view_page(database, None, None, 20)
}

fn history_view_page(
    database: &Database,
    after_sort_at: Option<i64>,
    after_id: Option<&str>,
    limit: usize,
) -> Result<RadarHistoryView, String> {
    let events = database.radar_events_page(after_sort_at, after_id, limit + 1)?;
    let has_more = events.len() > limit;
    let events: Vec<_> = events.into_iter().take(limit).collect();
    let next_cursor = if has_more {
        events.last().map(|event| {
            format!("{}:{}", event.closed_at.unwrap_or(event.latest_evidence_at), event.id)
        })
    } else {
        None
    };
    let accounts = account_names(database)?;
    let mut views = Vec::with_capacity(events.len());
    for record in events {
        let post_ids = database.radar_event_post_ids(&record.id)?;
        let evidence_posts = database
            .tibo_posts_by_ids(&post_ids)?
            .into_iter()
            .map(to_view)
            .collect();
        let analysis_page = list_event_analyses(database,&record.id,None,20)?;
        let analyses = analysis_page.items;
        let analyses_next_cursor = analysis_page.next_cursor;
        let quota_observations = database
            .quota_reset_observations(None, Some(&record.id))?
            .into_iter()
            .map(|item| QuotaObservationHistoryView {
                account_name: accounts
                    .get(&item.account_id)
                    .cloned()
                    .unwrap_or_default(),
                id: item.id,
                account_id: item.account_id,
                classification: item.classification,
                observed_at: item.observed_at,
                temporal_correlation: item.temporal_correlation,
                user_confirmed_at: item.user_confirmed_at,
            })
            .collect();
        let banked_observations = database
            .banked_reset_observations_for_event(&record.id)?
            .into_iter()
            .map(|item| BankedObservationHistoryView {
                account_name: accounts
                    .get(&item.account_id)
                    .cloned()
                    .unwrap_or_default(),
                account_id: item.account_id,
                previous_count: item.previous_count,
                current_count: item.current_count,
                observed_at: item.observed_at,
                kind: if item.current_count > item.previous_count {
                    "grant".into()
                } else {
                    "drop".into()
                },
            })
            .collect();
        views.push(RadarHistoryEventView {
            event: event_view(database, &record),
            evidence_posts,
            analyses,
            analyses_next_cursor,
            quota_observations,
            banked_observations,
        });
    }
    Ok(RadarHistoryView { events: views, next_cursor, has_more })
}

fn account_names(database: &Database) -> Result<std::collections::HashMap<String, String>, String> {
    let mut names = std::collections::HashMap::new();
    for source in database.openai_quota_sources()? {
        names.entry(source.account_id.clone())
            .or_insert_with(|| source.account_name.clone());
    }
    Ok(names)
}

pub fn reconcile_event_state(database: &Database, now: i64) -> Result<(), String> {
    refresh_time_claims(database)?;
    let events = database.active_radar_events()?;
    let quota_event = events
        .iter()
        .find(|event| event.event_type != "banked_reset")
        .cloned();
    let banked_event = events
        .iter()
        .find(|event| event.event_type == "banked_reset")
        .cloned();
    quota_watch::materialize_observations(database, quota_event.as_ref())?;
    quota_watch::materialize_banked_grants(database, banked_event.as_ref())?;
    // 幂等关联：先观察到变化、后识别出事件时，把此前独立保存的观察补挂到事件上；
    // 重复执行只更新 event_id IS NULL 的行，不产生重复记录，不重新触发旧信号。
    link_pending_observations(database, quota_event.as_ref(), banked_event.as_ref())?;
    for mut event in events {
        reconcile_one_event(database, &mut event, now)?;
    }
    Ok(())
}

/// 观察关联回看窗口：事件首次信号前 24 小时内的观察仍可能与事件相关
/// （先观察到变化、后识别出事件的场景）。
const OBSERVATION_LINK_LOOKBACK_MS: i64 = 24 * 3_600_000;

fn link_pending_observations(
    database: &Database,
    quota_event: Option<&RadarEventRecord>,
    banked_event: Option<&RadarEventRecord>,
) -> Result<(), String> {
    let mut quota_targets: Vec<RadarEventRecord> = quota_event.cloned().into_iter().collect();
    let mut banked_targets: Vec<RadarEventRecord> = banked_event.cloned().into_iter().collect();
    let since_closed = epoch_ms() - 7 * 86_400_000;
    for closed in database.recently_closed_radar_events(since_closed)? {
        if closed.event_type == "banked_reset" {
            banked_targets.push(closed);
        } else {
            quota_targets.push(closed);
        }
    }
    for event in &quota_targets {
        let since = event.first_signal_at - OBSERVATION_LINK_LOOKBACK_MS;
        for observation in database.quota_reset_observations_pending_link(since)? {
            if observation.classification == "unscheduled_reset"
                && quota_watch::confirms_landing(event, observation.observed_at)
                && database.observation_pair_is_contiguous(observation.previous_snapshot_id,observation.current_snapshot_id)? {
                let correlation = quota_watch::correlation_for(event, observation.observed_at);
                database.set_quota_reset_observation_event(observation.id, &event.id, correlation)?;
            }
        }
    }
    for event in &banked_targets {
        for observation in database.banked_reset_observations(None)? {
            if observation.event_id.is_none() && observation.current_count > observation.previous_count
                && quota_watch::confirms_landing(event,observation.observed_at)
                && database.observation_pair_is_contiguous(observation.previous_snapshot_id,observation.current_snapshot_id)? {
                database.connect()?.execute("UPDATE banked_reset_observations SET event_id=?2 WHERE id=?1 AND event_id IS NULL",rusqlite::params![observation.id,event.id]).map_err(|e|e.to_string())?;
            }
        }
    }
    Ok(())
}

fn reconcile_one_event(
    database: &Database,
    event: &mut RadarEventRecord,
    now: i64,
) -> Result<(), String> {
    let original = event.clone();
    // expected_at is adopted from a versioned announcement; reconciliation never reselects old claims.
    if matches!(
        event.phase.as_str(),
        "watching" | "upcoming" | "landed_claimed"
    ) {
        let observed_at = if event.event_type == "banked_reset" {
            quota_watch::banked_reset_observed_at(database, event)?
        } else {
            quota_watch::confirming_quota_reset_at(database, event)?
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
    let now = epoch_ms();
    if let Err(error) = repair::repair_unapplied_signals(database, now) {
        log::error!("雷达历史恢复失败：{error}");
        return Err(format!("雷达历史恢复失败：{error}"));
    }
    if let Err(error) = repair::repair_damaged_conclusions(database, now) {
        log::error!("雷达历史结论修复失败：{error}");
    }
    reconcile_event_state(database, now)
}

fn refresh_time_claims(database: &Database) -> Result<(), String> {
    // 浏览分页（80 条）之外，监控窗口内未消费帖子也要解析时间声明，供分析引用。
    let mut posts = database.list_tibo_posts(80)?;
    let window_start = epoch_ms() - MONITOR_WINDOW_MS;
    for record in database.unconsumed_tibo_posts_since(window_start, 200)? {
        if !posts.iter().any(|post| post.id == record.id) {
            posts.push(record);
        }
    }
    for post in posts {
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
                claim_kind: claim.claim_kind,
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

fn primary_live_event(events: &[RadarEventRecord]) -> Option<&RadarEventRecord> {
    events
        .iter()
        .find(|event| {
            event.observed_reset_at.is_none()
                && event.user_confirmed_reset_at.is_none()
                && matches!(event.phase.as_str(), "watching" | "upcoming" | "landed_claimed")
        })
        .or_else(|| events.first())
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
    if let Ok(transitions) = database.radar_event_transitions(&record.id) {
        for (at, before_json, after_json, reason) in transitions {
            let before: serde_json::Value = serde_json::from_str(&before_json).unwrap_or_default();
            let after: serde_json::Value = serde_json::from_str(&after_json).unwrap_or_default();
            let before_expected = before.get("expectedAt").and_then(serde_json::Value::as_i64);
            let after_expected = after.get("expectedAt").and_then(serde_json::Value::as_i64);
            if before_expected != after_expected {
                timeline.push(RadarEventNodeView {
                    at,
                    kind: "time_revision".into(),
                    label: match (before_expected, after_expected) {
                        (_, None) => "时间承诺已撤销".into(),
                        (None, Some(next)) => format!("预计时间更正为 {}", format_clock(next)),
                        (Some(prev), Some(next)) if next < prev => {
                            format!("预计时间提前至 {}", format_clock(next))
                        }
                        (Some(_), Some(next)) => format!("预计时间延期至 {}", format_clock(next)),
                    },
                });
            } else if reason != "state_update" && !timeline.iter().any(|node| node.at == at && node.kind == "closed") {
                timeline.push(RadarEventNodeView {
                    at,
                    kind: "transition".into(),
                    label: reason,
                });
            }
        }
        timeline.sort_by_key(|node| node.at);
    }
    let post_ids = database
        .radar_event_post_ids(&record.id)
        .unwrap_or_default();
    RadarEventView {
        id: record.id.clone(),
        phase: record.phase.clone(),
        title: record.title.clone(),
        summary: record.summary.as_deref().map(rewrite_banked_reset_zh),
        first_signal_at: record.first_signal_at,
        latest_evidence_at: record.latest_evidence_at,
        claimed_landed_at: record.claimed_landed_at,
        observed_reset_at: record.observed_reset_at,
        closed_at: record.closed_at,
        close_reason: record.close_reason.clone(),
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
/// 覆盖判断只看监控窗口内是否还有未消费帖子，与浏览筛选无关。
fn build_ai_assessment(
    database: &Database,
    latest_success: Option<&RadarAnalysisView>,
    active_event: Option<&RadarEventRecord>,
    groups: &ClassifiedPosts,
    checks: &[RadarCheckView],
    enabled: bool,
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
    } else if latest_delta.is_none() {
        "pending"
    } else if latest_delta
        .as_ref()
        .is_some_and(|analysis| analysis.analysis_mode.as_deref() == Some("historical_replay"))
    {
        "historical"
    } else {
        "covered"
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
    checks: &[RadarCheckView],
    sources: &[RadarSourceStatusView],
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
    let mut status = if checks.is_empty() && posts.is_empty() {
        "unchecked"
    } else if sources.iter().all(|source| source.freshness == "missing") && posts.is_empty() {
        "source_unavailable"
    } else if ai_enabled && !groups.new_posts.is_empty() && active_event.is_none() {
        "pending_analysis"
    } else {
        "no_signal"
    };
    let mut headline = match status {
        "unchecked" => "尚未检查重置来源".to_string(),
        "source_unavailable" => "重置来源暂不可用".to_string(),
        "pending_analysis" if ai.state == "failed" => "材料已同步，分析失败".to_string(),
        "pending_analysis" => "已同步材料，等待分析".to_string(),
        _ => "暂无下一轮重置信号".to_string(),
    };
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
                    headline = event_analysis
                        .and_then(|analysis| analysis.conclusion.clone())
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| {
                            if banked {
                                "重置卡即将到账".into()
                            } else {
                                "预计即将重置".into()
                            }
                        });
                    time_kind = "expected";
                }
                _ => {
                    status = "watching";
                    headline = event_analysis
                        .and_then(|analysis| analysis.conclusion.clone())
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| {
                            if banked {
                                "重置卡可能即将到账".into()
                            } else {
                                "可能即将重置".into()
                            }
                        });
                    time_kind = "unknown";
                }
            }
        }
    }
    let time_basis = active_event.and_then(|event| {
        database.radar_event_time_basis(&event.id).ok().flatten()
    });
    let time_text = decision_time_text(
        status,
        event_type.as_deref(),
        expected_at,
        observed_at,
        claimed_at,
        user_confirmed_at,
        time_basis.as_ref(),
    );
    let verification_hint = match status {
        "upcoming" | "watching" if active_event.is_some() => Some("尚未确认实际落地".into()),
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
        expected_at,
        observed_at,
        claimed_at,
        user_confirmed_at,
        observation_expires_at,
        recent_reset.as_ref(),
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
        recent_banked_grant: quota_watch::latest_banked_grant(database)?,
        recent_banked_decrease: quota_watch::latest_banked_decrease(database)?,
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
        other_active_event_ids: Vec::new(),
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
    time_basis: Option<&(Option<String>, Option<String>, String, Option<String>, bool)>,
) -> String {
    let banked = event_type == Some("banked_reset");
    match status {
        "unchecked" | "source_unavailable" | "pending_analysis" | "no_signal" => String::new(),
        "watching" => {
            if expected_at.is_none() {
                if banked {
                    "到账时间尚未明确".into()
                } else {
                    "重置时间尚未明确".into()
                }
            } else {
                upcoming_time_text(expected_at, time_basis)
            }
        }
        "upcoming" => upcoming_time_text(expected_at, time_basis),
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
        _ => String::new(),
    }
}

fn upcoming_time_text(
    expected_at: Option<i64>,
    time_basis: Option<&(Option<String>, Option<String>, String, Option<String>, bool)>,
) -> String {
    let Some(at) = expected_at else {
        return "重置时间尚未明确".into();
    };
    let clock = format_clock_long(at);
    let zone = time_basis.and_then(|basis| basis.3.as_deref());
    let assumed = time_basis.is_some_and(|basis| basis.4);
    let approximate = time_basis.is_some_and(|basis| basis.2 == "approximate");
    let around = if approximate { " 左右" } else { "" };
    let tz = match (zone, assumed) {
        (Some("PST"), false) => "，按原文 PST 换算",
        (Some("PDT"), false) => "，按原文 PDT 换算",
        (Some("PT"), false) => "，按原文 PT 换算",
        (_, true) => "，按未标注时区假设为太平洋时间换算",
        _ => "",
    };
    format!("预计北京时间 {clock}{around}{tz}")
}

fn format_clock_long(ms: i64) -> String {
    let beijing = chrono::FixedOffset::east_opt(8 * 3600).expect("UTC+8 is a valid offset");
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|time| {
            let time = time.with_timezone(&beijing);
            format!(
                "{}月{}日 {:02}:{:02}",
                time.format("%m").to_string().trim_start_matches('0'),
                time.format("%d").to_string().trim_start_matches('0'),
                time.format("%H"),
                time.format("%M")
            )
        })
        .unwrap_or_else(|| ms.to_string())
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
    expected_at: Option<i64>,
    observed_at: Option<i64>,
    claimed_at: Option<i64>,
    user_confirmed_at: Option<i64>,
    observation_expires_at: Option<i64>,
    recent_reset: Option<&RadarRecentEventView>,
) -> (String, String, String, String) {
    // “最近重置”只来自本机观察/用户确认的 recentReset，禁止回退来源声称或关闭时间。
    let recent_token = recent_reset.and_then(|item| {
        item.observed_reset_at
            .or(item.user_confirmed_reset_at)
            .map(|at| format!("最近重置于 {}", format_clock(at)))
    });
    if let Some((badge,line)) = match status {
        "pending_analysis" => Some(("待分析","材料已同步，尚未得出结论")),
        "unchecked" => Some(("未检查","尚未检查重置来源")),
        "source_unavailable" => Some(("来源不可用","重置来源暂不可用")),
        _ => None,
    } { return (badge.into(),line.into(),line.into(),"查看详情了解进度".into()); }
    let banked = event_type == Some("banked_reset");
    // 摘要不携带 AI 运行细节（是否已分析/待分析等），AI 状态在详情页查看。
    if !ai_enabled && !matches!(status, "landed_observed" | "user_confirmed") {
        if matches!(
            status,
            "watching" | "upcoming" | "expected_time_passed" | "landed_claimed"
        ) {
            let primary = if status == "watching" {
                if banked {
                    "重置卡可能即将到账".into()
                } else {
                    "可能即将重置".into()
                }
            } else if source_has_direct_signal {
                "来源出现直接重置信号".into()
            } else {
                decision_time_text(
                    status,
                    event_type,
                    expected_at,
                    observed_at,
                    claimed_at,
                    user_confirmed_at,
                    None,
                )
            };
            let compact = expected_at
                .map(|at| format!("预计 {}", format_clock(at)))
                .unwrap_or_else(|| primary.clone());
            let secondary = expected_at.map_or_else(
                || "等待验证".into(),
                |at| format!("预计 {} · 等待验证", format_clock(at)),
            );
            let badge = if status == "watching" {
                "观察中"
            } else {
                "强信号"
            };
            return (badge.into(), primary, compact, secondary);
        }
        if source_has_direct_signal {
            return (
                "直接信号".into(),
                "来源出现直接重置信号".into(),
                "来源出现直接重置信号".into(),
                recent_token.unwrap_or_else(|| "本机未观察到新变化".into()),
            );
        }
        return (
            "暂无新信号".into(),
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
                None,
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
                None,
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
                "来源明确预告 · 等待验证".into(),
            )
        }
        "watching" => {
            let primary: String = if banked {
                "重置卡可能即将到账".into()
            } else {
                "可能即将重置".into()
            };
            (
                "观察中".into(),
                primary.clone(),
                primary,
                "时间尚未明确 · 等待验证".into(),
            )
        }
        _ => {
            // 无信号：不重复徽章状态，综合行只给“最近重置”或本机无变化。
            (
                "暂无新信号".into(),
                "暂无下一轮重置信号".into(),
                "暂无新信号".into(),
                recent_token.unwrap_or_else(|| "本机未观察到新变化".into()),
            )
        }
    }
}

pub async fn run_check(
    database: &Database,
    coordinator: &RefreshCoordinator,
    analyze: bool,
    source_id: Option<&str>,
    model: Option<&str>,
    user_prompt: Option<&str>,
    app: Option<&tauri::AppHandle>,
) -> Result<RadarSnapshot, String> {
    // 检查只消费已保存的设置，不在刷新时偷偷保存偏好；设置保存走 save_analysis_prefs。
    let started = epoch_ms();
    let id = format!("radar-{started}");
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("初始化雷达客户端失败：{error}"))?;
    let (sync_status, parse_status, post_count, error_message) = match fetch_feed(&client).await {
        Ok(outcome) => {
            let count = outcome.posts.len() as i64;
            // 已成功拉取的数据立即落库并更新分来源状态：后续分析被取消/失败时材料仍保留。
            database.replace_tibo_posts(&outcome.posts, started)?;
            save_notice(database, outcome.notice.as_ref())?;
            let _ = database.upsert_radar_source_status(
                RADAR_SOURCE_CODEXRADAR,
                outcome.codex_error.is_none(),
                outcome.codex_error.as_deref(),
                Some(outcome.codex_count as i64),
            );
            let _ = database.upsert_radar_source_status(
                RADAR_SOURCE_WILLCODEX,
                outcome.will_error.is_none(),
                outcome.will_error.as_deref(),
                Some(outcome.will_count as i64),
            );
            ("success".into(), "success".into(), count, None)
        }
        Err((codex_error, will_error)) => {
            let _ = database.upsert_radar_source_status(
                RADAR_SOURCE_CODEXRADAR,
                false,
                Some(&codex_error),
                None,
            );
            let _ = database.upsert_radar_source_status(
                RADAR_SOURCE_WILLCODEX,
                false,
                Some(&will_error),
                None,
            );
            (
                "failed".into(),
                "failed".into(),
                0,
                Some(format!(
                    "CodexRadar ({codex_error}) 与 WillCodex ({will_error}) 均同步失败"
                )),
            )
        }
    };

    if let Some(app) = app {
        let _ = app.emit("radar-data-changed", ());
    }
    // 提示词与缓存键组装前先把时间声明、额度观察和事件超时协调到最新状态。
    reconcile_event_state(database, epoch_ms())?;

    let mut analyze_status = if analyze {
        "skipped".to_string()
    } else {
        "off".into()
    };
    let mut analyze_error = None;
    if analyze {
        match run_analysis(database, coordinator, source_id, model, user_prompt).await {
            Ok(status) => analyze_status = status.into(),
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
        analyze_status: Some(analyze_status.clone()),
        error_message: error_message.or(analyze_error),
        post_count,
    })?;
    reconcile_event_state(database, epoch_ms())?;
    let result = snapshot(database)?;
    // The event and check result are already persisted before optional translations start.
    if let Some(app) = app {
        let app = app.clone();
        let source_id = source_id.map(str::to_owned);
        let model = model.map(str::to_owned);
        start_post_enrichment(&analyze_status, async move {
            let database = app.state::<Database>();
            let coordinator = app.state::<RefreshCoordinator>();
            // Translation is best effort, with a batch budget independent of the check lock.
            let _ = tokio::time::timeout(Duration::from_secs(30),
                enrich_untranslated_posts(&database, &coordinator, source_id.as_deref(), model.as_deref())
            ).await;
            let _ = app.emit("radar-data-changed", ());
        });
    }
    Ok(result)
}

static ENRICHMENT_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
struct EnrichmentGuard;
impl Drop for EnrichmentGuard {
    fn drop(&mut self) { ENRICHMENT_RUNNING.store(false, Ordering::Release); }
}

fn start_post_enrichment(
    analysis_status: &str,
    work: impl std::future::Future<Output = ()> + Send + 'static,
) -> Option<tauri::async_runtime::JoinHandle<()>> {
    // No fresh analysis means no automatic model calls for already processed material.
    if !matches!(analysis_status, "success" | "cached") || ENRICHMENT_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() { return None; }
    let guard = EnrichmentGuard;
    Some(tauri::async_runtime::spawn(async move {
        let _guard = guard;
        work.await;
    }))
}

const ENRICH_SYSTEM_PROMPT: &str = concat!(
    "You are an AI assistant analyzing tweets from OpenAI / Codex staff (Tibo) regarding ChatGPT/Codex quota resets and banked reset cards.\n",
    "Task: Translate the post into fluent Simplified Chinese, and classify its signal relevance for quota reset: \n",
    "- \"direct\": explicitly declares an imminent, currently active, or just completed reset for all/paid users, or banked reset card grant. \n",
    "- \"indirect\": mentions quota reset, past resets, schedule, rate limits, cooldown, hints, but is NOT an active global reset announcement. \n",
    "- \"none\": unrelated joke, question, coding chatter, or general noise. \n",
    "Return JSON ONLY in this format: {\"translated\": \"<中文翻译>\", \"category\": \"direct\"|\"indirect\"|\"none\"}.\n",
    "Glossary: banked reset / banked resets / reset card = 重置卡 (never 银行重置). quota reset = 额度重置."
);

#[derive(serde::Deserialize)]
struct PostEnrichOutput {
    translated: Option<String>,
    category: Option<String>,
}

fn parse_enrich_result(
    extracted: &str,
) -> (
    Option<String>,
    Option<&'static str>,
    Option<bool>,
    Option<&'static str>,
) {
    if let Some(json_str) = extract_json_object(extracted) {
        if let Ok(res) = serde_json::from_str::<PostEnrichOutput>(&json_str) {
            let tr = res.translated.as_deref().map(rewrite_banked_reset_zh);
            let (k, exp, lane) = match res.category.as_deref() {
                Some("direct") => (Some("reset"), Some(true), Some("可能重置")),
                Some("indirect") => (Some("indirect"), Some(false), Some("间接相关")),
                Some("none") => (Some("none"), Some(false), Some("无重置信号")),
                _ => (None, None, None),
            };
            return (tr, k, exp, lane);
        }
    }
    (Some(rewrite_banked_reset_zh(extracted)), None, None, None)
}

/// 单条 Tibo 动态的中文翻译与信号归类：调用已接入的对话模型，结果写回 SQLite 缓存。
pub async fn translate_post(
    database: &Database,
    coordinator: &RefreshCoordinator,
    post_id: &str,
    source_id: Option<&str>,
) -> Result<RadarSnapshot, String> {
    let post = database
        .tibo_posts_by_ids(&[post_id.to_string()])?
        .into_iter()
        .next()
        .ok_or_else(|| format!("雷达动态 {post_id} 不存在"))?;
    let target = resolve_chat_target(database, source_id, None)?;
    let body = json!({
        "model": target.model,
        "messages": [
            {"role": "system", "content": ENRICH_SYSTEM_PROMPT},
            {"role": "user", "content": post.text}
        ]
    });
    let text = send_chat(coordinator.client(), &target, &body).await?;
    let raw_extracted = extract_chat_text(&text)?;
    let (translated, kind, explicit_reset, tibo_lane) = parse_enrich_result(&raw_extracted);
    let translated = translated.unwrap_or_else(|| rewrite_banked_reset_zh(&raw_extracted));
    if translated.is_empty() {
        return Err("模型未返回可用的翻译".into());
    }
    let source_label = chat_target(&target.adapter_id)
        .map(|(display, _)| format!("{display} · {}", target.model))
        .unwrap_or_else(|| target.model.clone());
    database.update_tibo_enrichment(
        post_id,
        Some(&translated),
        epoch_ms(),
        &source_label,
        kind,
        explicit_reset,
        tibo_lane,
    )?;
    snapshot(database)
}

/// 后台异步丰富最新未翻译与未归类动态：对监控窗口内推文调用可用模型批量翻译与归类（最多 6 条，不阻塞主流程）
pub async fn enrich_untranslated_posts(
    database: &Database,
    coordinator: &RefreshCoordinator,
    source_id: Option<&str>,
    model: Option<&str>,
) -> Result<usize, String> {
    let target = match resolve_chat_target(database, source_id, model) {
        Ok(t) => t,
        Err(_) => return Ok(0),
    };
    let window_start = epoch_ms() - MONITOR_WINDOW_MS;
    let unenriched = match database.unenriched_tibo_posts_since(window_start, 6) {
        Ok(posts) => posts,
        Err(_) => return Ok(0),
    };
    if unenriched.is_empty() {
        return Ok(0);
    }
    let source_label = chat_target(&target.adapter_id)
        .map(|(display, _)| format!("{display} · {}", target.model))
        .unwrap_or_else(|| target.model.clone());

    let mut enriched_count = 0;
    for post in unenriched {
        if !load_analysis_prefs(database)?.analyze { break; }
        let body = json!({
            "model": target.model,
            "messages": [
                {"role": "system", "content": ENRICH_SYSTEM_PROMPT},
                {"role": "user", "content": post.text}
            ]
        });
        let Some((url, bearer)) = chat_endpoint_candidates(&target.adapter_id, target.api_base.as_deref()).into_iter().next() else { break; };
        let request = coordinator.client().post(url).timeout(Duration::from_secs(10)).json(&body);
        let request = if bearer { request.bearer_auth(target.secret.trim()) }
            else { request.header("Authorization", target.secret.trim()) };
        let response = match request.send().await {
            Ok(response) if response.status().is_success() => response,
            _ => break, // Stop this batch on failure; no retry loop or credential fallback.
        };
        if let Ok(text) = response.text().await {
            if let Ok(raw_extracted) = extract_chat_text(&text) {
                let (translated, kind, explicit_reset, tibo_lane) =
                    parse_enrich_result(&raw_extracted);
                let tr = translated.unwrap_or_else(|| rewrite_banked_reset_zh(&raw_extracted));
                if (!tr.is_empty() || kind.is_some())
                    && database
                        .update_tibo_enrichment(
                            &post.id,
                            if tr.is_empty() { None } else { Some(&tr) },
                            epoch_ms(),
                            &source_label,
                            kind,
                            explicit_reset,
                            tibo_lane,
                        )
                        .is_ok()
                {
                    enriched_count += 1;
                }
            }
        }
    }
    Ok(enriched_count)
}

/// 可用对话模型解析：优先用户指定来源，否则取第一个已就绪的 API Key 来源。
struct ChatTarget {
    source_id: String,
    adapter_id: String,
    model: String,
    secret: String,
    api_base: Option<String>,
}

const OPENAI_COMPATIBLE_ADAPTER: &str = "openai_compatible";
const ENDPOINT_SOURCE_PREFIX: &str = "radar-endpoint:";

fn endpoint_source_id(endpoint_id: &str) -> String {
    format!("{ENDPOINT_SOURCE_PREFIX}{endpoint_id}")
}

fn parse_endpoint_id(source_id: &str) -> Option<&str> {
    source_id.strip_prefix(ENDPOINT_SOURCE_PREFIX)
}

fn endpoint_secret_ref(endpoint_id: &str) -> String {
    vault::secret_ref("radar-endpoint", endpoint_id)
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
                    .find(|item| item.ready && !item.model.is_empty())
                    .map(|item| item.source_id)
            })
        })
        .ok_or_else(|| {
            "请先在平台中心接入对话 API Key，或添加其他对话接入".to_string()
        })?;
    if let Some(endpoint_id) = parse_endpoint_id(&source_id) {
        return resolve_endpoint_target(database, endpoint_id, model);
    }
    let source = database.source(&source_id)?;
    let secret = source
        .secret_ref
        .as_deref()
        .and_then(|reference| vault::get(reference).ok().flatten())
        .ok_or_else(|| "所选模型没有可用凭据".to_string())?;
    let model = model
        .filter(|value| !value.is_empty())
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

fn resolve_endpoint_target(
    database: &Database,
    endpoint_id: &str,
    model: Option<&str>,
) -> Result<ChatTarget, String> {
    let endpoint = database
        .radar_chat_endpoint(endpoint_id)?
        .ok_or_else(|| "找不到该对话接入".to_string())?;
    let secret = vault::get(&endpoint.secret_ref)?
        .ok_or_else(|| "所选模型没有可用凭据".to_string())?;
    let model = model
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            database
                .list_radar_chat_endpoint_models(endpoint_id)
                .ok()
                .and_then(|models| models.into_iter().next().map(|item| item.model))
        })
        .ok_or_else(|| "请指定模型名称".to_string())?;
    Ok(ChatTarget {
        source_id: endpoint_source_id(endpoint_id),
        adapter_id: OPENAI_COMPATIBLE_ADAPTER.into(),
        model,
        secret,
        api_base: Some(endpoint.api_base_url),
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

async fn fetch_codexradar_feed(
    client: &Client,
) -> Result<(Vec<TiboPostRecord>, Option<RadarNotice>), String> {
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
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("读取 CodexRadar 失败：{error}"))?;
    let body = String::from_utf8_lossy(&bytes);
    let synced_at = epoch_ms();
    codexradar::parse_page(&body, synced_at)
}

async fn fetch_willcodex_feed(client: &Client) -> Result<Vec<TiboPostRecord>, String> {
    let response = client
        .get(WILLCODEX_FEED_URL)
        .header("Accept", "application/json")
        .header(
            "User-Agent",
            "AIQuotaMonitor/0.1 (desktop; Tibo radar sync; +https://www.willcodexquotareset.com/)",
        )
        .send()
        .await
        .map_err(|error| format!("无法连接 WillCodex：{error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "WillCodex 返回 HTTP {}",
            response.status().as_u16()
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("读取 WillCodex 失败：{error}"))?;
    let body = String::from_utf8_lossy(&bytes);
    let synced_at = epoch_ms();
    willcodex::parse_posts(&body, synced_at)
}

/// 融合来自 CodexRadar 与 WillCodex 的推文，按推文全局唯一 ID 严格去重并合并属性。
/// CodexRadar 的精校翻译、语境解读与摘要优先保留；WillCodex 的实时新帖与上下文自然并入。
fn record_source_provenance(post: &mut TiboPostRecord, source: &str) {
    let mut extra = extra_object(&post.extra_json);
    if !extra.is_object() { extra = json!({}); }
    extra[format!("source_{source}")] = json!({"url":post.url,"text":post.text,"postedAt":(post.posted_at>0).then_some(post.posted_at),"kind":post.kind,"context":extra.get("context"),"updatedAt":post.synced_at});
    post.extra_json = extra.to_string();
}

fn merge_tibo_posts(
    codex_posts: Vec<TiboPostRecord>,
    will_posts: Vec<TiboPostRecord>,
) -> Vec<TiboPostRecord> {
    use std::collections::HashMap;

    let mut map: HashMap<String, TiboPostRecord> =
        HashMap::with_capacity(codex_posts.len() + will_posts.len());

    // 1. 先存入 WillCodex 抓取到的推文（时效性高，覆盖最新的推文流与回复上下文）
    for mut post in will_posts {
        record_source_provenance(&mut post, "willcodex");
        map.insert(post.id.clone(), post);
    }

    // 2. 用 CodexRadar 的推文进行合并融合（保留中文精校翻译、摘要与语境解读）
    for mut codex_post in codex_posts {
        record_source_provenance(&mut codex_post, "codexradar");
        if let Some(existing) = map.get_mut(&codex_post.id) {
            if codex_post.translated_text.is_some() {
                existing.translated_text = codex_post.translated_text;
                existing.translated_at = codex_post.translated_at;
                existing.translation_source = codex_post.translation_source;
            }
            if !matches!(codex_post.kind.as_str(), "none" | "unknown") {
                existing.kind = codex_post.kind;
            }
            if codex_post.tibo_lane.is_some() {
                existing.tibo_lane = codex_post.tibo_lane;
            }
            if codex_post.explicit_reset {
                existing.explicit_reset = true;
            }
            if existing.text.trim().is_empty() { existing.text = codex_post.text.clone(); }
            if existing.posted_at <= 0 { existing.posted_at = codex_post.posted_at; }
            // 融合 extra_json 属性
            let mut base_extra = serde_json::from_str::<Value>(&existing.extra_json)
                .ok()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            let codex_extra = serde_json::from_str::<Value>(&codex_post.extra_json)
                .ok()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            for (k, v) in codex_extra {
                if !v.is_null() && v.as_str() != Some("") { base_extra.insert(k, v); }
            }
            existing.extra_json = serde_json::Value::Object(base_extra).to_string();
        } else {
            map.insert(codex_post.id.clone(), codex_post);
        }
    }

    let mut result: Vec<TiboPostRecord> = map.into_values().collect();
    // 统一按发布时间倒序排列
    result.sort_by(|a, b| b.posted_at.cmp(&a.posted_at));
    result
}

/// 单次检查结果：合并后的帖子 + 各来源独立成败。单源失败保留另一来源数据。
struct FeedOutcome {
    posts: Vec<TiboPostRecord>,
    notice: Option<RadarNotice>,
    codex_count: usize,
    will_count: usize,
    codex_error: Option<String>,
    will_error: Option<String>,
}

async fn fetch_feed(client: &Client) -> Result<FeedOutcome, (String, String)> {
    let (codex_res, will_res) = tokio::join!(
        fetch_codexradar_feed(client),
        fetch_willcodex_feed(client)
    );

    match (codex_res, will_res) {
        (Ok((codex_posts, notice)), Ok(will_posts)) => {
            let (codex_count, will_count) = (codex_posts.len(), will_posts.len());
            let merged = merge_tibo_posts(codex_posts, will_posts);
            Ok(FeedOutcome {
                posts: merged,
                notice,
                codex_count,
                will_count,
                codex_error: None,
                will_error: None,
            })
        }
        (Ok((codex_posts, notice)), Err(err)) => {
            log::warn!("WillCodex 同步失败，降级仅使用 CodexRadar: {err}");
            let codex_count = codex_posts.len();
            Ok(FeedOutcome {
                posts: merge_tibo_posts(codex_posts, Vec::new()),
                notice,
                codex_count,
                will_count: 0,
                codex_error: None,
                will_error: Some(err),
            })
        }
        (Err(err), Ok(will_posts)) => {
            log::warn!("CodexRadar 同步失败，降级仅使用 WillCodex: {err}");
            let will_count = will_posts.len();
            Ok(FeedOutcome {
                posts: merge_tibo_posts(Vec::new(), will_posts),
                notice: None,
                codex_count: 0,
                will_count,
                will_error: None,
                codex_error: Some(err),
            })
        }
        (Err(err1), Err(err2)) => Err((err1, err2)),
    }
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
        translated_text: post.translated_text.as_deref().map(rewrite_banked_reset_zh),
        translated_at: post.translated_at,
        translation_source: post.translation_source,
        summary: extra_string(&extra, "summary").map(|text| rewrite_banked_reset_zh(&text)),
        analysis: extra_string(&extra, "analysis").map(|text| rewrite_banked_reset_zh(&text)),
        lifecycle_consumed_at: post.lifecycle_consumed_at,
        context: extra_string(&extra, "context"),
        source: extra_string(&extra, "source"),
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

/// 把 banked reset 的直译「银行重置」收成产品用语「重置卡」，不改分类、不编造数据。
fn rewrite_banked_reset_zh(text: &str) -> String {
    let mut out = text.to_string();
    const PAIRS: &[(&str, &str)] = &[
        ("银行重置卡", "重置卡"),
        ("银行的重置", "重置卡"),
        ("银行式重置", "重置卡"),
        ("银行重置", "重置卡"),
        ("a banked reset", "重置卡"),
        ("the banked reset", "重置卡"),
        ("A banked reset", "重置卡"),
        ("The banked reset", "重置卡"),
        ("Banked Resets", "重置卡"),
        ("banked resets", "重置卡"),
        ("Banked Reset", "重置卡"),
        ("banked reset", "重置卡"),
    ];
    for (from, to) in PAIRS {
        if out.contains(from) {
            out = out.replace(from, to);
        }
    }
    out
}

fn rewrite_zh_list(items: Vec<String>) -> Vec<String> {
    items
        .into_iter()
        .map(|item| rewrite_banked_reset_zh(&item))
        .collect()
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
        // 缺少或无法识别的标签 ≠ 无关：标为未分类，等待上游或 AI 判定。
        _ => "unknown".into(),
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
        conclusion: record.conclusion.as_deref().map(rewrite_banked_reset_zh),
        analysis_basis: record.analysis_basis.as_deref().map(rewrite_banked_reset_zh),
        confidence: record.confidence,
        citations: json_list(&record.citations_json),
        support: rewrite_zh_list(json_list(&record.support_json)),
        against: rewrite_zh_list(json_list(&record.against_json)),
        uncertainty: rewrite_zh_list(json_list(&record.uncertainty_json)),
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
            if source.source_type != "api_key" || !source_ready(&source) {
                continue;
            }
            let display = chat_target(&source.adapter_id).map(|(name, _)| name);
            let display_name = display_name_for(
                &source,
                display.unwrap_or(platform.display_name.as_str()),
            );
            match chat_target(&source.adapter_id) {
                Some((_, model)) => {
                    options.push(RadarModelOption {
                        source_id: source.id.clone(),
                        platform_id: platform.platform_id.clone(),
                        display_name: display_name.clone(),
                        model: model.into(),
                        ready: true,
                        custom: false,
                        kind: "platform".into(),
                    });
                }
                None => {
                    // 支持对话端点但无可靠默认模型（如 Kimi）：只有还没有自定义模型时
                    // 才放空占位，方便在下方添加；已有自定义模型则不再占一行。
                    if chat_endpoint_candidates(&source.adapter_id, None).is_empty() {
                        continue;
                    }
                    let has_custom = custom_models.iter().any(|item| item.source_id == source.id);
                    if !has_custom {
                        options.push(RadarModelOption {
                            source_id: source.id.clone(),
                            platform_id: platform.platform_id.clone(),
                            display_name: display_name.clone(),
                            model: String::new(),
                            ready: true,
                            custom: false,
                            kind: "platform".into(),
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
                    display_name: display_name.clone(),
                    model: custom.model.clone(),
                    ready: true,
                    custom: true,
                    kind: "platform".into(),
                });
            }
        }
    }
    append_endpoint_models(database, &mut options)?;
    Ok(options)
}

fn append_endpoint_models(
    database: &Database,
    options: &mut Vec<RadarModelOption>,
) -> Result<(), String> {
    for endpoint in database.list_radar_chat_endpoints()? {
        let ready = vault::get(&endpoint.secret_ref)
            .ok()
            .flatten()
            .is_some();
        if !ready {
            continue;
        }
        let models = database.list_radar_chat_endpoint_models(&endpoint.id)?;
        for (index, item) in models.into_iter().enumerate() {
            options.push(RadarModelOption {
                source_id: endpoint_source_id(&endpoint.id),
                platform_id: "radar-endpoint".into(),
                display_name: endpoint.display_name.clone(),
                model: item.model,
                ready: true,
                custom: index > 0,
                kind: "endpoint".into(),
            });
        }
    }
    Ok(())
}

fn chat_endpoint_views(database: &Database) -> Result<Vec<RadarChatEndpointView>, String> {
    let mut views = Vec::new();
    for endpoint in database.list_radar_chat_endpoints()? {
        let ready = vault::get(&endpoint.secret_ref)
            .ok()
            .flatten()
            .is_some();
        let models = database
            .list_radar_chat_endpoint_models(&endpoint.id)?
            .into_iter()
            .map(|item| item.model)
            .collect();
        views.push(RadarChatEndpointView {
            id: endpoint.id.clone(),
            source_id: endpoint_source_id(&endpoint.id),
            display_name: endpoint.display_name,
            api_base_url: endpoint.api_base_url,
            models,
            ready,
        });
    }
    Ok(views)
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
        OPENAI_COMPATIBLE_ADAPTER => {
            let Some(base) = api_base.map(str::trim).filter(|value| !value.is_empty()) else {
                return Vec::new();
            };
            vec![(join_chat_url(base, "/chat/completions"), true)]
        }
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
    /// 监控窗口内的帖子（用于「来源直接信号」判断）。
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

/// 三组划分使用固定监控窗口（最近 72 小时），与浏览筛选完全解耦：
/// 浏览时间范围只影响前端展示，不改变「是否新增」的判断。
fn classify_analysis_posts(
    database: &Database,
    posts: &[TiboPostView],
    active_events: &[RadarEventRecord],
) -> Result<ClassifiedPosts, String> {
    let window_start = epoch_ms() - MONITOR_WINDOW_MS;
    let range_posts: Vec<_> = posts
        .iter()
        .filter(|post| post.posted_at >= window_start)
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
        let in_window = post.posted_at >= window_start;
        if closed_ids.contains(&post.id) {
            if in_window {
                historical.push(post.clone());
                seen.insert(post.id.as_str());
            }
            continue;
        }
        if in_window && post.lifecycle_consumed_at.is_none() {
            new_posts.push(post.clone());
            seen.insert(post.id.as_str());
            continue;
        }
        if event_ids.contains(&post.id) {
            event_context.push(post.clone());
            seen.insert(post.id.as_str());
            continue;
        }
        if in_window && !seen.contains(post.id.as_str()) {
            historical.push(post.clone());
        }
    }
    event_context.retain(|post| !new_posts.iter().any(|item| item.id == post.id));
    Ok(ClassifiedPosts {
        mode: "live_delta",
        range_posts,
        new_posts,
        event_context,
        historical,
    })
}

/// 本轮分析的新增/上下文分组：NEW POSTS 才能创建或推进事件。
#[derive(Clone)]
pub(crate) struct DeltaInputs {
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
                // 时间语义：grant=预告/发放，deadline=截止，historical=历史；由代码标注。
                "claim_kind": claim.claim_kind,
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

/// 帖子的自然语言友好标签（北京时间），用于正文可读化。
fn radar_post_label(post: &TiboPostView, _latest: bool) -> String {
    let Some(time) = chrono::DateTime::from_timestamp_millis(post.posted_at) else {
        return "动态".to_string();
    };
    let beijing = chrono::FixedOffset::east_opt(8 * 3600).expect("UTC+8 is a valid offset");
    let time = time.with_timezone(&beijing);
    format!("{:02}:{:02} 动态", time.format("%H"), time.format("%M"))
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

fn collect_delta_inputs(database: &Database) -> Result<DeltaInputs, String> {
    // 分析输入与浏览分页完全独立：NEW 候选直接查监控窗口内未消费帖（不共用 list_tibo_posts），
    // 上下文按事件证据帖取，历史上下文取最近两个已关闭事件的证据帖。
    let now = epoch_ms();
    let window_start = now - MONITOR_WINDOW_MS;
    let closed_ids = database.closed_radar_event_post_ids()?;
    let delta = database
        .unconsumed_tibo_posts_since(window_start, 48)?
        .into_iter()
        .filter(|post| !closed_ids.contains(&post.id))
        .map(to_view)
        .collect::<Vec<_>>();
    let events = database.active_radar_events()?;
    let active: Vec<_> = events
        .iter()
        .filter(|item| item.expires_at.is_none_or(|at| at > now))
        .cloned()
        .collect();
    let mut context_ids: Vec<String> = Vec::new();
    for event in &active {
        for post_id in database.radar_event_post_ids(&event.id).unwrap_or_default() {
            if !context_ids.contains(&post_id) {
                context_ids.push(post_id);
            }
        }
    }
    let mut historical_ids: Vec<String> = Vec::new();
    let closed_events = database
        .radar_events_recent(8)?
        .into_iter()
        .filter(|event| event.closed_at.is_some())
        .take(2);
    for event in closed_events {
        for post_id in database.radar_event_post_ids(&event.id).unwrap_or_default() {
            if !historical_ids.contains(&post_id) && !context_ids.contains(&post_id) {
                historical_ids.push(post_id);
            }
        }
    }
    let context = database
        .tibo_posts_by_ids(&context_ids)?
        .into_iter()
        .map(to_view)
        .collect::<Vec<_>>();
    let historical = database
        .tibo_posts_by_ids(&historical_ids)?
        .into_iter()
        .map(to_view)
        .collect::<Vec<_>>();
    let mode = "live_delta";
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
    source_id: Option<&str>,
    model: Option<&str>,
    user_prompt_override: Option<&str>,
) -> Result<&'static str, String> {
    let mut status = "skipped";
    for _ in 0..8 {
        let inputs = collect_delta_inputs(database)?;
        if inputs.delta.is_empty() {
            return Ok(status);
        }
        let before: std::collections::HashSet<_> =
            inputs.delta.iter().map(|post| post.id.clone()).collect();
        status = run_analysis_batch(
            database,
            coordinator,
            source_id,
            model,
            user_prompt_override,
            &inputs,
        )
        .await?;
        let window_start = epoch_ms() - MONITOR_WINDOW_MS;
        let after: std::collections::HashSet<_> = database
            .unconsumed_tibo_posts_since(window_start, 48)?
            .into_iter()
            .map(|post| post.id)
            .collect();
        if before.iter().all(|id| after.contains(id)) {
            break;
        }
    }
    Ok(status)
}

async fn run_analysis_batch(
    database: &Database,
    coordinator: &RefreshCoordinator,
    source_id: Option<&str>,
    model: Option<&str>,
    user_prompt_override: Option<&str>,
    inputs: &DeltaInputs,
) -> Result<&'static str, String> {
    let user_prompt = match user_prompt_override {
        Some(prompt) => sanitize_user_prompt(prompt),
        None => load_analysis_prefs(database)?.user_prompt,
    };
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
    let cache_key = application::cache_key(&input_hash, &context_hash, &prompt_hash, &target);
    if application::replay_prepared(database, &inputs, &cache_key)? { return Ok("cached"); }
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
        Ok(()) => Ok("success"),
        Err(error) => {
            let _ = persist_failed_analysis(database, source_id, model, &error);
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
    messages.insert(1, json!({"role":"system","content":application_prompt()}));
    let body = json!({"model":target.model,"stream":false,"messages":messages});
    let text = send_chat(client, target, &body).await?;
    let mut parsed = parse_model_json(&text)?;
    application::validate(&mut parsed, inputs)?;
    normalize_model_json(&mut parsed, inputs);
    validate_parsed_model_json(&parsed)?;
    let record = RadarAnalysisRecord {
        id: format!("analysis-{}", epoch_ms()),
        created_at: epoch_ms(),
        range_key: MONITOR_RANGE_KEY.into(),
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
    application::save_prepared(database, &application::cache_key(input_hash, context_hash, prompt_hash, target), &record, &parsed, &input)?;
    application::commit(database, inputs, &parsed, &record, &input)?;
    Ok(())
}

/// 结构级校验：空对象、无结论无枚举的输出、声明建/推进事件却没有任何有效引用的输出
/// 一律视为失败，不落库、不消耗整批材料（材料保持待处理，下次检查重试）。
fn validate_parsed_model_json(parsed: &ModelJson) -> Result<(), String> {
    let conclusion_blank = parsed
        .conclusion
        .as_deref()
        .map(str::trim)
        .is_none_or(str::is_empty);
    if conclusion_blank
        && parsed.signal_level.is_none()
        && parsed.event_relation.is_none()
        && parsed.citations.is_empty()
    {
        return Err("模型未返回有效分析结论".into());
    }
    if matches!(
        parsed.event_relation.as_deref(),
        Some("new_event" | "same_event")
    ) && parsed.citations.is_empty()
    {
        return Err("分析声明新事件但未引用任何帖子".into());
    }
    Ok(())
}

/// 把 AI 的结构化增量输出套到当前事件上。
/// 返回 (event_id, historical_replay)。没有引用真正 NEW POSTS 时不改事件。
/// 同批材料同时涉及额度重置与重置卡时，主信号之外可携带 secondary signal
/// 分别建/推进对应类型事件；两类事件互不关闭。
fn apply_analysis_to_event(
    database: &Database,
    inputs: &DeltaInputs,
    parsed: &ModelJson,
    analysis_id: &str,
) -> Result<(Option<String>, bool), String> {
    if inputs.mode == "historical_replay" {
        return Ok((None, true));
    }
    let cited_new: Vec<&TiboPostView> = cited_new_posts(inputs, parsed)
        .into_iter()
        .filter(|post| post.posted_at > 0)
        .collect();
    let cited_context = cited_context_posts(inputs, parsed);
    if cited_new.is_empty() {
        return Ok((None, false));
    }
    let weakening = matches!(parsed.delta_effect.as_deref(), Some("cancel" | "weaken"));
    if (parsed.signal_level.as_deref() == Some("none") && !weakening)
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
    let expected_at = expected_from_ai_post(inputs, parsed.expected_time_post.as_deref(), &cited_new)
        .or_else(|| expected_from_posts(inputs, &cited_new));
    let phase = ai_settable_phase(parsed.event_phase.as_deref()).unwrap_or("watching");
    let outcome = advance_or_create_event(
        database,
        event_type,
        parsed.event_relation.as_deref().unwrap_or("none"),
        phase,
        parsed.delta_effect.as_deref(),
        parsed.conclusion.as_deref(),
        parsed.analysis_basis.as_deref(),
        &cited_new,
        &cited_context,
        expected_at,
        analysis_id,
    )?;
    // v21 eventUpdates already apply each type independently; keep secondary_* only for legacy outputs.
    if parsed.event_updates.is_empty() {
        let _ = apply_secondary_signal(database, inputs, parsed, event_type, analysis_id)?;
    }
    Ok(outcome)
}

/// 单个信号类型的事件推进/创建；replay 表示「按推导已过期，只做历史解释」。
#[allow(clippy::too_many_arguments)]
fn advance_or_create_event(
    database: &Database,
    event_type: &str,
    relation: &str,
    phase: &str,
    delta_effect: Option<&str>,
    conclusion: Option<&str>,
    analysis_basis: Option<&str>,
    cited_new: &[&TiboPostView],
    cited_context: &[&TiboPostView],
    expected_at: Option<i64>,
    analysis_id: &str,
) -> Result<(Option<String>, bool), String> {
    let newest_posted_at = cited_new.iter().map(|post| post.posted_at).max().unwrap_or(0);
    let claimed_at = (phase == "landed_claimed").then_some(newest_posted_at);
    let now = epoch_ms();
    if derived_event_already_expired(phase, newest_posted_at, expected_at, claimed_at, now) {
        return Ok((None, true));
    }
    match relation {
        "new_event" => {
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
                event_type,
                analysis_basis,
                phase,
                cited_new,
                cited_context,
                expected_at,
                analysis_id,
            )?;
            Ok((Some(event_id), false))
        }
        "same_event" => {
            let Some(active) = database.active_radar_event_of_type(event_type)? else {
                if matches!(delta_effect, Some("cancel" | "weaken")) {
                    return Ok((None, false));
                }
                let event_id = create_radar_event(
                    database,
                    event_type,
                    analysis_basis,
                    phase,
                    cited_new,
                    cited_context,
                    expected_at,
                    analysis_id,
                )?;
                return Ok((Some(event_id), false));
            };
            if active.expires_at.is_some_and(|at| now >= at) {
                return Ok((None, true));
            }
            let mut updated = active;
            let has_new_evidence = newest_posted_at >= updated.latest_evidence_at;
            let old_phase = updated.phase.clone();
            match delta_effect {
                Some("cancel") if has_new_evidence && updated.observed_reset_at.is_none() && updated.user_confirmed_reset_at.is_none() => {
                    updated.phase = "closed".into();
                    updated.closed_at = Some(now);
                    updated.close_reason = Some("source_withdrawn".into());
                }
                Some("cancel") if has_new_evidence => {
                    // Landed local facts are not erased by a later retract; keep them and record the conflict.
                    let note = conclusion.unwrap_or("后续信息与已落地事实冲突");
                    updated.summary = Some(match &updated.summary {
                        Some(existing) => format!("{existing}；后续撤回：{note}"),
                        None => format!("后续撤回（已落地事实保留）：{note}"),
                    });
                }
                Some("weaken") if has_new_evidence && updated.observed_reset_at.is_none() && updated.user_confirmed_reset_at.is_none() => {
                    updated.phase = "watching".into();
                }
                Some("advance_phase") if has_new_evidence => {
                    if let Some(next) = ai_settable_phase(Some(phase)) {
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
                if let Some(basis) = analysis_basis {
                    updated.summary = Some(basis.to_string());
                }
            }
            if updated.phase != "closed" && updated.observed_reset_at.is_none() {
                updated.title = event_title_for_phase(&updated.phase, &updated.event_type);
            }
            if has_new_evidence && updated.observed_reset_at.is_none() && matches!(delta_effect, Some("update_time" | "advance_phase")) {
                updated.expected_at = expected_at.or(updated.expected_at);
            }
            updated.expires_at = event_expiry(&updated);
            if updated.phase != old_phase || has_new_evidence {
                updated.state_revision += 1;
            }
            database.update_radar_event(&updated)?;
            add_cited_evidence(database, &updated.id, analysis_id, cited_new, cited_context)?;
            Ok((Some(updated.id), false))
        }
        _ => Ok((None, false)),
    }
}

/// 同批材料中的次要信号（另一信号类型）：校验引用与来源信号后独立建/推进事件。
fn apply_secondary_signal(
    database: &Database,
    inputs: &DeltaInputs,
    parsed: &ModelJson,
    primary_event_type: &str,
    analysis_id: &str,
) -> Result<Option<String>, String> {
    let Some(event_type) = event_type_from_signal(parsed.secondary_signal_type.as_deref()) else {
        return Ok(None);
    };
    if event_type == primary_event_type {
        return Ok(None);
    }
    if !matches!(
        parsed.secondary_event_relation.as_deref(),
        Some("new_event" | "same_event")
    ) {
        return Ok(None);
    }
    let cited_new: Vec<&TiboPostView> = inputs
        .delta
        .iter()
        .filter(|post| parsed.secondary_citations.contains(&post.id))
        .collect();
    if cited_new.is_empty() {
        return Ok(None);
    }
    let cited_context: Vec<&TiboPostView> = inputs
        .context
        .iter()
        .filter(|post| parsed.secondary_citations.contains(&post.id))
        .collect();
    let expected_at = expected_from_ai_post(
        inputs,
        parsed.expected_time_post.as_deref(),
        &cited_new,
    )
    .or_else(|| expected_from_posts(inputs, &cited_new));
    let phase =
        ai_settable_phase(parsed.secondary_event_phase.as_deref()).unwrap_or("watching");
    let outcome = advance_or_create_event(
        database,
        event_type,
        parsed
            .secondary_event_relation
            .as_deref()
            .unwrap_or("none"),
        phase,
        parsed.secondary_event_phase.as_deref().map(|_| "advance_phase"),
        parsed.conclusion.as_deref(),
        parsed.analysis_basis.as_deref(),
        &cited_new,
        &cited_context,
        expected_at,
        analysis_id,
    )?;
    Ok(outcome.0)
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

#[allow(clippy::too_many_arguments)]
fn create_radar_event(
    database: &Database,
    event_type: &str,
    analysis_basis: Option<&str>,
    phase: &str,
    cited_new: &[&TiboPostView],
    cited_context: &[&TiboPostView],
    expected_at: Option<i64>,
    analysis_id: &str,
) -> Result<String, String> {
    let now = epoch_ms();
    let newest = cited_new.iter().map(|post| post.posted_at).max().unwrap_or(now);
    let oldest = cited_new.iter().map(|post| post.posted_at).min().unwrap_or(now);
    let mut event = RadarEventRecord {
        id: format!("event-{:x}", simple_hash(&format!("{}:{}", event_type, { let mut ids = cited_new.iter().map(|p|p.id.as_str()).collect::<Vec<_>>(); ids.sort_unstable(); ids.join(",") }))),
        phase: phase.into(),
        title: event_title_for_phase(phase, event_type),
        summary: analysis_basis.map(str::to_string),
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
    // 事件创建后，把此前已独立保存的本机观察补挂到该事件（先观察后识别场景）。
    let events = database.active_radar_events()?;
    if let Some(record) = events.iter().find(|item| item.id == event.id) {
        let cloned = record.clone();
        link_pending_observations(
            database,
            (event_type != "banked_reset").then_some(&cloned),
            (event_type == "banked_reset").then_some(&cloned),
        )?;
    }
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
        // 截止与历史语义的时间不参与预计重置时间。
        .filter(|claim| claim.claim_kind == "grant")
        .filter_map(|claim| claim.resolved_at)
        .collect::<std::collections::HashSet<_>>()
        .into_iter().fold(None, |acc: Option<(i64,bool)>, at| Some((at, acc.is_none())))
        .filter(|(_,unique)| *unique).map(|(at,_)|at)
}

/// AI 指定「预计重置时间」所在帖子（expected_time_post）：AI 只选帖，
/// 时间仍取该帖代码解析的时间声明（grant/unknown 语义，北京时间换算由后端完成）；
/// 指向的帖子不在被引用的新帖中或没有可解析声明时忽略，回退全组声明。
fn expected_from_ai_post(
    inputs: &DeltaInputs,
    ai_post_id: Option<&str>,
    cited_new: &[&TiboPostView],
) -> Option<i64> {
    let target = ai_post_id?;
    if !cited_new.iter().any(|post| post.id == target) {
        return None;
    }
    inputs
        .time_claims
        .iter()
        .filter(|claim| claim.post_id == target)
        .filter(|claim| claim.claim_kind == "grant")
        .filter_map(|claim| claim.resolved_at)
        .collect::<std::collections::HashSet<_>>()
        .into_iter().fold(None, |acc: Option<(i64,bool)>, at| Some((at, acc.is_none())))
        .filter(|(_,unique)| *unique).map(|(at,_)|at)
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

fn application_prompt() -> &'static str {
    r#"Required v21 output: keep the Chinese conclusion and analysis_basis. conclusion must be 15-30 chars without post refs or timestamps. analysis_basis must be 30-60 chars summarizing the core factual reason; do NOT write a mechanical post-by-post list (流水账), do NOT repeat dates/timestamps. Also return postOutcomes:[{"postId":"N1","outcome":"signal|no_signal|retry"}] for each NEW post and eventUpdates:[{"eventType":"quota_reset|banked_reset","eventId":null,"operation":"create|reinforce|weaken|advance_phase|cancel|update_time","phase":"watching|upcoming|landed_claimed","signalLevel":"none|weak|strong","citations":["N1"],"timePostId":null,"timeRaw":null,"clearTime":false,"conclusion":"简短中文业务结论（严禁包含帖子代号或时间）"}]. These arrays replace secondary_* fields. At most one update per event type. Every signal outcome must be cited by an update. no_signal means the post is unrelated; retry means insufficient information. Missing post outcomes stay pending. Cancellation and weakening refer to the existing event type even when the post is negative. Select a specific timeRaw from the provided claims only for an announced delivery time. Use clearTime only when a newer statement explicitly withdraws timing. Never create an event from a closed-event context alone."#
}

const ANALYSIS_SYSTEM_PROMPT: &str = concat!(
    "You analyze public Tibo/Codex reset-related posts. The product recognizes two distinct reset signal types. ",
    "A banked reset is a saved, one-time reset delivered to an account for later manual use. Wording such as banked reset, reset available, reset card, one reset per day, compensatory reset, or first one will land may announce its grant, delivery, or availability. ",
    "A banked reset is a valid new reset signal even though it does not immediately refresh usage windows. Never classify it as unrelated merely because it is not a global or automatic quota reset. ",
    "A quota reset is an actual refresh or restoration of Codex, ChatGPT Work, or related usage-limit windows, including resetting paid-user usage, restoring rate limits, a full or global reset, or a statement that such a reset was executed. ",
    "Banked-reset delivery is not proof that quota windows have already reset, and an observed quota refresh is not proof that a banked reset was delivered. ",
    "signal_type must be banked_reset, quota_reset, or none. banked reset, one reset per day, first one will land, and reset available are banked_reset. Never output none merely because a banked reset is not a global automatic quota refresh. quota_reset means usage windows actually refresh or restore. ",
    "When the same batch contains evidence for BOTH signal types, put the stronger one in signal_type and describe the other in the optional secondary fields: secondary_signal_type (banked_reset or quota_reset), secondary_citations (aliases of NEW POSTS supporting that second signal), secondary_event_relation (new_event or same_event), secondary_event_phase. Omit or leave empty when there is no second signal. ",
    "Time semantics matter: a grant or delivery time is when resets land; a deadline is the last moment to qualify or act; a historical time refers to a past reset. Never treat a deadline or a historical time as the expected reset time. When NEW POSTS announce a concrete expected reset time, fill expected_time_post with the alias of the post that carries the authoritative announcement time; do not compute or convert any time yourself. ",
    "When a new signal exists, conclusion and analysis_basis must explicitly call it 重置卡 or 额度重置. In Simplified Chinese user-facing fields, a banked reset is always 重置卡 and a quota reset is always 额度重置. Never write 银行重置, 银行重置卡, or treat bank as a financial institution; banked means stored for later manual use. new_event or same_event requires signal_type banked_reset or quota_reset, at least one NEW POST citation, and must not reuse a closed event. Use upcoming for promised delivery, landed_claimed for claimed delivery, and watching when timing is unclear. ",
    "Input has three groups: ",
    "NEW POSTS are genuinely unconsumed posts and the only posts that may create or advance an event; ",
    "EVENT CONTEXT POSTS are already linked to the current event and must not become new evidence just because they reappear; ",
    "HISTORICAL CONTEXT POSTS are already analyzed, closed-event, or old posts brought in by a wider range and may only produce historical explanation. ",
    "EVENT CONTEXT and HISTORICAL CONTEXT must never be the sole basis of a new event. ",
    "new_event or same_event must cite at least one NEW POST. Closed-event posts must not reactivate an event. ",
    "Do not splice old signal semantics with the timestamp of a new unrelated post to invent a new event. ",
    "Reply with JSON only: {\"conclusion\":\"\",\"analysis_basis\":\"\",\"confidence\":\"low|medium|high\",\"event_relation\":\"new_event|same_event|none\",\"event_phase\":\"watching|upcoming|landed_claimed\",\"delta_effect\":\"reinforce|no_change|weaken|advance_phase|cancel|new_event\",\"signal_level\":\"none|weak|strong\",\"signal_type\":\"banked_reset|quota_reset|none\",\"secondary_signal_type\":\"banked_reset|quota_reset|none\",\"secondary_citations\":[\"post_alias\"],\"secondary_event_relation\":\"new_event|same_event|none\",\"secondary_event_phase\":\"watching|upcoming|landed_claimed\",\"expected_time_post\":\"post_alias\",\"context_status\":\"complete|context_missing|conflicting\",\"citations\":[\"post_alias\"],\"support\":[\"\"],\"against\":[\"\"],\"uncertainty\":[\"\"]}. ",
    "Write conclusion and analysis_basis in Simplified Chinese. support, against, and uncertainty are expandable details, also in Simplified Chinese. ",
    "Each post is JSON and carries a short alias: N1, N2 … for NEW POSTS, C1, C2 … for EVENT CONTEXT, H1, H2 … for HISTORICAL CONTEXT. ",
    "In citations return exactly these aliases, one per cited post; never return raw numeric post ids or URLs. ",
    "Never write a raw numeric post id in conclusion, analysis_basis, support, against, or uncertainty. ",
    "analysis_basis must be 1 to 2 concise, fluent sentences in Simplified Chinese synthesizing the factual rationale (strictly 30 to 60 characters), without repeating conclusion verbatim. ",
    "analysis_basis must NEVER be a running account (流水账) that mechanically recites each post, alias, date, or timestamp one by one. Synthesize multiple posts into a unified factual rationale (e.g. \u{201c}官方宣布全员额度重置，后续补充提及中途刷新两次；确认窗口已实际刷新，非未来重置卡。\u{201d}). ",
    "In analysis_basis, NEVER write \u{201c}的最新帖子\u{201d} or \u{201c}最新帖\u{201d}. Refer to them naturally as \u{201c}官方动态\u{201d} or \u{201c}后续补充\u{201d} if necessary, or omit post references entirely. ",
    "conclusion must be a direct, definitive business status decision in 15 to 30 Chinese characters (e.g., \u{201c}官方已宣布额度全面重置，窗口已实际刷新\u{201d} or \u{201c}暂未发现额度重置信号\u{201d}). ",
    "conclusion must NEVER mention any post aliases (such as N1, N2, C1), post dates, timestamps, post references, or formulaic prefixes (such as \u{201c}本次新增帖子中\u{201d}, \u{201c}根据新增帖子\u{201d}). Evidence belongs strictly in citations and analysis_basis. ",
    "When older posts describe a completed reset, call them 上一轮历史背景 and never present them as confirmation of the current batch. ",
    "support may only contain claims the cited posts directly support; anything merely speculative belongs in uncertainty. ",
    "Every conclusion must be backed by citations referring to real input posts; never cite a post that was not provided. ",
    "Each post's time_claims are code-authoritative facts: resolved_beijing_at may be repeated verbatim; ambiguous claims must remain ambiguous. Never calculate, convert, or invent a time. If an unresolved post only says in about N hours, preserve that relative wording instead of inventing an absolute time. ",
    "CODE-AUTHORITATIVE EVENT STATE and the injected analysis time are facts and cannot be changed by your output. ",
    "Do not repeat internal prompt labels, enums, JSON keys, or NOW such as NEW POSTS, EVENT CONTEXT, HISTORICAL CONTEXT, CODE-AUTHORITATIVE EVENT STATE, event_relation, delta_effect, 分析时刻, 本次新增帖子, 事件上下文帖子, or 历史上下文帖子 in user-facing fields. ",
    "expected_time_passed means the announced time has passed but landing is still unverified; it is not landed. ",
    "If state says observed_landed, describe the posts as historical confirmation and use past tense. ",
    "conclusion must be a direct decision without markdown, evidence citations, or repeated reasoning. ",
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
    source_id: Option<&str>,
    model: Option<&str>,
    error: &str,
) -> Result<(), String> {
    database.insert_radar_analysis(&RadarAnalysisRecord {
        id: format!("analysis-{}", epoch_ms()),
        created_at: epoch_ms(),
        range_key: MONITOR_RANGE_KEY.into(),
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

/// 用户确认额度已重置：写入指定事件（缺省取当前活动事件）并进入 24 小时观察期，
/// 不冒充官方或重置卡归因；事件已观察/已关闭时拒绝，避免误操作到另一个事件。
pub fn confirm_user_reset(
    database: &Database,
    event_id: Option<&str>,
    expected_revision: Option<i64>,
) -> Result<RadarSnapshot, String> {
    let now = epoch_ms();
    reconcile_event_state(database, now)?;
    let mut event = match event_id {
        Some(id) => database
            .radar_event(id)?
            .ok_or_else(|| "指定事件不存在，可能已被清理".to_string())?,
        None => database
            .active_radar_event()?
            .ok_or_else(|| "当前没有可确认的重置事件".to_string())?,
    };
    if let Some(expected) = expected_revision {
        if event.state_revision != expected {
            return Err("事件已变化，请刷新后重试".into());
        }
    }
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

/// 撤销人工确认：清空确认时间并按 claimed/expected 恢复阶段；只作用于指定事件。
pub fn undo_user_reset(
    database: &Database,
    event_id: Option<&str>,
    expected_revision: Option<i64>,
) -> Result<RadarSnapshot, String> {
    let now = epoch_ms();
    reconcile_event_state(database, now)?;
    let mut event = match event_id {
        Some(id) => database
            .radar_event(id)?
            .ok_or_else(|| "指定事件不存在，可能已被清理".to_string())?,
        None => database
            .active_radar_event()?
            .ok_or_else(|| "当前没有可撤销的人工确认".to_string())?,
    };
    if let Some(expected) = expected_revision {
        if event.state_revision != expected {
            return Err("事件已变化，请刷新后重试".into());
        }
    }
    if event.closed_at.is_some() {
        return Err("事件已关闭，不能撤销人工确认".into());
    }
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

/// 隐藏或显示 CodexRadar 公告细条：只改展示偏好，不停同步、不清空公告缓存。
pub fn set_notice_hidden(database: &Database, hidden: bool) -> Result<RadarSnapshot, String> {
    database.set_setting_bool("radar_notice_hidden", hidden)?;
    snapshot(database)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all="camelCase")]
pub struct RadarAnalysisPage { pub items: Vec<RadarAnalysisView>, pub next_cursor: Option<String> }

pub fn list_event_analyses(database: &Database,event_id: &str,cursor: Option<&str>,limit: usize) -> Result<RadarAnalysisPage,String> {
    let (at,id)=parse_cursor(cursor);
    let limit=limit.clamp(1,100);
    let mut records=database.event_radar_analyses_page(event_id,at,id,limit+1)?;
    let has_more=records.len()>limit;
    records.truncate(limit);
    let next_cursor=if has_more {records.last().map(|r|format!("{}:{}",r.created_at,r.id))} else {None};
    Ok(RadarAnalysisPage {items:records.into_iter().map(|r|analysis_view(database,r,true)).collect(),next_cursor})
}

pub fn list_history(
    database: &Database,
    cursor: Option<&str>,
    limit: usize,
) -> Result<RadarHistoryView, String> {
    let (after_sort_at, after_id) = parse_cursor(cursor);
    history_view_page(database, after_sort_at, after_id, limit.max(1).min(100))
}

pub fn list_posts(
    database: &Database,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    cursor: Option<&str>,
    limit: usize,
) -> Result<Vec<TiboPostView>, String> {
    let (after_at, after_id) = parse_cursor(cursor);
    Ok(database
        .list_tibo_posts_page(from_ms, to_ms, after_at, after_id, limit.max(1).min(200))?
        .into_iter()
        .map(to_view)
        .collect())
}

pub fn get_post(database: &Database, post_id: &str) -> Result<TiboPostView, String> {
    database
        .tibo_posts_by_ids(&[post_id.to_string()])?
        .into_iter()
        .next()
        .map(to_view)
        .ok_or_else(|| format!("雷达动态 {post_id} 不存在"))
}

fn parse_cursor(cursor: Option<&str>) -> (Option<i64>, Option<&str>) {
    let Some(cursor) = cursor.filter(|value| !value.is_empty()) else {
        return (None, None);
    };
    match cursor.split_once(':') {
        Some((at, id)) => (at.parse().ok(), Some(id)),
        None => (None, Some(cursor)),
    }
}

pub fn save_analysis_prefs(
    database: &Database,
    analyze: bool,
    source_id: Option<&str>,
    model: Option<&str>,
    user_prompt: Option<&str>,
    background_check: Option<bool>,
) -> Result<(), String> {
    database.set_setting_bool("radar_analyze", analyze)?;
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
    if let Some(enabled) = background_check {
        database.set_setting_bool("radar_background_check", enabled)?;
    }
    Ok(())
}

/// 后台自动检查默认开启；仅当用户显式保存过 false 才关闭。
pub fn background_check_enabled(database: &Database) -> Result<bool, String> {
    Ok(database
        .setting_string("radar_background_check")?
        .map(|value| value == "true")
        .unwrap_or(true))
}

/// 下次后台检查到期时间（epoch 毫秒）；缺失视为已到期（启动后尽快补一次）。
pub fn background_next_check_at(database: &Database) -> Result<i64, String> {
    Ok(database
        .setting_string("radar_next_background_check_at")?
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0))
}

pub fn set_background_next_check_at(database: &Database, at: i64) -> Result<(), String> {
    database.set_setting_string("radar_next_background_check_at", &at.to_string())
}

/// 后台调度使用的偏好读取：失败时回退默认值（AI 关闭、后台检查开启），不阻塞调度。
pub fn load_analysis_prefs_for_runtime(database: &Database) -> RadarAnalysisPrefs {
    load_analysis_prefs(database).unwrap_or(RadarAnalysisPrefs {
        analyze: false,
        source_id: None,
        model: None,
        user_prompt: String::new(),
        default_user_prompt: DEFAULT_USER_PROMPT.into(),
        background_check: true,
        background_next_at: None,
    })
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
                || value == LEGACY_DEFAULT_USER_PROMPT_V18
                || value == LEGACY_DEFAULT_USER_PROMPT_V18_NO_INTERNAL_FIELDS
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
        source_id: database
            .setting_string("radar_source_id")?
            .filter(|value| !value.is_empty()),
        model: database
            .setting_string("radar_model")?
            .filter(|value| !value.is_empty()),
        user_prompt,
        default_user_prompt: DEFAULT_USER_PROMPT.into(),
        background_check: background_check_enabled(database)?,
        background_next_at: Some(background_next_check_at(database)?)
            .filter(|at| *at > 0),
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
    if parse_endpoint_id(source_id).is_some() {
        return add_endpoint_model(database, source_id, &model);
    }
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
    if parse_endpoint_id(source_id).is_some() {
        return delete_endpoint_model(database, source_id, &model);
    }
    database.delete_radar_custom_model(source_id, &model)?;
    snapshot(database)
}

async fn ping_chat_target(
    coordinator: &RefreshCoordinator,
    target: &ChatTarget,
) -> Result<(), String> {
    let body = json!({
        "model": target.model,
        "messages": [{"role": "user", "content": "ping"}]
    });
    send_chat(coordinator.client(), target, &body).await?;
    Ok(())
}

/// 未保存接入也可测：用用户填写的 https 地址和 API Key 发一次极小请求。
pub async fn test_chat_endpoint(
    coordinator: &RefreshCoordinator,
    api_base_url: &str,
    secret: &str,
    model: &str,
) -> Result<(), String> {
    let model = sanitize_custom_model(model)?;
    let api_base = sanitize_chat_base_url(api_base_url)?;
    let secret = secret.trim();
    if secret.is_empty() {
        return Err("API Key 不能为空".into());
    }
    let target = ChatTarget {
        source_id: "radar-endpoint:draft".into(),
        adapter_id: OPENAI_COMPATIBLE_ADAPTER.into(),
        model,
        secret: secret.into(),
        api_base: Some(api_base),
    };
    ping_chat_target(coordinator, &target).await
}

/// 测试通过后保存一条独立对话接入及其首个模型。
pub async fn save_chat_endpoint(
    database: &Database,
    coordinator: &RefreshCoordinator,
    display_name: &str,
    api_base_url: &str,
    secret: &str,
    model: &str,
) -> Result<RadarSnapshot, String> {
    let display_name = sanitize_endpoint_name(display_name)?;
    let api_base = sanitize_chat_base_url(api_base_url)?;
    let model = sanitize_custom_model(model)?;
    let secret = secret.trim();
    if secret.is_empty() {
        return Err("API Key 不能为空".into());
    }
    let target = ChatTarget {
        source_id: "radar-endpoint:draft".into(),
        adapter_id: OPENAI_COMPATIBLE_ADAPTER.into(),
        model: model.clone(),
        secret: secret.into(),
        api_base: Some(api_base.clone()),
    };
    ping_chat_target(coordinator, &target).await?;
    let id = format!("ep_{}_{}", epoch_ms(), std::process::id());
    let secret_ref = endpoint_secret_ref(&id);
    vault::set(&secret_ref, secret)?;
    let now = epoch_ms();
    if let Err(error) = database.insert_radar_chat_endpoint(&RadarChatEndpointRecord {
        id: id.clone(),
        display_name,
        api_base_url: api_base,
        secret_ref: secret_ref.clone(),
        created_at: now,
    }) {
        let _ = vault::delete(&secret_ref);
        return Err(error);
    }
    if let Err(error) = database.add_radar_chat_endpoint_model(&id, &model) {
        let _ = database.delete_radar_chat_endpoint(&id);
        let _ = vault::delete(&secret_ref);
        return Err(error);
    }
    snapshot(database)
}

pub fn delete_chat_endpoint(database: &Database, endpoint_id: &str) -> Result<RadarSnapshot, String> {
    let endpoint_id = endpoint_id.trim();
    if endpoint_id.is_empty() {
        return Err("接入 ID 不能为空".into());
    }
    let secret_ref = database
        .radar_chat_endpoint(endpoint_id)?
        .map(|item| item.secret_ref);
    database.delete_radar_chat_endpoint(endpoint_id)?;
    if let Some(secret_ref) = secret_ref {
        let _ = vault::delete(&secret_ref);
    }
    ensure_analysis_pref_valid(database)?;
    snapshot(database)
}

pub fn add_endpoint_model(
    database: &Database,
    source_id: &str,
    model: &str,
) -> Result<RadarSnapshot, String> {
    let model = sanitize_custom_model(model)?;
    let endpoint_id = parse_endpoint_id(source_id).ok_or_else(|| "不是独立对话接入".to_string())?;
    let endpoint = database
        .radar_chat_endpoint(endpoint_id)?
        .ok_or_else(|| "找不到该对话接入".to_string())?;
    let existing = database.list_radar_chat_endpoint_models(endpoint_id)?;
    if existing.iter().any(|item| item.model == model) {
        return Err("该接入已包含此模型".into());
    }
    let _ = vault::get(&endpoint.secret_ref)?
        .ok_or_else(|| "所选模型没有可用凭据".to_string())?;
    database.add_radar_chat_endpoint_model(endpoint_id, &model)?;
    snapshot(database)
}

pub fn delete_endpoint_model(
    database: &Database,
    source_id: &str,
    model: &str,
) -> Result<RadarSnapshot, String> {
    let model = sanitize_custom_model(model)?;
    let endpoint_id = parse_endpoint_id(source_id).ok_or_else(|| "不是独立对话接入".to_string())?;
    let remaining = database.list_radar_chat_endpoint_models(endpoint_id)?;
    if remaining.iter().filter(|item| item.model != model).count() == 0 {
        return Err("至少保留一个模型；要移除接入请删除整条对话接入".into());
    }
    database.delete_radar_chat_endpoint_model(endpoint_id, &model)?;
    ensure_analysis_pref_valid(database)?;
    snapshot(database)
}

fn ensure_analysis_pref_valid(database: &Database) -> Result<(), String> {
    let models = chat_models(database)?;
    let source = database.setting_string("radar_source_id")?.unwrap_or_default();
    let model = database.setting_string("radar_model")?.unwrap_or_default();
    let still = models
        .iter()
        .any(|item| item.ready && item.source_id == source && item.model == model);
    if still {
        return Ok(());
    }
    if let Some(first) = models
        .into_iter()
        .find(|item| item.ready && !item.model.is_empty())
    {
        database.set_setting_string("radar_source_id", &first.source_id)?;
        database.set_setting_string("radar_model", &first.model)?;
    } else {
        database.set_setting_string("radar_source_id", "")?;
        database.set_setting_string("radar_model", "")?;
    }
    Ok(())
}

fn sanitize_endpoint_name(raw: &str) -> Result<String, String> {
    let cleaned: String = raw
        .chars()
        .filter(|ch| *ch == ' ' || !ch.is_control())
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return Err("显示名称不能为空".into());
    }
    if trimmed.chars().count() > 64 {
        return Err("显示名称过长（最多 64 字）".into());
    }
    Ok(trimmed.to_string())
}

fn sanitize_chat_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("请求地址不能为空".into());
    }
    if trimmed.chars().any(|ch| ch.is_control() || ch == '\0') {
        return Err("请求地址包含非法字符".into());
    }
    if !trimmed.to_ascii_lowercase().starts_with("https://") {
        return Err("对话地址必须使用 https".into());
    }
    if trimmed.len() > 512 {
        return Err("请求地址过长".into());
    }
    Ok(trimmed.trim_end_matches('/').to_string())
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct ModelJson {
    #[serde(default)] post_outcomes: Vec<application::PostOutcome>,
    #[serde(default)] event_updates: Vec<application::EventUpdate>,
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
    /// 同批材料同时涉及另一类信号（重置卡/额度重置并存）时的次要信号类型。
    secondary_signal_type: Option<String>,
    secondary_citations: Vec<String>,
    secondary_event_relation: Option<String>,
    secondary_event_phase: Option<String>,
    /// AI 指定「预计重置时间」所在输入帖的别名（N*/C*）；后端只取该帖代码解析的时间。
    expected_time_post: Option<String>,
}

/// 对模型生成的雷达结论进行确定性清洗与安全截断：
/// 1. 剔除 markdown 符号、首尾空白；
/// 2. 剥离“本次新增帖子中”、“根据新增帖子”等无业务价值的前缀套话；
/// 3. 若模型误在结论中引用了别名（N1/N2…）或原始 ID，替换为“最新动态”，禁止展开为 17 字长标签；
/// 4. 执行银行重置术语规范化（“银行重置” -> “重置卡”）；
/// 5. 安全截断：上限放宽至 60 字符，优先在句末标点截断，无标点时取逗号或省略号，绝不生硬斩断中文字词。
pub(crate) fn sanitize_radar_conclusion(raw: &str, inputs: Option<&DeltaInputs>) -> String {
    let mut text = raw.replace(['`', '#', '*', '【', '】'], "").trim().to_string();

    const STRIP_PREFIXES: &[&str] = &[
        "本次新增帖子中，",
        "本次新增帖子中：",
        "本次新增帖子中",
        "在本次新增帖子中，",
        "在本次新增帖子中：",
        "在本次新增帖子中",
        "根据本次新增帖子，",
        "根据本次新增帖子：",
        "根据新增帖子，",
        "根据新增帖子：",
        "结合本次新增帖子，",
        "结合新增帖子，",
        "新增帖子显示，",
        "新增帖子显示：",
        "新增帖子中，",
        "新增帖子中：",
        "新增帖子中",
        "本次分析表明，",
        "分析表明，",
        "经分析，",
    ];
    for prefix in STRIP_PREFIXES {
        if let Some(stripped) = text.strip_prefix(prefix) {
            text = stripped.trim().to_string();
            break;
        }
    }

    if let Some(inputs) = inputs {
        for post in inputs.delta.iter().chain(inputs.context.iter()).chain(inputs.historical.iter()) {
            let alias = post_alias(&inputs.aliases, post);
            if !alias.is_empty() && text.contains(alias) {
                text = text.replace(alias, "最新动态");
            }
            if text.contains(&post.id) {
                text = text.replace(&post.id, "最新动态");
            }
        }
    }

    while text.contains("最新动态、最新动态") {
        text = text.replace("最新动态、最新动态", "最新动态");
    }
    while text.contains("最新动态和最新动态") {
        text = text.replace("最新动态和最新动态", "最新动态");
    }

    let mut cleaned = rewrite_banked_reset_zh(&text);

    const MAX_CONCLUSION_CHARS: usize = 60;
    if cleaned.chars().count() > MAX_CONCLUSION_CHARS {
        let chars: Vec<char> = cleaned.chars().collect();
        let mut cut_idx = None;
        for (i, &c) in chars.iter().enumerate().take(MAX_CONCLUSION_CHARS) {
            if matches!(c, '。' | '！' | '；') && i >= 10 {
                cut_idx = Some(i + 1);
            }
        }
        if let Some(idx) = cut_idx {
            cleaned = chars[..idx].iter().collect();
        } else {
            for (i, &c) in chars.iter().enumerate().take(MAX_CONCLUSION_CHARS) {
                if matches!(c, '，' | '、') && i >= 20 {
                    cut_idx = Some(i);
                }
            }
            if let Some(idx) = cut_idx {
                let mut s: String = chars[..idx].iter().collect();
                s.push('…');
                cleaned = s;
            } else {
                let mut s: String = chars[..58].iter().collect();
                s.push('…');
                cleaned = s;
            }
        }
    }

    cleaned = cleaned.trim().trim_end_matches(['、', '，', '：', ':']).to_string();
    if cleaned.ends_with("的最新帖") {
        cleaned = cleaned.strip_suffix("的最新帖").unwrap_or(&cleaned).trim().to_string();
    }
    if cleaned.ends_with("的最新帖子") {
        cleaned = cleaned.strip_suffix("的最新帖子").unwrap_or(&cleaned).trim().to_string();
    }
    if let Some(pos) = cleaned.rfind('、') {
        let tail = &cleaned[pos + '、'.len_utf8()..];
        if tail.chars().all(|c| c.is_ascii_digit() || matches!(c, '月' | '日' | ' ' | ':')) {
            cleaned = cleaned[..pos].trim().to_string();
        }
    }
    cleaned.trim().trim_end_matches(['、', '，']).to_string()
}

/// 分析依据自然语言清洗：剥离机械套话前缀，规整长日期标签，杜绝流水账与过长撑破。
pub fn sanitize_radar_analysis_basis(raw: &str, _inputs: Option<&DeltaInputs>) -> String {
    let mut text = raw.trim().to_string();
    if text.is_empty() {
        return text;
    }

    let prefixes = [
        "本次新增帖子中，",
        "本次新增帖子中：",
        "本次新增帖子中",
        "在本次新增帖子中，",
        "根据本次新增帖子，",
        "根据新增帖子，",
        "根据新增帖子：",
        "新增帖子中，",
        "新增帖子中：",
        "综合新增帖子分析，",
        "综合分析表明，",
        "分析表明，",
    ];
    for p in prefixes {
        if text.starts_with(p) {
            text = text.strip_prefix(p).unwrap_or(&text).trim().to_string();
            break;
        }
    }

    if let Ok(re_post) = regex::Regex::new(r"(?:\d+月\d+日\s*)?(\d{2}:\d{2})\s*(?:的最新)?(?:帖子|帖)") {
        text = re_post.replace_all(&text, "$1 动态").to_string();
    }

    text = text.replace("的最新帖子", "动态");
    text = text.replace("的最新帖", "动态");
    text = text.replace("动态和动态", "动态").replace("动态、动态", "动态");
    text = rewrite_banked_reset_zh(&text);

    if text.chars().count() > 120 {
        let puncts = ['。', '；', '！', ';'];
        let chars: Vec<char> = text.chars().collect();
        let mut cut = 120;
        for i in (40..=120.min(chars.len())).rev() {
            if i < chars.len() && puncts.contains(&chars[i]) {
                cut = i + 1;
                break;
            }
        }
        text = chars[..cut].iter().collect();
    }
    text.trim().trim_end_matches(['、', '，', ',', '：', ':']).to_string()
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
        &mut parsed.secondary_signal_type,
        &["banked_reset", "quota_reset", "none"],
    );
    allow(
        &mut parsed.secondary_event_relation,
        &["new_event", "same_event", "none"],
    );
    allow(
        &mut parsed.secondary_event_phase,
        &["watching", "upcoming", "landed_claimed"],
    );
    allow(
        &mut parsed.context_status,
        &["complete", "context_missing", "conflicting"],
    );
    if let Some(conclusion) = &mut parsed.conclusion {
        *conclusion = sanitize_radar_conclusion(conclusion, Some(inputs));
    }
    for update in &mut parsed.event_updates {
        update.conclusion = sanitize_radar_conclusion(&update.conclusion, Some(inputs));
    }
    parsed.analysis_basis = parsed
        .analysis_basis
        .as_deref()
        .map(|basis| sanitize_radar_analysis_basis(&humanize_post_refs(basis, inputs), Some(inputs)));
    let humanize_list = |items: &[String]| -> Vec<String> {
        items
            .iter()
            .map(|item| sanitize_radar_analysis_basis(&humanize_post_refs(item, inputs), Some(inputs)))
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
    // 次要信号引用与预计时间帖同样只接受已知别名；编造值一律丢弃。
    parsed.secondary_citations = parsed
        .secondary_citations
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
    parsed.expected_time_post = parsed
        .expected_time_post
        .as_deref()
        .and_then(|alias| alias_to_id.get(alias).copied())
        .map(str::to_string);
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
        post_outcomes: serde_json::from_value(parsed.get("postOutcomes").cloned().unwrap_or(json!([]))).map_err(|e| e.to_string())?,
        event_updates: serde_json::from_value(parsed.get("eventUpdates").cloned().unwrap_or(json!([]))).map_err(|e| e.to_string())?,
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
        secondary_signal_type: parsed
            .get("secondary_signal_type")
            .and_then(Value::as_str)
            .map(str::to_string),
        secondary_citations: string_list(&parsed, "secondary_citations"),
        secondary_event_relation: parsed
            .get("secondary_event_relation")
            .and_then(Value::as_str)
            .map(str::to_string),
        secondary_event_phase: parsed
            .get("secondary_event_phase")
            .and_then(Value::as_str)
            .map(str::to_string),
        expected_time_post: parsed
            .get("expected_time_post")
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
    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_before_wait_is_not_lost() {
        let control = super::RadarControl::default();
        assert!(control.try_begin());
        assert!(!control.try_begin());
        let generation = control.generation.load(std::sync::atomic::Ordering::Acquire);
        control.cancel();
        tokio::time::timeout(std::time::Duration::from_millis(100), control.wait_cancelled(generation))
            .await.expect("cancellation must not wait for a second notification");
        control.finish();
        assert!(control.try_begin());
    }
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
        assert!(DEFAULT_USER_PROMPT.contains("不要写成「银行重置」"));
        assert!(!DEFAULT_USER_PROMPT.contains("time_claims"));
        assert!(!DEFAULT_USER_PROMPT.contains("resolved_beijing_at"));
    }

    #[test]
    fn rewrite_banked_reset_zh_replaces_literal_bank_translation() {
        assert_eq!(
            rewrite_banked_reset_zh("来源称银行重置卡即将到账"),
            "来源称重置卡即将到账"
        );
        assert_eq!(
            rewrite_banked_reset_zh("这是银行重置，不是额度窗口刷新"),
            "这是重置卡，不是额度窗口刷新"
        );
        assert_eq!(
            rewrite_banked_reset_zh("Tibo announced a banked reset"),
            "Tibo announced 重置卡"
        );
        assert_eq!(rewrite_banked_reset_zh("额度重置已落地"), "额度重置已落地");
    }

    #[tokio::test]
    async fn enrichment_runs_after_success_without_blocking_and_releases_slot() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        let called = Arc::new(AtomicBool::new(false));
        for status in ["skipped", "off", "failed", "cancelled"] {
            let called = called.clone();
            assert!(start_post_enrichment(status, async move { called.store(true,Ordering::SeqCst); }).is_none());
        }
        assert!(!called.load(Ordering::SeqCst));
        let (started_tx, started_rx)=tokio::sync::oneshot::channel();
        let (release_tx, release_rx)=tokio::sync::oneshot::channel::<()>();
        let handle=start_post_enrichment("success", async move {
            started_tx.send(()).unwrap();
            let _=release_rx.await;
        }).unwrap();
        // The caller already has its result while translation is deliberately blocked.
        tokio::time::timeout(Duration::from_secs(1),started_rx).await.unwrap().unwrap();
        assert!(start_post_enrichment("success",async {}).is_none());
        release_tx.send(()).unwrap();
        handle.await.unwrap();
        let timed=start_post_enrichment("cached",async {
            let _=tokio::time::timeout(Duration::from_millis(5),std::future::pending::<()>()).await;
        }).unwrap();
        timed.await.unwrap();
        start_post_enrichment("success",async {}).unwrap().await.unwrap();
    }

    #[test]
    fn sanitize_radar_conclusion_cleans_prefixes_and_truncates_safely() {
        assert_eq!(
            sanitize_radar_conclusion("本次新增帖子中，官方已宣布额度全面重置。", None),
            "官方已宣布额度全面重置。"
        );
        assert_eq!(
            sanitize_radar_conclusion("根据新增帖子：额度窗口已刷新。", None),
            "额度窗口已刷新。"
        );
        assert_eq!(
            sanitize_radar_conclusion("本次新增帖子中，9月8日 12:05 的最新帖子、9月8日 12:07 的最新帖", None),
            "9月8日 12:05 的最新帖子"
        );
        assert_eq!(
            sanitize_radar_conclusion("官方宣布银行重置卡已经发放", None),
            "官方宣布重置卡已经发放"
        );
        // 超长文本安全截断不硬切字词
        let long_conclusion = "官方宣布全球付费用户额度已全部完成重置，各区域使用窗口均已实际恢复正常。后续将继续观察系统承载能力。";
        let sanitized = sanitize_radar_conclusion(long_conclusion, None);
        assert!(sanitized.ends_with('。') || sanitized.ends_with('…'));
        assert!(sanitized.chars().count() <= 60);
    }

    #[test]
    fn sanitize_radar_analysis_basis_cleans_prefixes_and_repetitive_labels() {
        let raw = "本次新增帖子中，9月8日 12:05 的最新帖子直接宣布所有用户的额度均已重置。";
        let cleaned = sanitize_radar_analysis_basis(raw, None);
        assert_eq!(cleaned, "12:05 动态直接宣布所有用户的额度均已重置。");

        let long = "官方宣布全员额度重置。后续动态补充提及中途刷新两次，确认窗口已实际刷新，非未来重置卡。后续各区域持续稳定运行，未见异常回退或限制。";
        let cleaned_long = sanitize_radar_analysis_basis(long, None);
        assert!(cleaned_long.chars().count() <= 120);
        assert!(cleaned_long.ends_with('。') || cleaned_long.ends_with('…'));
    }

    #[tokio::test]
    #[ignore]
    async fn explicit_live_feed_and_model_check() {
        let path=std::env::var("AQM_NETWORK_DB").expect("explicit disposable copy required");
        let db=Database::initialize_at(path.into()).unwrap();
        let client=Client::builder().timeout(std::time::Duration::from_secs(90)).build().unwrap();
        let feed=fetch_feed(&client).await.map_err(|(a,b)|format!("{a}; {b}")).unwrap();
        println!("live feed codex={} will={} source_errors={}",feed.codex_count,feed.will_count,usize::from(feed.codex_error.is_some())+usize::from(feed.will_error.is_some()));
        db.replace_tibo_posts(&feed.posts,epoch_ms()).unwrap();
        reconcile_event_state(&db,epoch_ms()).unwrap();
        let inputs=collect_delta_inputs(&db).unwrap();
        println!("live new material count={}",inputs.delta.len());
        if !inputs.delta.is_empty() {
            let prefs=load_analysis_prefs(&db).unwrap();
            let target=resolve_chat_target(&db,prefs.source_id.as_deref(),prefs.model.as_deref()).unwrap();
            run_analysis_inner(&db,&client,&inputs,&target,&prefs.user_prompt,"explicit-live-validation","context","prompt").await.unwrap();
            println!("live model validated and applied; active events={}",db.active_radar_events().unwrap().len());
        }
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
            post_outcomes: Vec::new(), event_updates: Vec::new(),
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
            secondary_signal_type: None,
            secondary_citations: Vec::new(),
            secondary_event_relation: None,
            secondary_event_phase: None,
            expected_time_post: None,
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
        let inputs = collect_delta_inputs(&database).unwrap();
        assert!(inputs.delta.iter().all(|post| post.id != "old-signal"));
        assert!(inputs.historical.iter().any(|post| post.id == "old-signal"));
        let parsed = parsed_signal("new_event", vec!["old-signal"]);
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
        let inputs = collect_delta_inputs(&database).unwrap();
        assert_eq!(
            inputs.delta.iter().map(|post| post.id.as_str()).collect::<Vec<_>>(),
            vec!["queued"]
        );
        // 已消费帖子不再进入 NEW 组；分析输入与浏览分页独立，不再依赖历史组承载已消费帖。
        assert!(inputs.delta.iter().all(|post| post.id != "consumed"));
        database
            .mark_tibo_posts_consumed(&["queued".into()], epoch_ms())
            .unwrap();
        let after = collect_delta_inputs(&database).unwrap();
        assert!(after.delta.is_empty());
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
        confirm_user_reset(&database, None, None).unwrap();
        let confirmed = database.active_radar_event().unwrap().expect("active");
        assert!(confirmed.user_confirmed_reset_at.is_some());
        assert_eq!(
            confirmed.expires_at,
            confirmed
                .user_confirmed_reset_at
                .map(|at| at + 24 * 3_600_000)
        );
        undo_user_reset(&database, None, None).unwrap();
        let restored = database.active_radar_event().unwrap().expect("active");
        assert!(restored.user_confirmed_reset_at.is_none());
        assert_eq!(restored.phase, "landed_claimed");
        confirm_user_reset(&database, Some("event-active"), Some(restored.state_revision)).unwrap();
        let confirmed_again = database.radar_event("event-active").unwrap().unwrap();
        assert!(undo_user_reset(&database, Some("event-active"), Some(confirmed_again.state_revision - 1)).is_err());
        let mut closed = confirmed_again;
        closed.closed_at = Some(epoch_ms());
        closed.phase = "closed".into();
        closed.state_revision += 1;
        database.update_radar_event(&closed).unwrap();
        assert!(undo_user_reset(&database, Some("event-active"), Some(closed.state_revision)).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn secondary_signal_creates_parallel_event_of_other_type() {
        let (database, path) = temp_db();
        let now = epoch_ms();
        database
            .replace_tibo_posts(
                &[
                    sample_post("quota-post", now - 3_600_000, "direct", true),
                    sample_post("banked-post", now - 1_800_000, "signal", false),
                ],
                now,
            )
            .unwrap();
        let inputs = collect_delta_inputs(&database).unwrap();
        let mut parsed = parsed_signal("new_event", vec!["quota-post"]);
        parsed.signal_type = Some("quota_reset".into());
        parsed.secondary_signal_type = Some("banked_reset".into());
        parsed.secondary_citations = vec!["banked-post".into()];
        parsed.secondary_event_relation = Some("new_event".into());
        parsed.secondary_event_phase = Some("watching".into());
        let (event_id, replay) =
            apply_analysis_to_event(&database, &inputs, &parsed, "analysis-dual").unwrap();
        assert!(!replay);
        // 主信号事件为 quota_reset；次要信号并行创建 banked_reset 事件，互不关闭。
        let quota = database
            .active_radar_event_of_type("quota_reset")
            .unwrap()
            .expect("quota event");
        assert_eq!(event_id.as_deref(), Some(quota.id.as_str()));
        let banked = database
            .active_radar_event_of_type("banked_reset")
            .unwrap()
            .expect("banked event");
        assert_eq!(banked.event_type, "banked_reset");
        assert!(banked.closed_at.is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn observations_link_idempotently_when_event_identified_later() {
        let (database, path) = temp_db();
        let now = epoch_ms();
        let observed_at = now - 3_600_000;
        // 本机先观察到计划外重置（事件尚未识别，observation 独立保存）。
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO capability_snapshots(account_id, source_id, capability_id, display_name, value_kind, primary_value, secondary_value, progress, trend_json, captured_at, generation, window_seconds, reset_at)
                 VALUES ('openai-codex-local','openai-codex-local','quota_window_7d','7 天窗口','percent','10%',NULL,0.1,'[]',?1,1,604800,?2),
                        ('openai-codex-local','openai-codex-local','quota_window_7d','7 天窗口','percent','95%',NULL,0.95,'[]',?3,1,604800,?4)",
                rusqlite::params![
                    observed_at - 600_000,
                    observed_at + 172_800_000,
                    observed_at,
                    observed_at + 604_800_000,
                ],
            )
            .unwrap();
        drop(connection);
        // 事件在观察之后才被识别（first_signal_at 晚于 observed_at，但在 24h 回看窗口内）。
        let mut event = RadarEventRecord {
            id: "event-late".into(),
            phase: "upcoming".into(),
            title: "预计即将重置".into(),
            summary: None,
            first_signal_at: now - 1_800_000,
            latest_evidence_at: now - 1_800_000,
            claimed_landed_at: None,
            observed_reset_at: None,
            closed_at: None,
            close_reason: None,
            expected_at: Some(now + 3_600_000),
            expires_at: None,
            state_revision: 1,
            user_confirmed_reset_at: None,
            event_type: "quota_reset".into(),
        };
        event.expires_at = event_expiry(&event);
        database.insert_radar_event(&event).unwrap();
        reconcile_event_state(&database, now).unwrap();
        let linked = database
            .quota_reset_observations(None, Some("event-late"))
            .unwrap();
        assert_eq!(linked.len(), 1, "先观察到变化、后识别出事件应补关联");
        // 重复执行不产生重复记录。
        reconcile_event_state(&database, now).unwrap();
        let again = database
            .quota_reset_observations(None, Some("event-late"))
            .unwrap();
        assert_eq!(again.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn deadline_claims_do_not_become_expected_reset_time() {
        let (database, path) = temp_db();
        let now = epoch_ms();
        // 一条预告时间（明晚 6pm PST）+ 一条更晚的截止时间（后天 11pm PST）。
        let text = "Reset will land tomorrow 6pm PST. Deadline to use your old credits is Friday 11pm PST.";
        database
            .replace_tibo_posts(
                &[TiboPostRecord {
                    id: "post-claim".into(),
                    url: "https://x.com/tibo/status/1".into(),
                    text: text.into(),
                    posted_at: now - 3_600_000,
                    kind: "direct".into(),
                    tibo_lane: None,
                    explicit_reset: true,
                    verification_status: None,
                    is_reply: false,
                    replies: 0,
                    reposts: 0,
                    likes: 0,
                    extra_json: json!({ "relevance": "direct" }).to_string(),
                    synced_at: now,
                    translated_text: None,
                    translated_at: None,
                    translation_source: None,
                    lifecycle_consumed_at: None,
                }],
                now,
            )
            .unwrap();
        refresh_time_claims(&database).unwrap();
        let mut event = RadarEventRecord {
            id: "event-claim".into(),
            phase: "upcoming".into(),
            title: "预计即将重置".into(),
            summary: None,
            first_signal_at: now - 3_600_000,
            latest_evidence_at: now - 3_600_000,
            claimed_landed_at: None,
            observed_reset_at: None,
            closed_at: None,
            close_reason: None,
            expected_at: None,
            expires_at: None,
            state_revision: 1,
            user_confirmed_reset_at: None,
            event_type: "quota_reset".into(),
        };
        event.expires_at = event_expiry(&event);
        database.insert_radar_event(&event).unwrap();
        database
            .add_radar_event_evidence("event-claim", "post-claim", "delta", "analysis-claim")
            .unwrap();
        reconcile_event_state(&database, now).unwrap();
        let refreshed = database.radar_event("event-claim").unwrap().expect("event");
        let claims = database
            .radar_time_claims_for_posts(&["post-claim".into()])
            .unwrap();
        assert!(
            claims.iter().any(|claim| claim.claim_kind == "deadline"),
            "截止语义应被识别为 deadline"
        );
        if let (Some(grant), Some(deadline)) = (
            claims
                .iter()
                .find(|claim| claim.claim_kind != "deadline" && claim.resolved_at.is_some())
                .and_then(|claim| claim.resolved_at),
            claims
                .iter()
                .find(|claim| claim.claim_kind == "deadline")
                .and_then(|claim| claim.resolved_at),
        ) {
            assert!(
                grant < deadline,
                "预计重置时间应取预告时间而不是更晚的截止时间"
            );
            assert_eq!(refreshed.expected_at, None, "协调不能擅自选用时间");
            let inputs = collect_delta_inputs(&database).unwrap();
            assert_eq!(expected_from_posts(&inputs, &inputs.delta.iter().collect::<Vec<_>>()), Some(grant));
        } else {
            panic!("时间声明未解析出可比较的预告/截止时间");
        }
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
    fn latest_confirmed_reset_event_ignores_banked_grant() {
        let (database, path) = temp_db();
        let now = epoch_ms();
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
        database
            .insert_radar_event(&RadarEventRecord {
                id: "event-banked".into(),
                phase: "closed".into(),
                title: "本机已观察到重置卡到账".into(),
                summary: None,
                first_signal_at: now - 8 * 3_600_000,
                latest_evidence_at: now - 2 * 3_600_000,
                claimed_landed_at: None,
                observed_reset_at: Some(now - 2 * 3_600_000),
                closed_at: Some(now - 60_000),
                close_reason: Some("completed".into()),
                expected_at: None,
                expires_at: None,
                state_revision: 1,
                user_confirmed_reset_at: None,
                event_type: "banked_reset".into(),
            })
            .unwrap();
        let confirmed = database
            .latest_confirmed_reset_event()
            .unwrap()
            .expect("confirmed");
        assert_eq!(confirmed.id, "event-observed");
        assert_eq!(confirmed.event_type, "quota_reset");
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
        let inputs = collect_delta_inputs(&database).unwrap();
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

    #[test]
    fn chat_base_url_requires_https_and_normalizes() {
        assert!(sanitize_chat_base_url("http://api.openai.com/v1").is_err());
        assert!(sanitize_chat_base_url("ftp://example.com").is_err());
        assert_eq!(
            sanitize_chat_base_url("https://api.openai.com/v1/").unwrap(),
            "https://api.openai.com/v1"
        );
        let url = join_chat_url("https://api.x.ai/v1", "/chat/completions");
        assert_eq!(url, "https://api.x.ai/v1/chat/completions");
        let already = join_chat_url(
            "https://openrouter.ai/api/v1/chat/completions",
            "/chat/completions",
        );
        assert_eq!(already, "https://openrouter.ai/api/v1/chat/completions");
    }

    #[test]
    fn parse_endpoint_source_id() {
        assert_eq!(parse_endpoint_id("radar-endpoint:ep_1"), Some("ep_1"));
        assert_eq!(parse_endpoint_id("deepseek-balance-api"), None);
    }

    #[test]
    fn empty_window_inputs_return_ok_without_failing() {
        let (database, path) = temp_db();
        let inputs = collect_delta_inputs(&database).unwrap();
        assert!(inputs.delta.is_empty());
        assert!(inputs.context.is_empty());
        assert!(inputs.historical.is_empty());

        let groups = ClassifiedPosts {
            mode: "live_delta",
            range_posts: Vec::new(),
            new_posts: Vec::new(),
            event_context: Vec::new(),
            historical: Vec::new(),
        };
        let checks = Vec::new();
        let ai = build_ai_assessment(&database, None, None, &groups, &checks, true).unwrap();
        assert_ne!(ai.state, "failed");
        assert_eq!(ai.state, "pending");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn merge_tibo_posts_deduplicates_by_id_and_preserves_translation() {
        let codex_post = TiboPostRecord {
            id: "2096494725318762951".into(),
            url: "https://x.com/thsottiaux/status/2096494725318762951".into(),
            text: "Looking at the dashboard".into(),
            posted_at: 100,
            kind: "indirect".into(),
            tibo_lane: Some("间接相关".into()),
            explicit_reset: false,
            verification_status: None,
            is_reply: false,
            replies: 0,
            reposts: 0,
            likes: 0,
            extra_json: json!({"summary": "摘要", "analysis": "解读"}).to_string(),
            synced_at: 100,
            translated_text: Some("查看仪表盘".into()),
            translated_at: Some(100),
            translation_source: Some("codexradar".into()),
            lifecycle_consumed_at: None,
        };

        let will_post_same = TiboPostRecord {
            id: "2096494725318762951".into(),
            url: "https://x.com/thsottiaux/status/2096494725318762951".into(),
            text: "Looking at the dashboard".into(),
            posted_at: 100,
            kind: "none".into(),
            tibo_lane: None,
            explicit_reset: false,
            verification_status: None,
            is_reply: false,
            replies: 0,
            reposts: 0,
            likes: 0,
            extra_json: json!({"source": "willcodex", "context": "引用的上下文"}).to_string(),
            synced_at: 110,
            translated_text: None,
            translated_at: None,
            translation_source: None,
            lifecycle_consumed_at: None,
        };

        let will_post_new = TiboPostRecord {
            id: "2096717905614524491".into(),
            url: "https://x.com/thsottiaux/status/2096717905614524491".into(),
            text: "We've made some improvements...".into(),
            posted_at: 200,
            kind: "reset".into(),
            tibo_lane: Some("可能重置".into()),
            explicit_reset: true,
            verification_status: None,
            is_reply: false,
            replies: 0,
            reposts: 0,
            likes: 0,
            extra_json: json!({"source": "willcodex"}).to_string(),
            synced_at: 110,
            translated_text: None,
            translated_at: None,
            translation_source: None,
            lifecycle_consumed_at: None,
        };

        let merged = merge_tibo_posts(vec![codex_post], vec![will_post_same, will_post_new]);

        // 验证去重：总数必须为 2（而不是 3）
        assert_eq!(merged.len(), 2);
        // 按时间倒序：最新的在最前面
        assert_eq!(merged[0].id, "2096717905614524491");
        assert_eq!(merged[1].id, "2096494725318762951");

        // 验证属性融合：重复的 ID 保留了 CodexRadar 的中文翻译与语境解读，同时融合了上下文
        let same = &merged[1];
        assert_eq!(same.translated_text.as_deref(), Some("查看仪表盘"));
        assert_eq!(same.kind, "indirect");
        assert!(same.extra_json.contains("摘要"));
        assert!(same.extra_json.contains("引用的上下文"));
    }
}
