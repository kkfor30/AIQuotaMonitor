/**
 * 悬浮球组件开发预览：只在 Vite 开发环境手工验收，不参与 Tauri 产品入口。
 * 示例值均明确标注为预览数据，避免与真实平台快照混淆。
 */
import React, { useState } from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ExternalLink, Moon, RefreshCw, SunMedium, X } from "lucide-react";
import type {
  CapabilitySnapshotViewModel,
  PlatformSummaryViewModel,
  SourceAccessMode,
  SourceState,
  SourceSummaryViewModel,
} from "@/lib/types";
import type { RadarSnapshot } from "@/lib/ipc";
import "@/styles/global.css";
import { HoverbarOrb } from "./HoverbarAnchorApp";
import { HoverbarPlatformCard } from "./HoverbarPlatformCard";
import { HoverbarRadarDetail } from "./HoverbarRadarDetail";
import { useHoverbarTheme } from "./hoverbar-theme";
import type { HoverbarEdge } from "./hoverbar-state";

const noop = () => undefined;

declare global {
  interface Window {
    __HOVERBAR_PREVIEW_ROOT__?: ReturnType<typeof ReactDOM.createRoot>;
  }
}

function source(
  sourceId: string,
  displayName: string,
  capabilityIds: string[],
  accessMode: SourceAccessMode,
  state: SourceState = "ready",
): SourceSummaryViewModel {
  return {
    sourceId,
    sourceType: accessMode === "local_cli" ? "local_cli" : accessMode === "personal_balance" ? "web_session" : "api_key",
    displayName,
    state,
    credentialConfigured: true,
    lastValidatedAt: null,
    lastSuccessAt: null,
    errorCode: state === "error" ? "PREVIEW_ERROR" : null,
    errorMessage: state === "error" ? "示例：暂时无法刷新" : null,
    capabilityIds,
    accessMode,
  };
}

function cap(
  capabilityId: string,
  sourceId: string,
  displayName: string,
  primary: string | null,
  secondary: string | null,
  freshness: CapabilitySnapshotViewModel["freshness"] = "fresh",
): CapabilitySnapshotViewModel {
  return {
    capabilityId,
    sourceId,
    displayName,
    freshness,
    capturedAt: null,
    lastGoodAt: null,
    value: { kind: capabilityId === "balance" ? "money" : "percent", primary, secondary, progress: null },
    trend: [],
  };
}

const gptPlatform: PlatformSummaryViewModel = {
  providerId: "openai",
  displayName: "GPT / Codex",
  aggregateStatus: "partial",
  accessSummary: "本机 Codex + 额外账号",
  sources: [
    source("openai-codex-local", "本机 Codex（当前 CLI）", ["quota_window_5h", "quota_window_7d", "credits", "plan_level"], "local_cli"),
    source("openai-codex-extra-2", "额外 ChatGPT 账号 2", ["quota_window_5h", "quota_window_7d", "credits", "plan_level"], "local_cli"),
  ],
  capabilities: [
    cap("quota_window_5h", "openai-codex-local", "本机 · 5 小时窗口", "62%", "已使用 38.0% · 重置 14:30"),
    cap("quota_window_7d", "openai-codex-local", "本机 · 7 天窗口", "81%", "已使用 19.0% · 重置 09/02 08:00"),
    cap("credits", "openai-codex-local", "本机 · Credits", "12.34", "仅展示额度接口实际返回值"),
    cap("plan_level", "openai-codex-local", "本机 · 订阅计划", "Plus", "ChatGPT / Codex 订阅"),
    cap("quota_window_5h", "openai-codex-extra-2", "账号 2 · 5 小时窗口", "40%", "已使用 60.0% · 重置 16:10"),
    cap("quota_window_7d", "openai-codex-extra-2", "账号 2 · 7 天窗口", null, null, "missing"),
    cap("credits", "openai-codex-extra-2", "账号 2 · Credits", "3.21", null),
    cap("plan_level", "openai-codex-extra-2", "账号 2 · 订阅计划", "Free", null),
  ],
};

const glmPlatform: PlatformSummaryViewModel = {
  providerId: "glm",
  displayName: "GLM 国内",
  aggregateStatus: "healthy",
  accessSummary: "Token Plan + 个人余额",
  sources: [
    source("glm-coding-plan", "Coding Plan", ["quota_window_5h", "quota_window_7d", "plan_level"], "coding_plan"),
    source("glm-web-balance", "网页个人余额", ["balance"], "personal_balance"),
  ],
  capabilities: [
    cap("quota_window_5h", "glm-coding-plan", "5 小时窗口", "72%", "已使用 28.0% · 重置 15:20"),
    cap("quota_window_7d", "glm-coding-plan", "周窗口", "80%", "已使用 20.0% · 重置 09/03 09:00"),
    cap("plan_level", "glm-coding-plan", "订阅计划", "Pro", "官方 Coding Plan"),
    cap("balance", "glm-web-balance", "账户余额", "¥88.10", "网页个人余额"),
  ],
};

const deepseekHealthy: PlatformSummaryViewModel = {
  providerId: "deepseek",
  displayName: "DeepSeek",
  aggregateStatus: "healthy",
  accessSummary: "API Key",
  sources: [source("preview-balance", "余额来源", ["balance", "today_spend", "month_spend"], "personal_balance")],
  capabilities: [
    cap("balance", "preview-balance", "账户余额", "¥25.00", "赠送 ¥1.00 · 充值 ¥24.00"),
    cap("today_spend", "preview-balance", "今日消耗", "¥1.20", null),
    cap("month_spend", "preview-balance", "本月消耗", "¥8.00", null),
  ],
};

const kimiError: PlatformSummaryViewModel = {
  providerId: "kimi",
  displayName: "Kimi",
  aggregateStatus: "error",
  accessSummary: "个人余额",
  sources: [source("kimi-balance-api", "个人余额", ["balance"], "personal_balance", "error")],
  capabilities: [cap("balance", "kimi-balance-api", "账户余额", null, null, "missing")],
};

const staleGlm: PlatformSummaryViewModel = {
  ...glmPlatform,
  aggregateStatus: "partial",
  capabilities: glmPlatform.capabilities.map((item) =>
    item.capabilityId === "balance" ? { ...item, freshness: "stale" as const } : item,
  ),
};

const previewPlatforms: PlatformSummaryViewModel[] = [gptPlatform, glmPlatform, deepseekHealthy, kimiError];

/** 预览雷达快照：仅为布局验收示例，不代表任何真实信号。 */
const previewRadar: RadarSnapshot = {
  sourceStatus: "fresh",
  lastSyncedAt: Date.now() - 42 * 60 * 1000,
  posts: [
    {
      id: "preview-1",
      url: "https://x.com/tibo/status/preview-1",
      text: "Rates appear to have rolled over for a fresh window in some regions.",
      postedAt: Date.now() - 3 * 60 * 60 * 1000,
      kind: "indirect",
      badge: "间接相关",
      filter: "related",
      explicitReset: false,
      isReply: false,
      replies: 4,
      reposts: 12,
      likes: 38,
      syncedAt: Date.now() - 42 * 60 * 1000,
      translatedText: "示例翻译：部分地区额度窗口似乎已经翻滚到新一轮。",
      translatedAt: Date.now() - 40 * 60 * 1000,
      translationSource: "codexradar",
      summary: "部分地区额度窗口似乎已经翻滚到新一轮。",
      analysis: "上游解读示例：这是弱信号，不能据此确认全量重置。",
    },
    {
      id: "preview-2",
      url: "https://x.com/tibo/status/preview-2",
      text: "Limits holding steady, no reset signal observed today.",
      postedAt: Date.now() - 8 * 60 * 60 * 1000,
      kind: "none",
      badge: "无重置信号",
      filter: "none",
      explicitReset: false,
      isReply: false,
      replies: 1,
      reposts: 3,
      likes: 9,
      syncedAt: Date.now() - 42 * 60 * 1000,
      translatedText: "限额保持稳定，今天没有看到重置信号。",
      translatedAt: Date.now() - 42 * 60 * 1000,
      translationSource: "codexradar",
      summary: "限额保持稳定，今天没有看到重置信号。",
      analysis: "上游解读示例：没有重置承诺。",
    },
    {
      id: "preview-3",
      url: "https://x.com/tibo/status/preview-3",
      text: "Earlier reset landed yesterday evening.",
      postedAt: Date.now() - 26 * 60 * 60 * 1000,
      kind: "direct",
      badge: "重置相关",
      filter: "signal",
      explicitReset: true,
      isReply: false,
      replies: 8,
      reposts: 21,
      likes: 55,
      syncedAt: Date.now() - 42 * 60 * 1000,
      translatedText: "昨天傍晚的重置已经落地。",
      translatedAt: Date.now() - 30 * 60 * 1000,
      translationSource: "codexradar",
      summary: "昨天傍晚的重置已经落地。",
      analysis: null,
    },
  ],
  latest: null,
  checks: [],
  analysis: {
    id: "preview-analysis",
    createdAt: Date.now() - 39 * 60 * 1000,
    rangeKey: "3d",
    cutPostId: "preview-3",
    cutLabel: null,
    sourceId: "deepseek-balance-api",
    model: "deepseek-chat",
    conclusion: "示例结论：近期出现新的重置迹象，仍在等待更多区域确认。",
    confidence: "medium",
    citations: [],
    support: ["部分地区窗口翻滚的动态", "限额观察未再恶化"],
    against: ["尚无官方公告"],
    uncertainty: ["区域覆盖范围未知"],
    errorMessage: null,
  },
  models: [],
  cut: null,
  notice: {
    headline: "Tibo：明天可能迎来 Codex 新里程碑",
    lead: "请关注 Codex 仪表板",
    items: ["这是新的官方弱信号，但尚未直接确认新一轮重置。"],
  },
};
previewRadar.latest = previewRadar.posts[0];

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

function PreviewPanel({
  edge,
  platforms,
  status,
  initialView = "quota",
}: {
  edge: HoverbarEdge;
  platforms: PlatformSummaryViewModel[];
  status: string;
  initialView?: "quota" | "radar";
}) {
  const { theme, toggleTheme } = useHoverbarTheme();
  const [view, setView] = useState<"quota" | "radar">(initialView);
  return (
    <div className="hb-preview-detail-frame" data-edge={edge}>
      <div className="hb-detail-root" data-edge={edge} data-motion="visible">
        <section className="hb-panel">
          <header className="hb-head">
            <p className="hb-refresh-status">{status}</p>
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
                {theme === "dark" ? <SunMedium size={16} aria-hidden /> : <Moon size={16} aria-hidden />}
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
            <div className="hb-service-scroll">
              {view === "radar" ? (
                <HoverbarRadarDetail radar={previewRadar} onBack={() => setView("quota")} />
              ) : (
                platforms.map((platform) => (
                  <HoverbarPlatformCard
                    key={`${edge}-${platform.providerId}`}
                    platform={platform}
                    radar={platform.providerId === "openai" ? previewRadar : undefined}
                    onOpenRadar={platform.providerId === "openai" ? () => setView("radar") : undefined}
                  />
                ))
              )}
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}

function HoverbarPreview() {
  return (
    <main className="hb-preview-page">
      <header>
        <h1>悬浮球状态预览</h1>
        <p>
          开发验收页。Credits、消费和赠送/充值在预览数据中存在，但不应出现在卡片文案里；
          雷达摘要与翻译均为示例数据。
        </p>
      </header>

      <div className="hb-preview-orb-grid">
        <OrbState label="默认" />
        <OrbState label="悬停" forceState="hover" />
        <OrbState label="键盘焦点" forceState="focus" />
        <OrbState label="按下" forceState="active" />
        <OrbState label="禁用" disabled />
      </div>

      <section className="hb-preview-detail-section">
        <h2>四边停靠 · 一行一个平台 · 窄版最终稿</h2>
        <div className="hb-preview-edges">
          <div>
            <h2>顶部 420px</h2>
            <PreviewPanel edge="top" platforms={previewPlatforms} status="更新于 11:51" />
          </div>
          <div>
            <h2>底部 420px</h2>
            <PreviewPanel edge="bottom" platforms={previewPlatforms} status="更新于 11:51" />
          </div>
          <div>
            <h2>右侧 300px</h2>
            <PreviewPanel edge="right" platforms={previewPlatforms} status="更新于 11:51" />
          </div>
          <div>
            <h2>左侧 300px</h2>
            <PreviewPanel edge="left" platforms={previewPlatforms} status="更新于 11:51" />
          </div>
        </div>
      </section>

      <section className="hb-preview-detail-section">
        <h2>GPT 重置雷达二级页 · 示例数据</h2>
        <div className="hb-preview-edges">
          <div>
            <h2>右侧停靠 · 雷达页</h2>
            <PreviewPanel edge="right" platforms={previewPlatforms} status="更新于 11:51" initialView="radar" />
          </div>
          <div>
            <h2>顶部停靠 · 雷达页</h2>
            <PreviewPanel edge="top" platforms={previewPlatforms} status="更新于 11:51" initialView="radar" />
          </div>
        </div>
      </section>

      <div className="hb-preview-panel-grid">
        <section>
          <h2>正常</h2>
          <HoverbarPlatformCard platform={deepseekHealthy} />
        </section>
        <section>
          <h2>部分可用 · 缓存可能过期</h2>
          <HoverbarPlatformCard platform={staleGlm} />
        </section>
        <section>
          <h2>异常</h2>
          <HoverbarPlatformCard platform={kimiError} />
        </section>
        <section>
          <h2>GPT Plus + Free · 含雷达摘要</h2>
          <HoverbarPlatformCard
            platform={gptPlatform}
            radar={previewRadar}
            onOpenRadar={() => undefined}
          />
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
    <QueryClientProvider client={new QueryClient()}>
      <HoverbarPreview />
    </QueryClientProvider>
  </React.StrictMode>,
);
