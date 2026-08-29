/**
 * 平台品牌资产：官方 Logo 与稳定展示色。
 * Logo 只使用 apps/desktop/public/assets/providers/ 内的官方资产，
 * 禁止字母占位或自行绘制品牌图形；未知平台回退中性图标。
 * 同一映射供总览、平台目录、详情、设置排序和趋势图例共用。
 */

export type ProviderBrand = {
  /** 官方 Logo 资产路径 */
  logo: string;
  /** 图表与图例使用的稳定品牌色 */
  color: string;
};

const BRAND_BY_ID: Record<string, ProviderBrand> = {
  deepseek: { logo: "/assets/providers/deepseek.png", color: "#4d6bfe" },
  openai: { logo: "/assets/providers/codex.svg", color: "#10a37f" },
  claude_code: { logo: "/assets/providers/claude-code.svg", color: "#c96442" },
  glm: { logo: "/assets/providers/glm.png", color: "#2450e6" },
  glm_intl: { logo: "/assets/providers/glm.png", color: "#3f6cf6" },
  kimi: { logo: "/assets/providers/kimi.png", color: "#f04e33" },
  mimo: { logo: "/assets/providers/mimo.png", color: "#8b5cf6" },
  minimax: { logo: "/assets/providers/minimax.png", color: "#2f7bf0" },
  minimax_intl: { logo: "/assets/providers/minimax.png", color: "#22a0e8" },
  siliconflow: { logo: "/assets/providers/siliconflow.png", color: "#6e29f5" },
  siliconflow_intl: { logo: "/assets/providers/siliconflow.png", color: "#8a5cf6" },
  stepfun: { logo: "/assets/providers/stepfun.png", color: "#1c1c1e" },
  openrouter: { logo: "/assets/providers/openrouter.png", color: "#7624f4" },
  novita: { logo: "/assets/providers/novita.svg", color: "#020145" },
  grok: { logo: "/assets/providers/grok.svg", color: "#050505" },
};

/** 未提供官方资产的平台使用的中性回退色序列（按 id 稳定取值）。 */
const FALLBACK_COLORS = ["#64748b", "#7c8aa0", "#5d7a95", "#8896ab", "#708090"];

export function providerBrand(providerId: string): ProviderBrand {
  const known = BRAND_BY_ID[providerId];
  if (known) return known;
  let hash = 0;
  for (let index = 0; index < providerId.length; index += 1) {
    hash = (hash * 31 + providerId.charCodeAt(index)) >>> 0;
  }
  return { logo: "", color: FALLBACK_COLORS[hash % FALLBACK_COLORS.length] ?? "#64748b" };
}
