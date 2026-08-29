/**
 * 悬浮球平台图标映射：Logo 路径统一读 lib/provider-brand.ts 的官方资产映射，
 * 主窗口 PlatformMark 与悬浮卡图标共用同一份来源。
 *
 * 迁移来源：DeepSeek-Monitor-Windows/DeepSeekMonitorWindows
 * src/provider-visuals.ts（提交 af6cfe07，MIT）。
 * 变更：不再维护独立 src 表，只保留悬浮卡内的轻微缩放对齐（不改变官方图形）。
 */
import { providerBrand } from "@/lib/provider-brand";

export type HoverbarProviderVisual = {
  src: string;
  scale: number;
};

/** 各平台 Logo 在悬浮卡内的缩放微调，未列出的平台按 0.8 展示。 */
const HOVERBAR_LOGO_SCALE: Record<string, number> = {
  deepseek: 0.78,
  openai: 0.82,
  claude_code: 0.82,
  glm: 0.74,
  glm_intl: 0.74,
  kimi: 0.72,
  mimo: 0.76,
  minimax: 0.78,
  minimax_intl: 0.78,
  siliconflow: 0.8,
  siliconflow_intl: 0.8,
  stepfun: 0.82,
  openrouter: 0.8,
  novita: 0.85,
};

export function hoverbarProviderVisual(providerId: string): HoverbarProviderVisual | null {
  const logo = providerBrand(providerId).logo;
  if (!logo) return null;
  return { src: logo, scale: HOVERBAR_LOGO_SCALE[providerId] ?? 0.8 };
}
