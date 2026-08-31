import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { ChartSpline } from "lucide-react";
import { FreshnessTag } from "@/components/ui/StatusBadge";
import { formatTime } from "@/lib/format";
import type { CapabilitySnapshotViewModel } from "@/lib/types";

/** 小金额刻度：低于 0.01 元保留三位小数，避免 ¥0 或省略前导零造成误读。 */
function formatTrendMoney(value: number): string {
  return value !== 0 && Math.abs(value) < 0.01 ? `¥${value.toFixed(3)}` : `¥${value}`;
}

/**
 * 趋势模块（近 7 日消费趋势）：只绘制真实 usage_trend 消费序列，不插值、不补零；
 * 无点时显示空状态，不造曲线；stale 状态显式标注。图标与能力模块体系统一（ChartSpline）。
 */
export function UsageTrend({ capability }: { capability: CapabilitySnapshotViewModel }) {
  const data = capability.trend.map((point) => ({
    label: point.label,
    value: point.value,
  }));

  return (
    <div className="glass-panel flex flex-col gap-3 p-4">
      <div className="flex items-center gap-2.5">
        <span
          aria-hidden
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-[9px] border border-q-border/70 bg-q-surface-muted text-q-text-secondary"
        >
          <ChartSpline size={15} />
        </span>
        <h3 className="min-w-0 flex-1 truncate text-[14px] font-semibold tracking-tight text-q-text-primary">
          {capability.displayName}
        </h3>
        <FreshnessTag freshness={capability.freshness} />
      </div>
      {capability.freshness === "stale" && capability.lastGoodAt !== null && (
        <p className="-mt-1.5 text-[11px] text-q-warning">
          缓存数据 · 上次成功 {formatTime(capability.lastGoodAt)}
        </p>
      )}

      {data.length === 0 ? (
        <div
          className="flex h-[120px] w-full items-center justify-center rounded-[12px] border border-dashed border-q-border text-[13px] text-q-text-muted"
        >
          暂无真实消费记录，刷新后展示近 7 日趋势
        </div>
      ) : (
        <div className="h-[180px] w-full" data-selectable="true">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={data} margin={{ top: 6, right: 8, bottom: 0, left: -18 }}>
              <defs>
                <linearGradient id="q-trend-fill" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="var(--q-primary)" stopOpacity={0.28} />
                  <stop offset="100%" stopColor="var(--q-primary)" stopOpacity={0.02} />
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="3 6" stroke="rgba(13,27,54,0.07)" vertical={false} />
              <XAxis
                dataKey="label"
                tickLine={false}
                axisLine={false}
                tick={{ fill: "var(--q-text-muted)", fontSize: 11 }}
                dy={6}
              />
              <YAxis
                tickLine={false}
                axisLine={false}
                tick={{ fill: "var(--q-text-muted)", fontSize: 11 }}
                width={46}
                tickFormatter={formatTrendMoney}
              />
              <Tooltip
                cursor={{ stroke: "rgba(7,86,238,0.3)", strokeDasharray: "4 4" }}
                contentStyle={{
                  borderRadius: 10,
                  border: "1px solid rgba(15,23,42,0.08)",
                  boxShadow: "var(--q-shadow-md)",
                  fontSize: 12,
                  padding: "6px 10px",
                }}
                formatter={(value) => [formatTrendMoney(value as number), "消费"]}
              />
              <Area
                type="monotone"
                dataKey="value"
                stroke="var(--q-primary)"
                strokeWidth={2}
                fill="url(#q-trend-fill)"
                activeDot={{ r: 3.5, fill: "var(--q-primary)", strokeWidth: 0 }}
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      )}
    </div>
  );
}
