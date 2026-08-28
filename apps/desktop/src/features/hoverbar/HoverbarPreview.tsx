/**
 * 悬浮球组件开发预览：只在 Vite 开发环境手工验收，不参与 Tauri 产品入口。
 * 示例值均明确标注为预览数据，避免与真实平台快照混淆。
 */
import React from "react";
import ReactDOM from "react-dom/client";
import { ExternalLink, Moon, RefreshCw, SunMedium, X } from "lucide-react";
import type { PlatformSummaryViewModel } from "@/lib/types";
import "@/styles/global.css";
import { HoverbarOrb } from "./HoverbarAnchorApp";
import { HoverbarPlatformCard } from "./HoverbarPlatformCard";
import { useHoverbarTheme } from "./hoverbar-theme";

const noop = () => undefined;

declare global {
  interface Window {
    __HOVERBAR_PREVIEW_ROOT__?: ReturnType<typeof ReactDOM.createRoot>;
  }
}

const healthyPlatform: PlatformSummaryViewModel = {
  providerId: "deepseek",
  displayName: "DeepSeek",
  aggregateStatus: "healthy",
  accessSummary: "API Key + 网页会话",
  sources: [
    {
      sourceId: "preview-balance",
      sourceType: "api_key",
      displayName: "余额来源",
      state: "ready",
      credentialConfigured: true,
      lastValidatedAt: null,
      lastSuccessAt: null,
      errorCode: null,
      errorMessage: null,
      capabilityIds: ["balance"],
    },
  ],
  capabilities: [
    {
      capabilityId: "balance",
      sourceId: "preview-balance",
      displayName: "账户余额",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "money", primary: "¥ 25.00", secondary: null, progress: null },
      trend: [],
    },
    {
      capabilityId: "today_spend",
      sourceId: "preview-balance",
      displayName: "今日消耗",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "money", primary: "¥ 1.20", secondary: null, progress: null },
      trend: [],
    },
  ],
};

const gptPlatform: PlatformSummaryViewModel = {
  providerId: "openai",
  displayName: "GPT / Codex",
  aggregateStatus: "healthy",
  accessSummary: "本机 Codex",
  sources: [
    {
      sourceId: "openai-codex-local",
      sourceType: "local_cli",
      displayName: "本机 Codex（当前 CLI）",
      state: "ready",
      credentialConfigured: true,
      lastValidatedAt: null,
      lastSuccessAt: null,
      errorCode: null,
      errorMessage: null,
      capabilityIds: ["quota_window_5h", "quota_window_7d", "credits", "plan_level"],
      accessMode: "local_cli",
    },
    {
      sourceId: "openai-codex-extra-2",
      sourceType: "local_cli",
      displayName: "额外 ChatGPT 账号 2",
      state: "ready",
      credentialConfigured: true,
      lastValidatedAt: null,
      lastSuccessAt: null,
      errorCode: null,
      errorMessage: null,
      capabilityIds: ["quota_window_5h", "quota_window_7d", "credits", "plan_level"],
      accessMode: "local_cli",
    },
    {
      sourceId: "openai-codex-extra-3",
      sourceType: "local_cli",
      displayName: "额外 ChatGPT 账号 3",
      state: "ready",
      credentialConfigured: true,
      lastValidatedAt: null,
      lastSuccessAt: null,
      errorCode: null,
      errorMessage: null,
      capabilityIds: ["quota_window_5h", "quota_window_7d", "plan_level"],
      accessMode: "local_cli",
    },
  ],
  capabilities: [
    {
      capabilityId: "quota_window_5h",
      sourceId: "openai-codex-local",
      displayName: "本机 Codex · 5 小时窗口",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "percent", primary: "62.5%", secondary: "已使用 37.5%", progress: 0.625 },
      trend: [],
    },
    {
      capabilityId: "quota_window_7d",
      sourceId: "openai-codex-local",
      displayName: "本机 Codex · 7 天窗口",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "percent", primary: "81.0%", secondary: "已使用 19.0%", progress: 0.81 },
      trend: [],
    },
    {
      capabilityId: "credits",
      sourceId: "openai-codex-local",
      displayName: "本机 Codex · Credits",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "credits", primary: "12.34", secondary: "仅展示额度接口实际返回值", progress: null },
      trend: [],
    },
    {
      capabilityId: "plan_level",
      sourceId: "openai-codex-local",
      displayName: "本机 Codex · 订阅计划",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "text", primary: "Plus", secondary: "ChatGPT / Codex 订阅", progress: null },
      trend: [],
    },
    {
      capabilityId: "quota_window_5h",
      sourceId: "openai-codex-extra-2",
      displayName: "额外账号 · 5 小时窗口",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "percent", primary: "40.0%", secondary: "已使用 60.0%", progress: 0.4 },
      trend: [],
    },
    {
      capabilityId: "quota_window_7d",
      sourceId: "openai-codex-extra-2",
      displayName: "额外账号 · 7 天窗口",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "percent", primary: "66.0%", secondary: "已使用 34.0%", progress: 0.66 },
      trend: [],
    },
    {
      capabilityId: "plan_level",
      sourceId: "openai-codex-extra-2",
      displayName: "额外账号 · 订阅计划",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "text", primary: "Pro", secondary: null, progress: null },
      trend: [],
    },
    {
      capabilityId: "credits",
      sourceId: "openai-codex-extra-2",
      displayName: "额外账号 · Credits",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "credits", primary: "3.21", secondary: null, progress: null },
      trend: [],
    },
    {
      capabilityId: "quota_window_5h",
      sourceId: "openai-codex-extra-3",
      displayName: "额外账号 · 5 小时窗口",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "percent", primary: "18.0%", secondary: "已使用 82.0%", progress: 0.18 },
      trend: [],
    },
    {
      capabilityId: "quota_window_7d",
      sourceId: "openai-codex-extra-3",
      displayName: "额外账号 · 7 天窗口",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "percent", primary: "44.0%", secondary: "已使用 56.0%", progress: 0.44 },
      trend: [],
    },
    {
      capabilityId: "plan_level",
      sourceId: "openai-codex-extra-3",
      displayName: "额外账号 · 订阅计划",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "text", primary: "Free", secondary: null, progress: null },
      trend: [],
    },
  ],
};

const errorPlatform: PlatformSummaryViewModel = {
  ...healthyPlatform,
  aggregateStatus: "error",
  sources: [
    {
      ...healthyPlatform.sources[0],
      state: "error",
      errorCode: "PREVIEW_ERROR",
      errorMessage: "示例：暂时无法刷新，保留上次成功数据",
    },
  ],
  capabilities: healthyPlatform.capabilities.map((capability) => ({
    ...capability,
    freshness: "stale",
  })),
};

const glmDualPlatform: PlatformSummaryViewModel = {
  providerId: "glm",
  displayName: "GLM 国内",
  aggregateStatus: "healthy",
  accessSummary: "Token Plan + 个人余额",
  sources: [
    {
      sourceId: "glm-coding-plan",
      sourceType: "api_key",
      displayName: "Coding Plan",
      state: "ready",
      credentialConfigured: true,
      lastValidatedAt: null,
      lastSuccessAt: null,
      errorCode: null,
      errorMessage: null,
      capabilityIds: ["quota_window_5h", "quota_window_7d", "plan_level"],
      accessMode: "coding_plan",
    },
    {
      sourceId: "glm-web-balance",
      sourceType: "web_session",
      displayName: "网页个人余额",
      state: "ready",
      credentialConfigured: true,
      lastValidatedAt: null,
      lastSuccessAt: null,
      errorCode: null,
      errorMessage: null,
      capabilityIds: ["balance"],
      accessMode: "personal_balance",
    },
  ],
  capabilities: [
    {
      capabilityId: "quota_window_5h",
      sourceId: "glm-coding-plan",
      displayName: "5 小时窗口",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "percent", primary: "72%", secondary: null, progress: 0.72 },
      trend: [],
    },
    {
      capabilityId: "quota_window_7d",
      sourceId: "glm-coding-plan",
      displayName: "周窗口",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "percent", primary: "80%", secondary: null, progress: 0.8 },
      trend: [],
    },
    {
      capabilityId: "plan_level",
      sourceId: "glm-coding-plan",
      displayName: "订阅计划",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "text", primary: "Pro", secondary: "官方 Coding Plan", progress: null },
      trend: [],
    },
    {
      capabilityId: "balance",
      sourceId: "glm-web-balance",
      displayName: "账户余额",
      freshness: "fresh",
      capturedAt: null,
      lastGoodAt: null,
      value: { kind: "money", primary: "¥88.10", secondary: "网页个人余额", progress: null },
      trend: [],
    },
  ],
};

const previewPlatforms: PlatformSummaryViewModel[] = [
  gptPlatform,
  glmDualPlatform,
  healthyPlatform,
  errorPlatform,
];

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

function HoverbarPreview() {
  const { theme, toggleTheme } = useHoverbarTheme();

  return (
    <main className="hb-preview-page">
      <header>
        <h1>悬浮球状态预览</h1>
        <p>开发验收页，卡片金额为示例数据，不来自真实平台。</p>
      </header>

      <div className="hb-preview-orb-grid">
        <OrbState label="默认" />
        <OrbState label="悬停" forceState="hover" />
        <OrbState label="键盘焦点" forceState="focus" />
        <OrbState label="按下" forceState="active" />
        <OrbState label="禁用" disabled />
      </div>

      <section className="hb-preview-detail-section">
        <h2>方案 A：一张卡多行，5 小时窗口 / 套餐 / Credits</h2>
        <div className="hb-preview-detail-frame">
          <div className="hb-detail-root" data-edge="right" data-motion="visible">
            <section className="hb-panel">
              <header className="hb-head">
                <p className="hb-refresh-status">刚刚更新</p>
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
                    {theme === "dark" ? (
                      <SunMedium size={16} aria-hidden />
                    ) : (
                      <Moon size={16} aria-hidden />
                    )}
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
                {previewPlatforms.map((platform) => (
                  <HoverbarPlatformCard key={platform.providerId} platform={platform} />
                ))}
              </div>
            </section>
          </div>
        </div>
      </section>

      <div className="hb-preview-panel-grid">
        <section>
          <h2>成功</h2>
          <HoverbarPlatformCard platform={healthyPlatform} />
        </section>
        <section>
          <h2>GPT 窗口额度</h2>
          <HoverbarPlatformCard platform={gptPlatform} />
        </section>
        <section>
          <h2>错误与缓存</h2>
          <HoverbarPlatformCard platform={errorPlatform} />
        </section>
        <section>
          <h2>加载</h2>
          <button
            type="button"
            className="hb-action-button hb-preview-loading"
            data-loading="true"
            aria-label="刷新中"
            disabled
          >
            <RefreshCw size={16} aria-hidden />
          </button>
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
    <HoverbarPreview />
  </React.StrictMode>,
);
