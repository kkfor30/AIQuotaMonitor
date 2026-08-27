import { useState } from "react";
import { Card } from "@/components/ui/Card";
import { cn } from "@/lib/cn";

/**
 * GPT 重置雷达（product-shell-v5，设计稿 05/06/07 预留）：
 * 内部固定三个 Tab——信号摘要 / Tibo 动态 / 规则与 AI。
 * 阶段一仅实现页面骨架与结构占位；抓取、规则计算与 AI 均为阶段四业务，
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
            只服务 GPT 重置判断。公开来源、本地规则和可选 AI 独立产出证据；
            综合判断仅为推测，不代表官方结论。
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
      {tab === "rules" && <RulesAiView />}
    </div>
  );
}

type RadarTabId = "signal" | "tibo" | "rules";

const RADAR_TABS: Array<{ id: RadarTabId; label: string }> = [
  { id: "signal", label: "信号摘要" },
  { id: "tibo", label: "Tibo 动态" },
  { id: "rules", label: "规则与 AI" },
];

/** 信号摘要（05）：综合判断、额度上下文、独立证据链、检查历史。 */
function SignalSummaryView() {
  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)] gap-4">
        <Card className="flex flex-col gap-3">
          <h2 className="text-sm font-semibold text-q-text-primary">综合判断</h2>
          <p className="rounded-q-control border border-q-border bg-q-neutral-soft px-3 py-2 text-[13px] text-q-neutral">
            未接入：待信号源接入后给出推测性判断，并始终标注「仅为推测，不代表官方结论」。
          </p>
          <div className="grid grid-cols-3 gap-3 text-xs">
            {["公开来源证据", "本地规则命中", "AI 分析（默认关闭）"].map((label) => (
              <div key={label} className="glass-inset px-3 py-2">
                <p className="text-q-text-muted">{label}</p>
                <p className="mt-1 font-medium text-q-text-secondary">未提供</p>
              </div>
            ))}
          </div>
        </Card>
        <Card className="flex flex-col gap-3">
          <h2 className="text-sm font-semibold text-q-text-primary">额度上下文</h2>
          <p className="text-[13px] leading-relaxed text-q-text-secondary">
            窗口额度、Credits 与可重置次数来自 GPT / Codex 平台 Source，接入后在此展示。
          </p>
          <div className="mt-auto rounded-q-control border border-dashed border-q-border-strong px-3 py-3 text-center text-xs text-q-text-muted">
            未接入
          </div>
        </Card>
      </div>
      <Card className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-q-text-primary">检查历史</h2>
        <p className="text-xs text-q-text-muted">尚未执行检查；「立即检查」将展示抓取、规则计算与可选 AI 三个阶段的独立进度。</p>
      </Card>
    </div>
  );
}

/** Tibo 动态（06）：左列表右详情联动，不使用遮挡弹窗。 */
function TiboFeedView() {
  return (
    <div className="grid min-h-[320px] grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)] gap-4">
      <Card className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-q-text-primary">动态列表</h2>
        <p className="text-xs text-q-text-muted">筛选（来源 / 相关性 / 时间）与列表占位；「内容相关」不等于「新重置信号」，上一轮内容会明确标注。</p>
        <div className="mt-auto rounded-q-control border border-dashed border-q-border-strong px-3 py-4 text-center text-xs text-q-text-muted">
          未接入：待抓取源接入
        </div>
      </Card>
      <Card className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-q-text-primary">动态详情</h2>
        <p className="text-xs leading-relaxed text-q-text-muted">
          原文、翻译、来源、抓取时间、规则命中与额度关联将分区显示；
          「加入证据」只改变当前检查的证据集合，不直接修改综合结论。
        </p>
      </Card>
    </div>
  );
}

/** 规则与 AI（07）：可视化条件构建器，不暴露任意脚本编辑器。 */
function RulesAiView() {
  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-2 gap-4">
        <Card className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">规则列表</h2>
          <p className="text-xs leading-relaxed text-q-text-muted">
            规则拥有优先级、权重、时间窗口、冷却期、启用状态与版本历史；
            保存前展示最近 30 天回测摘要，回测不修改正式规则。
          </p>
        </Card>
        <Card className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-q-text-primary">条件构建器</h2>
          <p className="text-xs leading-relaxed text-q-text-muted">
            可视化条件构建占位；MVP 不提供任意脚本编辑器。
          </p>
        </Card>
      </div>
      <Card className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-q-text-primary">AI 分析</h2>
        <p className="text-xs leading-relaxed text-q-text-muted">
          AI 默认关闭，影响最终结论的权重有上限；发送内容仅限公开文本与脱敏摘要，
          禁止发送凭据和 Cookie。审计日志在此记录。
        </p>
      </Card>
    </div>
  );
}
