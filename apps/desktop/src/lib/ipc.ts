/**
 * Tauri IPC 封装：所有后端调用集中于此，组件不得直接 invoke。
 * 事件名与 src-tauri/src/commands/window_commands.rs 保持一致。
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  HoverbarPreferencesDto,
  LegacyConfigInspection,
  LegacyImportResult,
  PlatformSummaryViewModel,
} from "./types";

export async function fetchPlatformSummaries(): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("get_platform_summaries");
}

export async function refreshPlatform(providerId: string): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("refresh_platform", { providerId });
}

export async function validateSourceCredential(
  sourceId: string,
  secret: string,
): Promise<string> {
  return invoke<string>("validate_source_credential", { sourceId, secret });
}

export async function saveSourceCredential(
  sourceId: string,
  secret: string,
): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("save_source_credential", { sourceId, secret });
}

export async function clearSourceCredential(sourceId: string): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("clear_source_credential", { sourceId });
}

export async function startSourceLogin(sourceId: string): Promise<void> {
  return invoke<void>("start_source_login", { sourceId });
}

export async function closeSourceLogin(sourceId: string): Promise<void> {
  return invoke<void>("close_source_login", { sourceId });
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

/** 悬浮详情窗口事件名（迁移自旧项目，保持不变） */
export const HOVERBAR_EVENTS = {
  detailOpen: "hoverbar-detail-open",
  detailClose: "hoverbar-detail-close",
  detailVisibility: "hoverbar-detail-visibility",
  detailPointer: "hoverbar-detail-pointer",
} as const;
