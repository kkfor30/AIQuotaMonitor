/**
 * Tauri IPC 封装：所有后端调用集中于此，组件不得直接 invoke。
 * 事件名与 src-tauri/src/commands/window_commands.rs 保持一致。
 */
import { invoke } from "@tauri-apps/api/core";
import type { HoverbarPreferencesDto, PlatformSummaryViewModel } from "./types";

export async function fetchPlatformSummaries(): Promise<PlatformSummaryViewModel[]> {
  return invoke<PlatformSummaryViewModel[]>("get_platform_summaries");
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
