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
  quotaVerifications: QuotaVerification[];
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
  errorMessage: string | null;
  coversLatest: boolean;
  eventId: string | null;
  analysisMode: string | null;
  eventRelation: string | null;
  eventPhase: string | null;
  deltaEffect: string | null;
  signalLevel: string | null;
  contextStatus: string | null;
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
  timeline: RadarEventNode[];
  postIds: string[];
};

export type RadarAiAssessment = {
  enabled: boolean;
  /** current | disabled | pending | failed | not_analyzed */
  state: string;
  current: RadarAnalysis | null;
  history: RadarAnalysis | null;
  latestError: string | null;
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
  windowId: string | null;
  windowLabel: string | null;
  windowSeconds: number | null;
  previous: QuotaWindowPoint | null;
  current: QuotaWindowPoint | null;
  lastSuccessAt: number | null;
  note: string | null;
  lastResetObservedAt: number | null;
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
  accountId: string;
  sourceId: string;
  capturedAt: number;
}): Promise<RadarSnapshot> {
  return invoke<RadarSnapshot>("confirm_radar_quota_change", {
    accountId: input.accountId,
    sourceId: input.sourceId,
    capturedAt: input.capturedAt,
  });
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
