import { useState } from "react";
import { Card } from "@/components/ui/Card";
import { cn } from "@/lib/cn";

/**
 * GPT 重置雷达（V1，设计稿 05/06/07 预留）：
 * 内部固定三个 Tab——信号摘要 / Tibo 动态 / AI 辅助分析。
 * V1 从 Codex Radar 同步其转载的 Tibo 英文原文，再交给用户配置的可选 AI 分析。
 * 阶段一仅实现页面骨架与结构占位；内容同步与 AI 分析均为阶段四业务，
 * 缺失数据一律显示「未接入」，不使用设计稿示例数字补位。
 */
export function GptRadarPage() {
  const [tab, setTab] = useState<RadarTabId>("signal");

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-6">
      {/* RadarHeader */}
      <header className="glass-panel flex flex-wrap items-start justify-between gap-3 px-5 py-4">
        <div>
          <div className="flex items-center gap-3">
            <h1 className="text-lg font-semibold tracking-tight text-q-text-primary">GPT 重置雷达</h1>
            <span className="rounded-q-pill bg-q-neutral-soft px-2.5 py-1 text-xs text-q-neutral">
              阶段四实现 · 当前为结构占位
            </span>
          </div>
          <p className="mt-1 max-w-2xl text-[13px] leading-relaxed text-q-text-secondary">
            V1 从 Codex Radar 同步 Tibo 英文原文，由用户配置的可选 AI 辅助研判；
            直接访问 X 留作后续 Source。所有结论仅为推测，不代表官方结论。
          </p>
        </div>
        <button
          type="button"
          disabled
          title="阶段四接入"
          className="inline-flex h-9 cursor-not-allowed items-center gap-2 rounded-q-control bg-q-primary px-4 text-sm font-medium text-white opacity-50"
        >
          立即检查
        </button>
      </header>

      {/* RadarTabs */}
      <div
        role="tablist"
        aria-label="GPT 重置雷达视图"
        className="inline-flex w-fit items-center gap-1 rounded-q-control border border-q-border bg-q-surface-muted p-1"
      >
        {RADAR_TABS.map((item) => (
          <button
            key={item.id}
            role="tab"
            type="button"
            aria-selected={tab === item.id}
            onClick={() => setTab(item.id)}
            className={cn(
              "cursor-pointer rounded-[7px] px-4 py-1.5 text-[13px] font-medium transition-colors duration-150",
              "text-q-text-secondary hover:text-q-text-primary",
              "data-[active=true]:bg-white data-[active=true]:text-q-primary data-[active=true]:shadow-q-sm",
            )}
            data-active={tab === item.id}
          >
            {item.label}
          </button>
        ))}
      </div>

      {tab === "signal" && <SignalSummaryView />}
      {tab === "tibo" && <TiboFeedView />}
      {tab === "ai" && <AiAnalysisView />}
    </div>
  );
}

type RadarTabId = "signal" | "tibo" | "ai";

const RADAR_TABS: Array<{ id: RadarTabId; label: string }> = [
  { id: "signal", label: "信号摘要" },
  { id: "tibo", label: "Tibo 动态" },
  { id: "ai", label: "AI 辅助分析" },
];

/** 信号摘要（05）：来源状态、AI 辅助结论、独立额度上下文和检查历史。 */
function SignalSummaryView() {
  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-3 gap-4">
        <Card className="flex flex-col gap-3">
          <h2 className="text-sm font-semibold text-q-text-primary">Codex Radar 来源</h2>
          <p className="rounded-q-control border border-q-border bg-q-neutral-soft px-3 py-2 text-[13px] text-q-neutral">
            未接入：待 `CodexRadarSource` 接入后展示同步状态、最后成功时间和 freshness。
          </p>
          <p className="text-xs leading-relaxed text-q-text-muted">
            来源失败时保留最后成功的 Tibo 快照并标记 stale；没有真实快照时显示 missing。
          </p>
        </Card>
        <Card className="flex flex-col gap-3">
          <h2 className="text-sm font-semibold text-q-text-primary">AI 辅助结论</h2>
          <p className="rounded-q-control border border-q-border bg-q-neutral-soft px-3 py-2 text-[13px] text-q-neutral">
            未分析：AI 默认关闭，启用后只分析 Codex Radar 同步的 Tibo 英文原文。
          </p>
          <p className="text-xs leading-relaxed text-q-text-muted">
            输出必须引用原文并展示把握度、正反依据和不确定性；仅为推测，不代表官方结论。
          </p>
        </Card>
        <Card className="flex flex-col gap-3">
          <h2 className="text-sm font-semibold text-q-text-primary">独立额度上下文</h2>
          <p className="text-[13px] leading-relaxed text-q-text-secondary">
            窗口额度与 Credits 来自 GPT / Codex 平台 Source，接入后独立展示，不把估算值当成重置结论。
          </p>
          <div className="mt-auto rounded-q-control border border-dashed border-q-border-strong px-3 py-3 text-center text-xs text-q-text-muted">
            未接入
          </div>
        </Card>
      </div>
      <Card className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-q-text-primary">检查历史</h2>
        <p className="text-xs text-q-text-muted">
          尚未执行检查；「立即检查」将展示 Codex Radar 同步、原文解析与可选 AI 分析三个阶段的独立进度。
        </p>
      </Card>
    </div>
  );
}

/** Tibo 动态（06）：左列表右详情联动，不使用遮挡弹窗。 */
function TiboFeedView() {
  return (
    <div className="grid min-h-[320px] grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)] gap-4">
      <Card className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-q-text-primary">Codex Radar 同步动态</h2>
        <p className="text-xs leading-relaxed text-q-text-muted">
          V1 只展示 Codex Radar 转载的 Tibo 动态；列表标签来自用户配置 AI，未分析时明确显示未分析。
        </p>
        <div className="mt-auto rounded-q-control border border-dashed border-q-border-strong px-3 py-4 text-center text-xs text-q-text-muted">
          未接入：待 CodexRadarSource 接入
        </div>
      </Card>
      <Card className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-q-text-primary">动态详情</h2>
        <p className="text-xs leading-relaxed text-q-text-muted">
          英文原文、发布时间、X 原帖链接、Codex Radar 来源链接、同步时间和 freshness 分区显示。
          中文翻译仅供阅读，不参与 AI 输入；上游信号标签和模型语境解读也不进入用户配置 AI 分析。
        </p>
      </Card>
    </div>
  );
}

/** AI 辅助分析（07）：用户配置 AI、输入预览、引用式结论和分析历史。 */
function AiAnalysisView() {
  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-2 gap-4">
        <Card className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">AI 分析配置</h2>
          <p className="text-xs leading-relaxed text-q-text-muted">
            AI 默认关闭。用户选择已安全配置的提供商、模型和分析范围；前端只接收模型别名，不接触完整凭据。
          </p>
        </Card>
        <Card className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">本次分析输入</h2>
          <p className="text-xs leading-relaxed text-q-text-muted">
            只发送 Tibo 公开英文原文、发布时间和原帖链接；不发送翻译、上游判断、账号、额度、凭据或 Cookie。
          </p>
        </Card>
      </div>
      <Card className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-q-text-primary">辅助结论与历史</h2>
        <p className="text-xs leading-relaxed text-q-text-muted">
          结论包含把握度、引用原文、支持依据、反向依据、不确定性、模型和分析时间。
          相同原文、模型与提示词版本复用结果；没有真实原文或分析失败时不生成结论。
        </p>
      </Card>
    </div>
  );
}
