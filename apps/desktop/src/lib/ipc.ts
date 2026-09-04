/**
 * Tauri IPC 封装：所有后端调用集中于此，组件不得直接 invoke。
 * 事件名与 src-tauri/src/commands/window_commands.rs 保持一致。
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  HoverbarPreferencesDto,
  LegacyConfigInspection,
  LegacyImportResult,
  PlatformCatalogItem,
  PlatformSetupViewModel,
  PlatformSummaryViewModel,
} from "./types";

export async function fetchPlatformSummaries(): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("get_platform_summaries");
}

export async function fetchPlatformCatalog(): Promise<PlatformCatalogItem[]> {
  return invoke<PlatformCatalogItem[]>("list_platform_catalog");
}

export async function fetchPlatformSetup(platformId: string): Promise<PlatformSetupViewModel> {
  return invoke<PlatformSetupViewModel>("get_platform_setup", { platformId });
}

export async function addUserPlatforms(platformIds: string[]): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("add_user_platforms", { platformIds });
}

export type AddPlatformAccountResult = {
  platforms: PlatformSummaryViewModel[];
  accountId: string;
  sourceIds: string[];
};

export async function addPlatformAccount(platformId: string): Promise<AddPlatformAccountResult> {
  return invoke<AddPlatformAccountResult>("add_platform_account", { platformId });
}

export async function renamePlatformAccount(
  accountId: string,
  displayName: string,
): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("rename_platform_account", { accountId, displayName });
}

export async function removePlatformAccount(accountId: string): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("remove_platform_account", { accountId });
}

export async function removeUserPlatform(platformId: string): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("remove_user_platform", { platformId });
}

export async function revealSourceSecret(sourceId: string): Promise<string> {
  return invoke<string>("reveal_source_secret", { sourceId });
}

export async function openExternalUrl(url: string): Promise<void> {
  return invoke<void>("open_external_url", { url });
}

export type EndpointLatencyView = {
  url: string;
  latencyMs: number | null;
  error: string | null;
};

export async function testApiEndpoints(urls: string[]): Promise<EndpointLatencyView[]> {
  return invoke<EndpointLatencyView[]>("test_api_endpoints", { urls });
}

export async function refreshPlatform(providerId: string): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("refresh_platform", { providerId });
}

export async function refreshAllPlatforms(): Promise<void> {
  return invoke<void>("refresh_all_platforms");
}

export async function validateSourceCredential(
  sourceId: string,
  secret: string,
  apiBaseUrl?: string,
): Promise<string> {
  return invoke<string>("validate_source_credential", { sourceId, secret, apiBaseUrl });
}

export async function savePlatformSetup(input: {
  platformId: string;
  displayName: string;
  notes: string;
  apiBaseUrl: string;
  sourceId?: string | null;
  secret: string;
}): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("save_platform_setup", { input });
}

export async function saveSourceCredential(
  sourceId: string,
  secret: string,
  apiBaseUrl?: string,
): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("save_source_credential", { sourceId, secret, apiBaseUrl });
}

export async function clearSourceCredential(sourceId: string): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("clear_source_credential", { sourceId });
}

export async function startSourceLogin(sourceId: string): Promise<void> {
  return invoke<void>("start_source_login", { sourceId });
}

/** Claude 浏览器授权完成后把页面展示的授权码转发给进行中的登录进程 */
export async function submitSourceLoginCode(sourceId: string, code: string): Promise<void> {
  return invoke<void>("submit_source_login_code", { sourceId, code });
}

export async function closeSourceLogin(sourceId: string): Promise<void> {
  return invoke<void>("close_source_login", { sourceId });
}

export async function takeCapturedSourceSecret(sourceId: string): Promise<string> {
  return invoke<string>("take_captured_source_secret", { sourceId });
}

export function ipcErrorMessage(cause: unknown, fallback: string): string {
  if (typeof cause === "string" && cause.trim()) return cause.trim();
  if (cause instanceof Error && cause.message.trim()) return cause.message.trim();
  if (cause && typeof cause === "object") {
    const record = cause as { message?: unknown; error?: unknown };
    if (typeof record.message === "string" && record.message.trim()) return record.message.trim();
    if (typeof record.error === "string" && record.error.trim()) return record.error.trim();
  }
  return fallback;
}

export async function inspectLegacyConfig(): Promise<LegacyConfigInspection> {
  return invoke<LegacyConfigInspection>("inspect_legacy_config");
}

export async function importLegacyConfig(archiveOldFile: boolean): Promise<LegacyImportResult> {
  return invoke<LegacyImportResult>("import_legacy_config", { archiveOldFile });
}

export async function fetchHoverbarPreferences(): Promise<HoverbarPreferencesDto> {
  return invoke<HoverbarPreferencesDto>("get_hoverbar_preferences");
}

export async function setHoverbarEnabled(enabled: boolean): Promise<void> {
  return invoke<void>("set_hoverbar_enabled", { enabled });
}

export async function openMainWindow(): Promise<void> {
  return invoke<void>("open_main_window");
}

export type RadarSnapshot = {
  sourceStatus: string;
  lastSyncedAt: number | null;
  posts: RadarPost[];
  latest: RadarPost | null;
  checks: RadarCheck[];
  analysis: RadarAnalysis | null;
  models: RadarModelOption[];
  analysisPrefs: RadarAnalysisPrefs;
  notice: RadarNotice | null;
  sourceAssessment: RadarSourceAssessment;
  event: RadarEvent | null;
  aiAssessment: RadarAiAssessment;
  decision: RadarDecision;
  quotaVerifications: QuotaVerification[];
  analysisGroups: RadarAnalysisGroups;
};

export type RadarAnalysisPrefs = {
  analyze: boolean;
  rangeKey: string;
  sourceId: string | null;
  model: string | null;
  userPrompt: string;
  defaultUserPrompt: string;
};

export type RadarNotice = {
  headline: string;
  lead: string | null;
  items: string[];
  /** 最后一次解析到公告的时间。 */
  updatedAt: number | null;
  /** 当前 CodexRadar 页面是否仍出现该公告；false 时展示“最近公告”。 */
  isCurrent: boolean;
  /** 距最后一次出现的毫秒数。 */
  freshnessMs: number | null;
};

export type RadarPost = {
  id: string;
  url: string;
  text: string;
  postedAt: number;
  kind: string;
  badge: string;
  filter: string;
  explicitReset: boolean;
  isReply: boolean;
  replies: number;
  reposts: number;
  likes: number;
  syncedAt: number;
  translatedText: string | null;
  translatedAt: number | null;
  translationSource: string | null;
  summary: string | null;
  analysis: string | null;
  lifecycleConsumedAt: number | null;
};

export type RadarCheck = {
  id: string;
  startedAt: number;
  finishedAt: number | null;
  status: string;
  syncStatus: string | null;
  parseStatus: string | null;
  analyzeStatus: string | null;
  errorMessage: string | null;
  postCount: number;
};

export type RadarAnalysis = {
  id: string;
  createdAt: number;
  rangeKey: string;
  cutPostId: string | null;
  cutLabel: string | null;
  sourceId: string | null;
  model: string | null;
  conclusion: string | null;
  analysisBasis: string | null;
  confidence: string | null;
  citations: string[];
  support: string[];
  against: string[];
  uncertainty: string[];
  /** 本次输入 NEW POSTS；“当前判断依据”引用只能来自 citations ∩ newPostIds。 */
  newPostIds: string[];
  /** 本次输入 EVENT CONTEXT POSTS。 */
  eventContextPostIds: string[];
  /** 本次输入 HISTORICAL CONTEXT POSTS；只允许出现在历史区。 */
  historicalPostIds: string[];
  errorMessage: string | null;
  coversLatest: boolean;
  eventId: string | null;
  analysisMode: string | null;
  eventRelation: string | null;
  eventPhase: string | null;
  deltaEffect: string | null;
  signalLevel: string | null;
  contextStatus: string | null;
  /** banked_reset | quota_reset | none | unknown */
  signalType: string | null;
  /** before_expected | expected_time_passed | claimed_landed | observed_landed | historical | timeless */
  temporalPhase: string | null;
  validUntil: number | null;
  timeClaims: RadarTimeClaim[];
};

/** 帖内时间声明的确定性解析结果（time-v1）。 */
export type RadarTimeClaim = {
  postId: string;
  rawText: string;
  /** resolved | ambiguous */
  parseStatus: string;
  timezoneKind: string | null;
  timezoneAssumed: boolean;
  resolvedAt: number | null;
  /** exact | assumed | ambiguous */
  precision: string;
};

export type RadarSourceAssessment = {
  headline: string | null;
  lead: string | null;
  lastSyncedAt: number | null;
  freshness: string;
};

export type RadarEventNode = {
  at: number;
  kind: string;
  label: string;
};

export type RadarEvent = {
  id: string;
  phase: string;
  title: string;
  summary: string | null;
  firstSignalAt: number;
  latestEvidenceAt: number;
  claimedLandedAt: number | null;
  observedResetAt: number | null;
  closedAt: number | null;
  expectedAt: number | null;
  expiresAt: number | null;
  stateRevision: number;
  temporalStatus: string;
  timeline: RadarEventNode[];
  postIds: string[];
  userConfirmedResetAt: number | null;
  /** banked_reset | quota_reset */
  eventType: string;
};

export type RadarAiAssessment = {
  enabled: boolean;
  /** disabled | pending | failed | covered | historical */
  state: string;
  /** 当前（或最近）事件的最新成功分析：本轮事件为什么成立。 */
  eventAnalysis: RadarAnalysis | null;
  /** 最近一次成功增量分析（含无关帖）：最新帖子是否改变当前判断。 */
  latestDeltaAnalysis: RadarAnalysis | null;
  history: RadarAnalysis | null;
  latestError: string | null;
};

/** 「最近一次事件」折叠区：仅无活动事件时展示。 */
export type RadarRecentEvent = {
  id: string;
  phase: string;
  title: string;
  closeReason: string | null;
  observedResetAt: number | null;
  closedAt: number | null;
  postIds: string[];
  analysis: RadarAnalysis | null;
  claimedLandedAt: number | null;
  userConfirmedResetAt: number | null;
  /** observed | user_confirmed | claimed */
  confirmationSource: string | null;
  /** banked_reset | quota_reset */
  eventType: string;
};

/** Rust 推导的综合判断；React 只消费不二次判断。 */
export type RadarDecision = {
  /** no_signal | watching | upcoming | expected_time_passed | landed_claimed | landed_observed | user_confirmed */
  status: string;
  activeEventId: string | null;
  headline: string;
  timeText: string;
  verificationHint: string | null;
  observationPeriodText: string | null;
  expectedAt: number | null;
  observedAt: number | null;
  claimedAt: number | null;
  userConfirmedAt: number | null;
  observationExpiresAt: number | null;
  /** unknown | expected | passed | claimed | observed | confirmed */
  timeKind: string;
  signalLevel: string | null;
  /** 最近一次本机观察/用户确认的重置；“最近一次重置”唯一来源。 */
  recentReset: RadarRecentEvent | null;
  /** 最近关闭的普通雷达事件（invalid_historical_replay 等），只用于历史与来源声称提示。 */
  recentClosedEvent: RadarRecentEvent | null;
  relevantPostIds: string[];
  /** 当前判断依据引用：主分析 citations ∩ newPostIds。 */
  currentKeyCitationIds: string[];
  /** 历史上下文引用（citations ∩ historicalPostIds），不进入当前依据。 */
  historicalCitationIds: string[];
  latestIrrelevantUpdateAt: number | null;
  canConfirmReset: boolean;
  canUndoConfirm: boolean;
  pendingUnconsumedCount: number;
  sourceHasDirectSignal: boolean;
  stripBadge: string;
  stripPrimary: string;
  stripPrimaryCompact: string;
  stripSecondary: string;
  recentSummaryText: string | null;
  deltaImpactText: string;
  /** banked_reset | quota_reset；无当前事件时为 null。 */
  eventType: string | null;
};

export type RadarAnalysisGroups = {
  mode: string;
  newPostIds: string[];
  eventContextIds: string[];
  historicalContextIds: string[];
  pendingUnconsumedCount: number;
};

export type QuotaWindowPoint = {
  capturedAt: number;
  remaining: number | null;
  resetAt: number | null;
};

export type QuotaVerification = {
  accountId: string;
  accountName: string;
  sourceId: string;
  /** unavailable | insufficient_data | pending | scheduled | possible_reset | unscheduled_reset | no_change */
  status: string;
  /** unknown | scheduled | user_confirmed | radar_correlated */
  attribution: string;
  observationId: number | null;
  temporalCorrelation: string;
  windowId: string | null;
  windowLabel: string | null;
  windowSeconds: number | null;
  previous: QuotaWindowPoint | null;
  current: QuotaWindowPoint | null;
  lastSuccessAt: number | null;
  note: string | null;
  lastResetObservedAt: number | null;
  /** 可用重置卡 0 张 / 可用重置卡 1 张 / 暂无法获取 */
  bankedResetLabel: string;
};

export type RadarModelOption = {
  sourceId: string;
  platformId: string;
  displayName: string;
  model: string;
  ready: boolean;
  custom: boolean;
};

export async function fetchRadarSnapshot(): Promise<RadarSnapshot> {
  return invoke<RadarSnapshot>("get_radar_snapshot");
}

/** 主窗口手动确认重置卡：只追加归因 user_confirmed，不篡改快照。 */
/** 终止进行中的雷达检查：后端在下一个网络等待点打断，不落检查与分析记录。 */
export async function cancelRadarCheck(): Promise<void> {
  return invoke<void>("cancel_radar_check");
}

export async function confirmRadarQuotaChange(input: {
  observationId: number;
  confirmedAt: number;
}): Promise<RadarSnapshot> {
  return invoke<RadarSnapshot>("confirm_radar_quota_change", {
    observationId: input.observationId,
    confirmedAt: input.confirmedAt,
  });
}

export async function confirmRadarUserReset(): Promise<RadarSnapshot> {
  return invoke<RadarSnapshot>("confirm_radar_user_reset");
}

export async function undoRadarUserReset(): Promise<RadarSnapshot> {
  return invoke<RadarSnapshot>("undo_radar_user_reset");
}

export async function runRadarCheck(input: {
  analyze: boolean;
  rangeKey?: string;
  sourceId?: string | null;
  model?: string | null;
  userPrompt?: string | null;
}): Promise<RadarSnapshot> {
  return invoke<RadarSnapshot>("run_radar_check", input);
}

export async function translateRadarPost(postId: string): Promise<RadarSnapshot> {
  return invoke<RadarSnapshot>("translate_radar_post", { postId });
}

export async function saveRadarAnalysisPrefs(input: {
  analyze: boolean;
  rangeKey: string;
  sourceId?: string | null;
  model?: string | null;
  userPrompt?: string | null;
}): Promise<RadarSnapshot> {
  return invoke<RadarSnapshot>("save_radar_analysis_prefs", input);
}

export async function testRadarModel(input: { sourceId: string; model: string }): Promise<void> {
  return invoke<void>("test_radar_model", input);
}

export async function addRadarCustomModel(input: { sourceId: string; model: string }): Promise<RadarSnapshot> {
  return invoke<RadarSnapshot>("add_radar_custom_model", input);
}

export async function deleteRadarCustomModel(input: { sourceId: string; model: string }): Promise<RadarSnapshot> {
  return invoke<RadarSnapshot>("delete_radar_custom_model", input);
}

export type AppSettingsView = {
  theme: string;
  autostart: boolean;
  refreshIntervalMinutes: number;
  hoverbarSortMode: "manual" | "smart" | string;
  hoverbarAutoRadarCheck: boolean;
};

export async function fetchAppSettings(): Promise<AppSettingsView> {
  return invoke<AppSettingsView>("get_app_settings");
}

export async function setAppTheme(theme: string): Promise<AppSettingsView> {
  return invoke<AppSettingsView>("set_app_theme", { theme });
}

export async function setRefreshInterval(minutes: number): Promise<AppSettingsView> {
  return invoke<AppSettingsView>("set_refresh_interval", { minutes });
}

export async function setAutostart(enabled: boolean): Promise<AppSettingsView> {
  return invoke<AppSettingsView>("set_autostart", { enabled });
}

export async function reorderPlatforms(platformIds: string[]): Promise<void> {
  return invoke<void>("reorder_platforms", { platformIds });
}

export async function setHoverbarSortMode(mode: "manual" | "smart"): Promise<AppSettingsView> {
  return invoke<AppSettingsView>("set_hoverbar_sort_mode", { mode });
}

export async function setHoverbarAutoRadarCheck(enabled: boolean): Promise<AppSettingsView> {
  return invoke<AppSettingsView>("set_hoverbar_auto_radar_check", { enabled });
}

export async function clearLocalCache(): Promise<void> {
  return invoke<void>("clear_local_cache");
}

/** 悬浮详情窗口事件名（迁移自旧项目，保持不变） */
export const HOVERBAR_EVENTS = {
  detailOpen: "hoverbar-detail-open",
  detailClose: "hoverbar-detail-close",
  detailVisibility: "hoverbar-detail-visibility",
  detailPointer: "hoverbar-detail-pointer",
} as const;
