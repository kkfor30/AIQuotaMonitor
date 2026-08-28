/**
 * 悬浮球平台图标映射。
 *
 * 迁移来源：DeepSeek-Monitor-Windows/DeepSeekMonitorWindows
 * src/provider-visuals.ts（提交 af6cfe07，MIT）。
 * 变更：旧 provider id 映射到当前平台模板 id，仅供悬浮球紧凑视图使用。
 */
export type HoverbarProviderVisual = {
  src: string;
  scale: number;
};

export const HOVERBAR_PROVIDER_VISUALS: Record<string, HoverbarProviderVisual> = {
  deepseek: { src: "/assets/providers/deepseek.png", scale: 0.78 },
  openai: { src: "/assets/providers/codex.svg", scale: 0.82 },
  claude_code: { src: "/assets/providers/claude-code.svg", scale: 0.82 },
  glm: { src: "/assets/providers/glm.png", scale: 0.74 },
  kimi: { src: "/assets/providers/kimi.png", scale: 0.72 },
  mimo: { src: "/assets/providers/mimo.png", scale: 0.76 },
  minimax: { src: "/assets/providers/minimax.png", scale: 0.78 },
};
