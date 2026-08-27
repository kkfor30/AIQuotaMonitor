import { LayoutDashboard, Radar, Settings, Boxes } from "lucide-react";
import type { LucideIcon } from "lucide-react";

export type NavId = "overview" | "platform-center" | "gpt-radar" | "settings";

export interface NavItem {
  id: NavId;
  label: string;
  icon: LucideIcon;
  description: string;
}

/** 一级导航固定四项（设计稿 platform-center-v4 / product-shell-v5）。 */
export const NAV_ITEMS: NavItem[] = [
  { id: "overview", label: "总览", icon: LayoutDashboard, description: "全部平台健康摘要" },
  { id: "platform-center", label: "平台中心", icon: Boxes, description: "额度与接入管理" },
  { id: "gpt-radar", label: "GPT 重置雷达", icon: Radar, description: "重置信号与动态" },
  { id: "settings", label: "设置", icon: Settings, description: "外观与悬浮球偏好" },
];

/** 平台中心定位目标：总览关注项点击后携带，避免用户二次寻找（v5 交互 2）。 */
export interface PlatformCenterTarget {
  providerId: string;
  tab: "usage" | "sources";
  /** 需要定位的 Source（仅接入与来源 Tab 使用） */
  focusSourceId?: string;
}
