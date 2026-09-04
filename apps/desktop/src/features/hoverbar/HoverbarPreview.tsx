/**
 * 悬浮球组件开发预览：只在 Vite 开发环境手工验收，不参与 Tauri 产品入口。
 * 示例值均明确标注为预览数据，避免与真实平台快照混淆。
 */
import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ExternalLink, Moon, RefreshCw, SunMedium, X } from "lucide-react";
import type {
  CapabilitySnapshotViewModel,
  PlatformSummaryViewModel,
  SourceAccessMode,
  SourceState,
  SourceSummaryViewModel,
} from "@/lib/types";
import type {
  RadarAnalysis,
  RadarDecision,
  RadarEvent,
  QuotaVerification,
  RadarSnapshot,
} from "@/lib/ipc";
import "@/styles/global.css";
import { HoverbarOrb } from "./HoverbarAnchorApp";
import { HoverbarPlatformCard } from "./HoverbarPlatformCard";
import { HoverbarRadarDetail } from "./HoverbarRadarDetail";
import { useHoverbarTheme } from "./hoverbar-theme";
import type { HoverbarEdge } from "./hoverbar-state";

const noop = () => undefined;

declare global {
  interface Window {
    __HOVERBAR_PREVIEW_ROOT__?: ReturnType<typeof ReactDOM.createRoot>;
  }
}

function source(
  sourceId: string,
  displayName: string,
  capabilityIds: string[],
  accessMode: SourceAccessMode,
  state: SourceState = "ready",
  credentialConfigured = true,
): SourceSummaryViewModel {
  const accountId = sourceId === "openai-codex-local"
    ? "openai-local"
    : sourceId.startsWith("openai-codex-extra-")
      ? "openai-extra-2"
      : sourceId.startsWith("glm-lite")
        ? "glm-lite"
        : sourceId.startsWith("glm-")
          ? "glm-default"
          : sourceId.startsWith("kimi-")
            ? "kimi-default"
            : "deepseek-default";
  return {
    sourceId,
    adapterId: sourceId.startsWith("openai-codex-extra-") ? "openai-codex-local" : sourceId,
    accountId,
    accountName: accountId === "openai-local" ? "本机 Codex" : accountId === "openai-extra-2" ? "额外账号 2" : "默认账号",
    accountKind: accountId === "openai-local" ? "local" : accountId === "openai-extra-2" ? "additional" : "default",
    sourceType:
      accessMode === "local_cli"
        ? "local_cli"
        : accessMode === "personal_balance" || accessMode === "web_usage"
          ? "web_session"
          : "api_key",
    displayName,
    state,
    credentialConfigured,
    lastValidatedAt: null,
    lastSuccessAt: null,
    errorCode: state === "error" ? "PREVIEW_ERROR" : null,
    errorMessage: state === "error" ? "示例：暂时无法刷新" : null,
    capabilityIds,
    accessMode,
  };
}

function cap(
  capabilityId: string,
  sourceId: string,
  displayName: string,
  primary: string | null,
  secondary: string | null,
  freshness: CapabilitySnapshotViewModel["freshness"] = "fresh",
  progress: number | null = null,
): CapabilitySnapshotViewModel {
  return {
    capabilityId,
    sourceId,
    accountId: sourceId === "openai-codex-local"
      ? "openai-local"
      : sourceId.startsWith("openai-codex-extra-")
        ? "openai-extra-2"
        : sourceId.startsWith("glm-lite")
          ? "glm-lite"
          : sourceId.startsWith("glm-")
            ? "glm-default"
            : sourceId.startsWith("kimi-")
              ? "kimi-default"
              : "deepseek-default",
    displayName,
    freshness,
    capturedAt: null,
    lastGoodAt: null,
    value: {
      kind: capabilityId === "balance" ? "money" : capabilityId === "cache_hit_rate" ? "percent" : "tokens",
      primary,
      secondary,
      progress,
    },
    trend: [],
  };
}

const gptPlatform: PlatformSummaryViewModel = {
  providerId: "openai",
  displayName: "GPT / Codex",
  aggregateStatus: "healthy",
  accessSummary: "本机 Codex + 额外账号",
  supportsMultipleAccounts: true,
  accounts: [
    { accountId: "openai-local", displayName: "本地 Codex 账户", kind: "local", status: "healthy", sourceIds: ["openai-codex-local"], canRename: false, canRemove: false },
    { accountId: "openai-extra-2", displayName: "额外 ChatGPT 账号 2", kind: "additional", status: "healthy", sourceIds: ["openai-codex-extra-2"], canRename: true, canRemove: true },
    { accountId: "openai-extra-3", displayName: "工作账号", kind: "additional", status: "setup_required", sourceIds: ["openai-codex-extra-3"], canRename: true, canRemove: true },
  ],
  sources: [
    source("openai-codex-local", "本机 Codex", ["quota_window_5h", "quota_window_7d", "credits", "plan_level"], "local_cli"),
    source("openai-codex-extra-2", "额外 ChatGPT 账号 2", ["quota_window_30d", "credits", "plan_level"], "local_cli"),
    source("openai-codex-extra-3", "工作账号", [], "local_cli", "auth_required", false),
  ],
  capabilities: [
    cap("quota_window_5h", "openai-codex-local", "本机 · 5 小时窗口", "62%", "已使用 38.0% · 重置 14:30"),
    cap("quota_window_7d", "openai-codex-local", "本机 · 7 天窗口", "81%", "已使用 19.0% · 重置 09/02 08:00"),
    cap("credits", "openai-codex-local", "本机 · Credits", "12.34", "仅展示额度接口实际返回值"),
    cap("plan_level", "openai-codex-local", "本机 · 订阅计划", "Plus", "ChatGPT / Codex 订阅"),
    cap("quota_window_30d", "openai-codex-extra-2", "账号 2 · 30 天窗口", "68%", "已使用 32% · 重置 09/28 00:00"),
    cap("credits", "openai-codex-extra-2", "账号 2 · Credits", "3.21", null),
    cap("plan_level", "openai-codex-extra-2", "账号 2 · 订阅计划", "Free", null),
  ],
};

const glmPlatform: PlatformSummaryViewModel = {
  providerId: "glm",
  displayName: "GLM 国内",
  aggregateStatus: "healthy",
  accessSummary: "Token Plan + 个人余额",
  supportsMultipleAccounts: true,
  accounts: [
    { accountId: "glm-default", displayName: "默认账号", kind: "default", status: "healthy", sourceIds: ["glm-coding-plan", "glm-web-balance"], canRename: false, canRemove: false },
    { accountId: "glm-lite", displayName: "个人 GLM", kind: "additional", status: "healthy", sourceIds: ["glm-lite-plan"], canRename: true, canRemove: true },
  ],
  sources: [
    source("glm-coding-plan", "Coding Plan", ["quota_window_5h", "quota_window_7d", "plan_level"], "coding_plan"),
    source("glm-web-balance", "网页个人余额", ["balance"], "personal_balance"),
    source("glm-lite-plan", "个人 Coding Plan", ["quota_window_5h", "quota_window_7d", "plan_level"], "coding_plan"),
  ],
  capabilities: [
    cap("quota_window_5h", "glm-coding-plan", "5 小时窗口", "72%", "已使用 28.0% · 重置 15:20"),
    cap("quota_window_7d", "glm-coding-plan", "周窗口", "80%", "已使用 20.0% · 重置 09/03 09:00"),
    cap("plan_level", "glm-coding-plan", "订阅计划", "Pro", "官方 Coding Plan"),
    cap("balance", "glm-web-balance", "账户余额", "¥88.10", "网页个人余额"),
    cap("quota_window_5h", "glm-lite-plan", "个人 · 5 小时窗口", "100%", "已使用 0% · 重置 09/02 22:59"),
    cap("quota_window_7d", "glm-lite-plan", "个人 · 周窗口", "0%", "重置 09/06 22:59"),
    cap("plan_level", "glm-lite-plan", "个人 · 订阅计划", "Lite", "个人 GLM 套餐"),
  ],
};

const deepseekBalanceSource = "preview-ds-balance";
const deepseekWebSource = "preview-ds-web";

/** DeepSeek 全 fresh：资金 + V4 Flash/Pro 模型行 + 靶心缓存命中率（含后端 secondary 说明）。 */
const deepseekHealthy: PlatformSummaryViewModel = {
  providerId: "deepseek",
  displayName: "DeepSeek",
  aggregateStatus: "healthy",
  accessSummary: "API Key + 网页会话",
  supportsMultipleAccounts: true,
  accounts: [{ accountId: "deepseek-default", displayName: "默认账号", kind: "default", status: "healthy", sourceIds: [deepseekBalanceSource, deepseekWebSource], canRename: false, canRemove: false }],
  sources: [
    source(deepseekBalanceSource, "API 余额", ["balance"], "personal_balance"),
    source(
      deepseekWebSource,
      "网页用量与缓存",
      ["today_spend", "month_spend", "model_usage_v4_flash", "model_usage_v4_flash_vision", "model_usage_v4_pro", "cache_hit_rate"],
      "web_usage",
    ),
  ],
  capabilities: [
    cap("balance", deepseekBalanceSource, "充值余额", "¥25.00", null),
    cap("today_spend", deepseekWebSource, "今日消费", "¥7.42", null),
    cap("month_spend", deepseekWebSource, "本月消费", "¥24.63", null),
    cap("model_usage_v4_flash", deepseekWebSource, "V4 Flash 用量", "181.25M", null),
    // 真实零值场景（对齐 V2 设计稿）：Vision 未产生用量时显示后端真实 0，不是 missing
    cap("model_usage_v4_flash_vision", deepseekWebSource, "V4 Flash Vision 用量", "0", null),
    cap("model_usage_v4_pro", deepseekWebSource, "V4 Pro 用量", "1.94M", null),
    cap("cache_hit_rate", deepseekWebSource, "缓存命中率", "97.3%", "命中 181.25M / 输入 234.52M", "fresh", 0.973),
  ],
};

/** DeepSeek 余额实时 + 网页用量 stale：保留真实值，仅展示缓存提示。 */
const deepseekWebStale: PlatformSummaryViewModel = {
  ...deepseekHealthy,
  aggregateStatus: "partial",
  capabilities: deepseekHealthy.capabilities.map((item) =>
    item.sourceId === deepseekWebSource ? { ...item, freshness: "stale" as const } : item,
  ),
};

/** DeepSeek 余额 missing + 网页用量 fresh：missing 不补零，只展示「暂不可用」。 */
const deepseekBalanceMissing: PlatformSummaryViewModel = {
  ...deepseekHealthy,
  aggregateStatus: "partial",
  capabilities: deepseekHealthy.capabilities.map((item) =>
    item.sourceId === deepseekBalanceSource
      ? { ...item, freshness: "missing" as const, value: { ...item.value, primary: null, secondary: null } }
      : item,
  ),
};

/** DeepSeek 全 missing：两个来源都无最后成功快照，不补零。 */
const deepseekAllMissing: PlatformSummaryViewModel = {
  ...deepseekHealthy,
  aggregateStatus: "error",
  capabilities: deepseekHealthy.capabilities.map((item) => ({
    ...item,
    freshness: "missing" as const,
    value: { ...item.value, primary: null, secondary: null, progress: null },
  })),
};

const kimiError: PlatformSummaryViewModel = {
  providerId: "kimi",
  displayName: "Kimi",
  aggregateStatus: "error",
  accessSummary: "个人余额",
  supportsMultipleAccounts: true,
  accounts: [{ accountId: "kimi-default", displayName: "默认账号", kind: "default", status: "error", sourceIds: ["kimi-balance-api"], canRename: false, canRemove: false }],
  sources: [source("kimi-balance-api", "个人余额", ["balance"], "personal_balance", "error")],
  capabilities: [cap("balance", "kimi-balance-api", "账户余额", null, null, "missing")],
};

const staleGlm: PlatformSummaryViewModel = {
  ...glmPlatform,
  aggregateStatus: "partial",
  capabilities: glmPlatform.capabilities.map((item) =>
    item.capabilityId === "balance" ? { ...item, freshness: "stale" as const } : item,
  ),
};

const previewPlatforms: PlatformSummaryViewModel[] = [gptPlatform, glmPlatform, deepseekHealthy, kimiError];

/** 预览分析记录：生命周期矩阵的共用示例分析。 */
const previewAnalysis: RadarAnalysis = {
  id: "preview-analysis",
  createdAt: Date.now() - 39 * 60 * 1000,
  rangeKey: "3d",
  cutPostId: "2094252447271366730",
  cutLabel: null,
  sourceId: "deepseek-balance-api",
  model: "deepseek-chat",
  coversLatest: true,
  conclusion: "示例结论：按钮已按下，等待本机额度验证。",
  analysisBasis: "Tibo 明确宣布按下重置按钮，庆祝推迟至明天，符合强信号特征。",
  confidence: "high",
  citations: [],
  support: ["明确宣布按下按钮", "庆祝活动推迟"],
  against: [],
  uncertainty: ["全量覆盖范围未知"],
  newPostIds: ["2094252447271366730"],
  eventContextPostIds: ["preview-1"],
  historicalPostIds: [],
  errorMessage: null,
  eventId: "preview-event",
  analysisMode: "delta",
  eventRelation: "same_event",
  eventPhase: "landed_claimed",
  deltaEffect: "reinforce",
  signalLevel: "strong",
  contextStatus: "complete",
  signalType: "quota_reset",
  temporalPhase: "observed_landed",
  validUntil: null,
  timeClaims: [
    {
      postId: "2094252447271366730",
      rawText: "6pm PST",
      parseStatus: "resolved",
      timezoneKind: "PST",
      timezoneAssumed: false,
      resolvedAt: Date.now() - 20 * 60 * 60 * 1000,
      precision: "exact",
    },
  ],
};

/** 预览事件：来源称已落地阶段，等待本机验证。 */
const previewEvent: RadarEvent = {
  id: "preview-event",
  phase: "landed_claimed",
  title: "按钮今日已按下，等待本机额度验证",
  summary: "新帖 2094252447271366730 宣布按下重置按钮，庆祝活动推迟至明天。",
  firstSignalAt: Date.now() - 26 * 60 * 60 * 1000,
  latestEvidenceAt: Date.now() - 39 * 60 * 1000,
  claimedLandedAt: Date.now() - 20 * 60 * 60 * 1000,
  observedResetAt: null,
  closedAt: null,
  timeline: [
    { at: Date.now() - 26 * 60 * 60 * 1000, kind: "signal", label: "首次信号" },
    { at: Date.now() - 20 * 60 * 60 * 1000, kind: "claimed", label: "来源称已落地" },
  ],
  postIds: ["2094252447271366730", "preview-1"],
  expectedAt: Date.now() - 20 * 60 * 60 * 1000,
  expiresAt: Date.now() + 24 * 60 * 60 * 1000,
  stateRevision: 3,
  temporalStatus: "claimed_landed",
  userConfirmedResetAt: null,
  eventType: "quota_reset",
};

/** 按状态构建预览决策，保证与事件 mock 同源一致（仅布局验收用）。 */
function previewDecision(overrides: Partial<RadarDecision> = {}): RadarDecision {
  const claimedAt = previewEvent.claimedLandedAt;
  return {
    status: "landed_claimed",
    activeEventId: "preview-event",
    headline: "来源称已经重置",
    timeText: "来源于 09-01 14:06 声称额度已重置",
    verificationHint: "等待本机检测或用户确认",
    observationPeriodText: null,
    expectedAt: previewEvent.expectedAt,
    observedAt: null,
    claimedAt,
    userConfirmedAt: null,
    observationExpiresAt: null,
    timeKind: "claimed",
    signalLevel: "strong",
    recentReset: null,
    recentClosedEvent: null,
    relevantPostIds: [...previewEvent.postIds],
    currentKeyCitationIds: ["2094252447271366730"],
    historicalCitationIds: [],
    latestIrrelevantUpdateAt: null,
    canConfirmReset: true,
    canUndoConfirm: false,
    pendingUnconsumedCount: 0,
    sourceHasDirectSignal: true,
    stripBadge: "待验证",
    stripPrimary: "来源称已经重置",
    stripPrimaryCompact: "来源称已经重置",
    stripSecondary: "等待本机检测或用户确认",
    recentSummaryText: null,
    deltaImpactText: "最新动态已分析，未改变当前判断",
    eventType: "quota_reset",
    ...overrides,
  };
}

/** 预览本机额度验证：本机观察到非计划刷新，额外账号未见变化。 */
const previewQuota: QuotaVerification[] = [
  {
    accountId: "openai-local",
    accountName: "本机 Codex",
    sourceId: "openai-codex-local",
    status: "unscheduled_reset",
    attribution: "radar_correlated",
    observationId: 1000 + 1,
    temporalCorrelation: "high",
    windowId: "quota_window_7d",
    windowLabel: "7 天窗口",
    windowSeconds: 604800,
    previous: { capturedAt: Date.now() - 30 * 60 * 60 * 1000, remaining: 0.12, resetAt: Date.now() + 60 * 60 * 1000 },
    current: { capturedAt: Date.now() - 8 * 60 * 1000, remaining: 0.94, resetAt: Date.now() + 7 * 24 * 60 * 60 * 1000 },
    lastSuccessAt: Date.now() - 8 * 60 * 1000,
    note: "未到原定时间窗口已恢复，重置时间明显后移",
    lastResetObservedAt: Date.now() - 8 * 60 * 1000,
    bankedResetLabel: "可用重置卡 0 张",
  },
  {
    accountId: "openai-extra-2",
    accountName: "额外账号 2",
    sourceId: "openai-codex-extra-2",
    status: "no_change",
    attribution: "unknown",
    observationId: 1000 + 2,
    temporalCorrelation: "high",
    windowId: "quota_window_30d",
    windowLabel: "30 天窗口",
    windowSeconds: 2592000,
    previous: { capturedAt: Date.now() - 30 * 60 * 60 * 1000, remaining: 0.66, resetAt: Date.now() + 20 * 24 * 60 * 60 * 1000 },
    current: { capturedAt: Date.now() - 8 * 60 * 1000, remaining: 0.64, resetAt: Date.now() + 20 * 24 * 60 * 60 * 1000 },
    lastSuccessAt: Date.now() - 8 * 60 * 1000,
    lastResetObservedAt: null,
    note: "已成功刷新，本次未观察到窗口恢复",
    bankedResetLabel: "可用重置卡 1 张",
  },
];

/** 预览雷达快照：仅为布局验收示例，不代表任何真实信号。 */
const previewRadar: RadarSnapshot = {
  sourceStatus: "fresh",
  lastSyncedAt: Date.now() - 42 * 60 * 1000,
  posts: [
    {
      id: "preview-1",
      url: "https://x.com/tibo/status/preview-1",
      text: "Rates appear to have rolled over for a fresh window in some regions.",
      postedAt: Date.now() - 3 * 60 * 60 * 1000,
      kind: "indirect",
      badge: "间接相关",
      filter: "related",
      explicitReset: false,
      isReply: false,
      replies: 4,
      reposts: 12,
      likes: 38,
      syncedAt: Date.now() - 42 * 60 * 1000,
      translatedText: "示例翻译：部分地区额度窗口似乎已经翻滚到新一轮。",
      translatedAt: Date.now() - 40 * 60 * 1000,
      translationSource: "codexradar",
      summary: "部分地区额度窗口似乎已经翻滚到新一轮。",
      analysis: "上游解读示例：这是弱信号，不能据此确认全量重置。",
      lifecycleConsumedAt: Date.now() - 40 * 60 * 1000,
    },
    {
      id: "preview-2",
      url: "https://x.com/tibo/status/preview-2",
      text: "Limits holding steady, no reset signal observed today.",
      postedAt: Date.now() - 8 * 60 * 60 * 1000,
      kind: "none",
      badge: "无重置信号",
      filter: "none",
      explicitReset: false,
      isReply: false,
      replies: 1,
      reposts: 3,
      likes: 9,
      syncedAt: Date.now() - 42 * 60 * 1000,
      translatedText: "限额保持稳定，今天没有看到重置信号。",
      translatedAt: Date.now() - 42 * 60 * 1000,
      translationSource: "codexradar",
      summary: "限额保持稳定，今天没有看到重置信号。",
      analysis: "上游解读示例：没有重置承诺。",
      lifecycleConsumedAt: null,
    },
    {
      // 模拟真实 X 帖的长数字 ID：验收「分析依据不暴露原始帖子编号」的友好化展示。
      id: "2094252447271366730",
      url: "https://x.com/tibo/status/2094252447271366730",
      text: "Earlier reset landed yesterday evening.",
      postedAt: Date.now() - 26 * 60 * 60 * 1000,
      kind: "direct",
      badge: "重置相关",
      filter: "signal",
      explicitReset: true,
      isReply: false,
      replies: 8,
      reposts: 21,
      likes: 55,
      syncedAt: Date.now() - 42 * 60 * 1000,
      translatedText: "昨天傍晚的重置已经落地。",
      translatedAt: Date.now() - 30 * 60 * 1000,
      translationSource: "codexradar",
      summary: "昨天傍晚的重置已经落地。",
      analysis: null,
      lifecycleConsumedAt: Date.now() - 26 * 60 * 60 * 1000,
    },
  ],
  latest: null,
  checks: [],
  analysis: previewAnalysis,
  models: [],
  analysisPrefs: {
    analyze: false,
    rangeKey: "3d",
    sourceId: null,
    model: null,
    userPrompt: "",
    defaultUserPrompt: "",
  },
  notice: {
    headline: "Tibo：明天可能迎来 Codex 新里程碑",
    lead: "请关注 Codex 仪表板",
    items: ["这是新的官方弱信号，但尚未直接确认新一轮重置。"],
    updatedAt: Date.now() - 42 * 60 * 1000,
    isCurrent: true,
    freshnessMs: 42 * 60 * 1000,
  },
  sourceAssessment: {
    headline: "按钮今日已按下，庆祝活动推迟至明天",
    lead: "请关注 Codex 仪表板",
    lastSyncedAt: Date.now() - 42 * 60 * 1000,
    freshness: "fresh",
  },
  event: previewEvent,
  aiAssessment: {
    enabled: true,
    state: "covered",
    eventAnalysis: previewAnalysis,
    latestDeltaAnalysis: previewAnalysis,
    history: previewAnalysis,
    latestError: null,
  },
  decision: previewDecision(),
  quotaVerifications: previewQuota,
  analysisGroups: {
    mode: "live_delta",
    newPostIds: ["preview-2"],
    eventContextIds: ["2094252447271366730"],
    historicalContextIds: ["preview-1"],
    pendingUnconsumedCount: 1,
  },
};
previewRadar.latest = previewRadar.posts[0];

/** 生命周期状态矩阵变体：覆盖状态验收矩阵的关键组合，全部为示例数据。 */
function lifecycleVariants(): Array<{ label: string; snapshot: RadarSnapshot }> {
  const withOverrides = (overrides: Partial<RadarSnapshot>): RadarSnapshot => ({
    ...previewRadar,
    ...overrides,
  });
  return [
    {
      label: "AI 开 · 当前匹配 · 非计划刷新",
      snapshot: withOverrides({}),
    },
    {
      label: "AI 关 · 有历史分析",
      snapshot: withOverrides({
        aiAssessment: {
          enabled: false,
          state: "disabled",
          eventAnalysis: previewAnalysis,
          latestDeltaAnalysis: null,
          history: previewAnalysis,
          latestError: null,
        },
      }),
    },
    {
      label: "AI 失败 · 历史保留",
      snapshot: withOverrides({
        aiAssessment: {
          enabled: true,
          state: "failed",
          eventAnalysis: previewAnalysis,
          latestDeltaAnalysis: null,
          history: previewAnalysis,
          latestError: "示例：当前时间窗内没有 Tibo 动态可分析",
        },
      }),
    },
    {
      label: "CodexRadar 缓存过期",
      snapshot: withOverrides({
        sourceStatus: "stale",
        lastSyncedAt: Date.now() - 6 * 60 * 60 * 1000,
        sourceAssessment: { ...previewRadar.sourceAssessment, freshness: "stale", lastSyncedAt: Date.now() - 6 * 60 * 60 * 1000 },
      }),
    },
    {
      label: "额度网络不可达",
      snapshot: withOverrides({
        quotaVerifications: [
          {
            accountId: "openai-local",
            accountName: "本机 Codex",
            sourceId: "openai-codex-local",
            status: "unavailable",
            attribution: "unknown",
            observationId: 1000 + 3,
            temporalCorrelation: "high",
            windowId: "quota_window_7d",
            windowLabel: "7 天窗口",
            windowSeconds: 604800,
            previous: null,
            current: null,
            lastSuccessAt: Date.now() - 25 * 60 * 60 * 1000,
            lastResetObservedAt: null,
            note: "示例：暂时无法刷新",
            bankedResetLabel: "暂无法获取",
          },
        ],
      }),
    },
    {
      label: "缺少额度基线",
      snapshot: withOverrides({
        quotaVerifications: [
          {
            accountId: "openai-local",
            accountName: "本机 Codex",
            sourceId: "openai-codex-local",
            status: "insufficient_data",
            attribution: "unknown",
            observationId: 1000 + 4,
            temporalCorrelation: "high",
            windowId: "quota_window_7d",
            windowLabel: "7 天窗口",
            windowSeconds: 604800,
            previous: null,
            current: {
              capturedAt: Date.now() - 8 * 60 * 1000,
              remaining: 0.9,
              resetAt: Date.now() + 6 * 24 * 60 * 60 * 1000,
            },
            lastSuccessAt: Date.now() - 8 * 60 * 1000,
            lastResetObservedAt: null,
            note: "缺少额度基线快照，成功刷新两次后可观察",
            bankedResetLabel: "暂无法获取",
          },
        ],
      }),
    },
    {
      label: "重置卡观察中",
      snapshot: withOverrides({
        event: {
          ...previewEvent,
          phase: "watching",
          temporalStatus: "timeless",
          title: "重置卡可能即将到账",
          eventType: "banked_reset",
          claimedLandedAt: null,
          expectedAt: null,
        },
        decision: previewDecision({
          status: "watching",
          headline: "重置卡可能即将到账",
          timeText: "到账时间尚未明确",
          timeKind: "unknown",
          eventType: "banked_reset",
          claimedAt: null,
          expectedAt: null,
          canConfirmReset: true,
          stripBadge: "观察中",
          stripPrimary: "重置卡可能即将到账",
          stripPrimaryCompact: "重置卡可能即将到账",
          stripSecondary: "AI 已分析 · 等待验证",
          recentSummaryText: "最近一次重置于 08-31 10:27 · 本机观察确认",
        }),
      }),
    },
    {
      label: "正常计划内刷新",
      snapshot: withOverrides({
        event: {
          ...previewEvent,
          phase: "upcoming",
          temporalStatus: "before_expected",
          title: "出现较强的即将重置信号",
          expectedAt: Date.now() + 14 * 60 * 60 * 1000,
        },
        decision: previewDecision({
          status: "upcoming",
          headline: "预计即将重置",
          timeText: "预计北京时间 09-02 10:00 左右",
          timeKind: "expected",
          expectedAt: Date.now() + 14 * 60 * 60 * 1000,
          canConfirmReset: true,
          stripBadge: "强信号",
          stripPrimary: "预计北京时间 09-02 10:00 左右",
          stripPrimaryCompact: "预计 09-02 10:00",
          stripSecondary: "来源明确预告 · AI 已分析 · 等待验证",
        }),
        quotaVerifications: [
          {
            accountId: "openai-local",
            accountName: "本机 Codex",
            sourceId: "openai-codex-local",
            status: "scheduled",
            attribution: "scheduled",
            observationId: 1000 + 5,
            temporalCorrelation: "high",
            windowId: "quota_window_7d",
            windowLabel: "7 天窗口",
            windowSeconds: 604800,
            previous: { capturedAt: Date.now() - 9 * 24 * 60 * 60 * 1000, remaining: 0.2, resetAt: Date.now() - 2 * 24 * 60 * 60 * 1000 },
            current: { capturedAt: Date.now() - 8 * 60 * 1000, remaining: 0.96, resetAt: Date.now() + 5 * 24 * 60 * 60 * 1000 },
            lastSuccessAt: Date.now() - 8 * 60 * 1000,
            lastResetObservedAt: null,
            note: "到达原定时间后的正常周期刷新",
            bankedResetLabel: "可用重置卡 0 张",
          },
        ],
      }),
    },
    {
      label: "多账号部分观察到",
      snapshot: withOverrides({}),
    },
    {
      label: "已观察到落地 · 头部单徽章",
      snapshot: withOverrides({
        event: {
          ...previewEvent,
          phase: "landed_observed",
          temporalStatus: "observed_landed",
          observedResetAt: Date.now() - 8 * 60 * 1000,
          expiresAt: Date.now() + 24 * 60 * 60 * 1000 - 8 * 60 * 1000,
          title: "本机已观察到额度刷新",
        },
        decision: previewDecision({
          status: "landed_observed",
          headline: "本机已观察到额度重置",
          timeText: "本机于 08-31 10:27 观察到额度重置",
          timeKind: "observed",
          observedAt: Date.now() - 8 * 60 * 1000,
          observationExpiresAt: Date.now() + 24 * 60 * 60 * 1000 - 8 * 60 * 1000,
          observationPeriodText: "24 小时观察期至 09-01 10:27",
          canConfirmReset: false,
          stripBadge: "已观察",
          stripPrimary: "本机于 08-31 10:27 观察到额度重置",
          stripPrimaryCompact: "08-31 10:27 额度重置",
          stripSecondary: "24 小时观察期至 09-01 10:27",
        }),
      }),
    },
    {
      label: "预告时间已过 · 等待验证",
      snapshot: withOverrides({
        event: { ...previewEvent, phase: "upcoming", temporalStatus: "expected_time_passed", title: "预告时间已过，等待本机验证" },
        decision: previewDecision({
          status: "expected_time_passed",
          headline: "预告时间已过，等待验证",
          timeText: "原预告时间 09-01 10:00",
          verificationHint: "等待本机检测或用户确认",
          timeKind: "passed",
          canConfirmReset: true,
          stripBadge: "等待验证",
          stripPrimary: "原预告时间 09-01 10:00",
          stripPrimaryCompact: "原预告 09-01 10:00",
          stripSecondary: "等待本机检测或用户确认",
        }),
      }),
    },
    {
      label: "无事件 · AI 未分析",
      snapshot: withOverrides({
        event: null,
        aiAssessment: {
          enabled: true,
          state: "not_analyzed",
          eventAnalysis: null,
          latestDeltaAnalysis: null,
          history: null,
          latestError: null,
        },
        decision: previewDecision({
          status: "no_signal",
          activeEventId: null,
          headline: "暂无下一轮重置信号",
          timeText: "下一次重置时间暂时无法判断",
          expectedAt: null,
          timeKind: "unknown",
          signalLevel: null,
          relevantPostIds: [],
          canConfirmReset: false,
          stripBadge: "暂无新信号",
          stripPrimary: "下一次重置时间暂时无法判断",
          stripPrimaryCompact: "下一次时间暂无法判断",
          stripSecondary: "最近重置于 08-31 10:27",
          recentSummaryText: "最近一次重置于 08-31 10:27 · 本机观察确认",
          recentReset: {
            id: "preview-event-closed",
            phase: "closed",
            title: "本机已观察到额度重置",
            closeReason: "completed",
            observedResetAt: Date.now() - 26 * 60 * 60 * 1000,
            closedAt: Date.now() - 2 * 60 * 60 * 1000,
            postIds: ["2094252447271366730"],
            analysis: previewAnalysis,
            claimedLandedAt: Date.now() - 30 * 60 * 60 * 1000,
            userConfirmedResetAt: null,
            confirmationSource: "observed",
            eventType: "quota_reset",
          },
          eventType: null,
        }),
        quotaVerifications: [],
      }),
    },
  ];
}

function OrbState({
  label,
  forceState,
  disabled,
}: {
  label: string;
  forceState?: "hover" | "focus" | "active";
  disabled?: boolean;
}) {
  return (
    <section className="hb-preview-tile">
      <span>{label}</span>
      <div className="hb-preview-orb-frame">
        <HoverbarOrb
          edge="right"
          active={false}
          ariaLabel={label}
          onActivate={noop}
          onPointerDown={noop}
          forceState={forceState}
          disabled={disabled}
        />
      </div>
    </section>
  );
}

function PreviewPanel({
  edge,
  platforms,
  status,
  initialView = "quota",
  radar = previewRadar,
}: {
  edge: HoverbarEdge;
  platforms: PlatformSummaryViewModel[];
  status: string;
  initialView?: "quota" | "radar";
  radar?: RadarSnapshot;
}) {
  const { theme, toggleTheme } = useHoverbarTheme();
  const [view, setView] = useState<"quota" | "radar">(initialView);
  const rootRef = useRef<HTMLDivElement | null>(null);
  // 自动断言：四边停靠均要求零横向溢出（scrollWidth == clientWidth），违例打 console.error
  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const assertNoHorizontalOverflow = () => {
      const targets = root.querySelectorAll<HTMLElement>(".hb-detail-root, .hb-panel, .hb-service-scroll");
      targets.forEach((node) => {
        if (node.scrollWidth !== node.clientWidth) {
          console.error(
            `[hoverbar-preview] 横向溢出 edge=${edge} <${node.className}> scrollWidth=${node.scrollWidth} clientWidth=${node.clientWidth}`,
          );
        }
      });
    };
    assertNoHorizontalOverflow();
    const timer = window.setTimeout(assertNoHorizontalOverflow, 300);
    return () => window.clearTimeout(timer);
  }, [edge, view, platforms]);
  return (
    <div className="hb-preview-detail-frame" data-edge={edge}>
      <div ref={rootRef} className="hb-detail-root" data-edge={edge} data-motion="visible">
        <section className="hb-panel">
          <header className="hb-head">
            <p className="hb-refresh-status">{status}</p>
            <div className="hb-actions">
              <button type="button" className="hb-action-button" aria-label="刷新" onClick={noop}>
                <RefreshCw size={16} aria-hidden />
              </button>
              <button
                type="button"
                className="hb-action-button"
                aria-label={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"}
                onClick={toggleTheme}
              >
                {theme === "dark" ? <SunMedium size={16} aria-hidden /> : <Moon size={16} aria-hidden />}
              </button>
              <button type="button" className="hb-action-button" aria-label="打开主窗口" onClick={noop}>
                <ExternalLink size={16} aria-hidden />
              </button>
              <button type="button" className="hb-action-button" aria-label="收起" onClick={noop}>
                <X size={17} aria-hidden />
              </button>
            </div>
          </header>
          <div className="hb-service-list">
            <div className="hb-service-scroll">
              {view === "radar" ? (
                <HoverbarRadarDetail
                  radar={radar}
                  onBack={() => setView("quota")}
                  onRefresh={noop}
                  onRetryQuota={noop}
                />
              ) : (
                platforms.map((platform) => (
                  <HoverbarPlatformCard
                    key={`${edge}-${platform.providerId}`}
                    platform={platform}
                    radar={platform.providerId === "openai" ? radar : undefined}
                    onOpenRadar={platform.providerId === "openai" ? () => setView("radar") : undefined}
                    onRefreshRadar={platform.providerId === "openai" ? noop : undefined}
                  />
                ))
              )}
            </div>
          </div>
          {/* 预览页脚：全部为示例文案，仅供四边/主题人工检查 */}
          <footer className="hb-foot">
            <span>数据仅供参考 · v0.1.0</span>
            <span>共 {platforms.length} 个平台</span>
            <span className="hb-foot-time">最后更新：11:51</span>
          </footer>
        </section>
      </div>
    </div>
  );
}

function HoverbarPreview() {
  return (
    <main className="hb-preview-page">
      <header>
        <h1>悬浮球状态预览</h1>
        <p>
          开发验收页。Credits、消费和赠送/充值在预览数据中存在，但不应出现在卡片文案里；
          雷达摘要与翻译均为示例数据。
        </p>
      </header>

      <div className="hb-preview-orb-grid">
        <OrbState label="默认" />
        <OrbState label="悬停" forceState="hover" />
        <OrbState label="键盘焦点" forceState="focus" />
        <OrbState label="按下" forceState="active" />
        <OrbState label="禁用" disabled />
      </div>

      <section className="hb-preview-detail-section">
        <h2>四边停靠 · 一行一个平台 · 窄版最终稿</h2>
        <div className="hb-preview-edges">
          <div>
            <h2>顶部 420px</h2>
            <PreviewPanel edge="top" platforms={previewPlatforms} status="更新于 11:51" />
          </div>
          <div>
            <h2>底部 420px</h2>
            <PreviewPanel edge="bottom" platforms={previewPlatforms} status="更新于 11:51" />
          </div>
          <div>
            <h2>右侧 300px</h2>
            <PreviewPanel edge="right" platforms={previewPlatforms} status="更新于 11:51" />
          </div>
          <div>
            <h2>左侧 300px</h2>
            <PreviewPanel edge="left" platforms={previewPlatforms} status="更新于 11:51" />
          </div>
        </div>
      </section>

      <section className="hb-preview-detail-section">
        <h2>GPT 重置雷达二级页 · 示例数据</h2>
        <div className="hb-preview-edges">
          <div>
            <h2>右侧停靠 · 雷达页</h2>
            <PreviewPanel edge="right" platforms={previewPlatforms} status="更新于 11:51" initialView="radar" />
          </div>
          <div>
            <h2>顶部停靠 · 雷达页</h2>
            <PreviewPanel edge="top" platforms={previewPlatforms} status="更新于 11:51" initialView="radar" />
          </div>
        </div>
      </section>

      <section className="hb-preview-detail-section">
        <h2>重置事件生命周期 · 状态矩阵（右侧 300px，示例数据）</h2>
        <div className="hb-preview-edges">
          {lifecycleVariants().map(({ label, snapshot }) => (
            <div key={label}>
              <h2>{label}</h2>
              <PreviewPanel edge="right" platforms={[]} status="更新于 11:51" initialView="radar" radar={snapshot} />
            </div>
          ))}
        </div>
      </section>

      <div className="hb-preview-panel-grid">
        <section>
          <h2>DeepSeek · 全 fresh（资金 + 模型行 + 靶心缓存）</h2>
          <HoverbarPlatformCard platform={deepseekHealthy} />
        </section>
        <section>
          <h2>DeepSeek · 余额实时 + 网页用量缓存</h2>
          <HoverbarPlatformCard platform={deepseekWebStale} />
        </section>
        <section>
          <h2>DeepSeek · 余额未获取 + 网页用量实时</h2>
          <HoverbarPlatformCard platform={deepseekBalanceMissing} />
        </section>
        <section>
          <h2>DeepSeek · 全部缺失</h2>
          <HoverbarPlatformCard platform={deepseekAllMissing} />
        </section>
        <section>
          <h2>部分可用 · 缓存可能过期</h2>
          <HoverbarPlatformCard platform={staleGlm} />
        </section>
        <section>
          <h2>异常</h2>
          <HoverbarPlatformCard platform={kimiError} />
        </section>
        <section>
          <h2>GPT Plus + Free · 含雷达摘要</h2>
          <HoverbarPlatformCard
            platform={gptPlatform}
            radar={previewRadar}
            onOpenRadar={() => undefined}
            onRefreshRadar={noop}
          />
        </section>
      </div>
    </main>
  );
}

const previewRoot =
  window.__HOVERBAR_PREVIEW_ROOT__ ??
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
window.__HOVERBAR_PREVIEW_ROOT__ = previewRoot;

previewRoot.render(
  <React.StrictMode>
    <QueryClientProvider client={new QueryClient()}>
      <HoverbarPreview />
    </QueryClientProvider>
  </React.StrictMode>,
);
