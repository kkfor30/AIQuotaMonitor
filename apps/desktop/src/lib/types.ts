/**
 * 前后端共享 ViewModel 契约（与 src-tauri/src/domain/view_models.rs 的 serde 输出对齐）。
 * 阶段一为静态数据；金额均为后端已格式化字符串，前端不做数值计算。
 */

export type PlatformAggregateStatus = "healthy" | "partial" | "setup_required" | "error";

export type DataFreshness = "fresh" | "stale" | "missing";

export type SourceState = "ready" | "refreshing" | "auth_required" | "error";

export type SourceType = "api_key" | "web_session" | "local_cli" | "oauth";

export type SourceAccessMode =
  | "coding_plan"
  | "token_plan"
  | "personal_balance"
  | "web_usage"
  | "local_cli";

export type AccountKind = "local" | "default" | "additional";

export type CredentialInput = {
  label: string;
  placeholder: string;
  helpText: string;
  secretKind: "api_key" | "bearer_token" | "cookie";
};

export type RefreshHistoryEntry = {
  id: string;
  sourceId: string;
  sourceName: string;
  accountId: string;
  accountName: string;
  status: "running" | "success" | "partial" | "failed";
  /** epoch 毫秒；进行中的记录尚未结束。 */
  finishedAt: number | null;
  errorMessage: string | null;
};

export type LegacyConfigInspection = {
  available: boolean;
  path: string | null;
};

export type LegacyImportResult = {
  platforms: PlatformSummaryViewModel[];
  importedSourceIds: string[];
  archivedPath: string | null;
};

export interface SourceSummaryViewModel {
  sourceId: string;
  adapterId: string;
  accountId: string;
  accountName: string;
  accountKind: AccountKind;
  sourceType: SourceType;
  displayName: string;
  state: SourceState;
  credentialConfigured: boolean;
  /** epoch 毫秒 */
  lastValidatedAt: number | null;
  /** epoch 毫秒 */
  lastSuccessAt: number | null;
  errorCode: string | null;
  errorMessage: string | null;
  capabilityIds: string[];
  /** 阶段一预览对象可能缺失；真实后端始终返回。 */
  credentialInput?: CredentialInput | null;
  supportsInteractiveLogin?: boolean;
  supportsCliLogin?: boolean;
  accessMode?: SourceAccessMode;
}

export interface CapabilityDisplayValue {
  /** money | tokens | percent | trend */
  kind: string;
  primary: string | null;
  secondary: string | null;
  progress: number | null;
}

export interface TrendPoint {
  label: string;
  value: number;
}

export interface CapabilitySnapshotViewModel {
  capabilityId: string;
  sourceId: string;
  accountId: string;
  displayName: string;
  freshness: DataFreshness;
  /** epoch 毫秒 */
  capturedAt: number | null;
  /** epoch 毫秒 */
  lastGoodAt: number | null;
  value: CapabilityDisplayValue;
  trend: TrendPoint[];
}

export interface PlatformCatalogItem {
  id: string;
  displayName: string;
  officialUrl: string;
  apiBaseUrl: string | null;
  apiKeyUrl: string | null;
  accessHint: string;
  needsApiKey: boolean;
  needsWebLogin: boolean;
  needsLocalCli: boolean;
  added: boolean;
  supportsMultipleAccounts: boolean;
}

export interface AccountSummaryViewModel {
  accountId: string;
  displayName: string;
  kind: AccountKind;
  status: PlatformAggregateStatus;
  sourceIds: string[];
  canRename: boolean;
  canRemove: boolean;
}

export interface PlatformSetupViewModel {
  platformId: string;
  displayName: string;
  notes: string;
  officialUrl: string;
  apiKeyUrl: string | null;
  apiBaseUrl: string;
  officialApiBaseUrl: string;
  apiEndpointHint: string;
  apiKeySourceId: string | null;
  apiKeyConfigured: boolean;
  localCliSourceId: string | null;
  needsApiKey: boolean;
  needsWebLogin: boolean;
  needsLocalCli: boolean;
}

export interface PlatformSummaryViewModel {
  providerId: string;
  displayName: string;
  aggregateStatus: PlatformAggregateStatus;
  accessSummary: string;
  supportsMultipleAccounts: boolean;
  accounts: AccountSummaryViewModel[];
  /** 阶段一预览对象可能缺失；真实后端始终返回。 */
  officialUrl?: string | null;
  apiBaseUrl?: string | null;
  sources: SourceSummaryViewModel[];
  capabilities: CapabilitySnapshotViewModel[];
  refreshHistory?: RefreshHistoryEntry[];
}

/** 悬浮球偏好（与 src-tauri/src/storage/mod.rs 对齐） */
export interface HoverbarAnchorDto {
  edge: "top" | "right" | "bottom" | "left" | string;
  ratio: number;
}

export interface HoverbarPreferencesDto {
  enabled: boolean;
  anchor: HoverbarAnchorDto;
  detailSize: { width: number; height: number };
}

/** 聚合状态展示元数据：文字 + 颜色 token + 图标名，状态绝不只靠颜色。 */
export const AGGREGATE_STATUS_META: Record<
  PlatformAggregateStatus,
  { label: string; icon: "circle-check" | "circle-alert" | "settings" | "circle-x"; tone: "success" | "warning" | "neutral" | "danger" }
> = {
  healthy: { label: "正常", icon: "circle-check", tone: "success" },
  partial: { label: "部分可用", icon: "circle-alert", tone: "warning" },
  setup_required: { label: "需配置", icon: "settings", tone: "neutral" },
  error: { label: "异常", icon: "circle-x", tone: "danger" },
};

/** 数据新鲜度展示元数据 */
export const FRESHNESS_META: Record<
  DataFreshness,
  { label: string; tone: "primary" | "warning" | "neutral" }
> = {
  fresh: { label: "实时", tone: "primary" },
  stale: { label: "缓存数据", tone: "warning" },
  missing: { label: "暂无数据", tone: "neutral" },
};

export const SOURCE_STATE_META: Record<
  SourceState,
  { label: string; tone: "success" | "warning" | "neutral" | "danger" | "primary" }
> = {
  ready: { label: "运行中", tone: "success" },
  refreshing: { label: "刷新中", tone: "primary" },
  auth_required: { label: "待配置", tone: "neutral" },
  error: { label: "异常", tone: "danger" },
};

export const SOURCE_TYPE_LABEL: Record<SourceType, string> = {
  api_key: "API Key",
  web_session: "网页会话",
  local_cli: "本地 CLI",
  oauth: "OAuth 订阅",
};

export const ACCESS_MODE_LABEL: Record<SourceAccessMode, string> = {
  coding_plan: "Coding Plan",
  token_plan: "Token Plan",
  personal_balance: "个人余额",
  web_usage: "网页用量",
  local_cli: "本机 CLI",
};
