import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { FreshnessTag } from "@/components/ui/StatusBadge";
import { formatTime } from "@/lib/format";
import type { CapabilitySnapshotViewModel } from "@/lib/types";

/**
 * 近 7 日消费趋势（阶段一为静态缓存数据）。
 * 蓝色渐变面积图与设计稿一致；stale 状态显式标注。
 */
export function UsageTrend({ capability }: { capability: CapabilitySnapshotViewModel }) {
  const data = capability.trend.map((point) => ({
    label: point.label,
    value: point.value,
  }));

  return (
    <div className="glass-panel flex flex-col gap-3 p-4">
      <div className="flex items-center justify-between gap-2">
        <div>
          <p className="text-[13px] font-medium text-q-text-secondary">{capability.displayName}</p>
          {capability.freshness === "stale" && capability.lastGoodAt !== null && (
            <p className="mt-0.5 text-[11px] text-q-warning">
              缓存数据 · 上次成功 {formatTime(capability.lastGoodAt)}
            </p>
          )}
        </div>
        <FreshnessTag freshness={capability.freshness} />
      </div>

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
              tickFormatter={(value: number) => `¥${value}`}
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
              formatter={(value) => [`¥${value}`, "消费"]}
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
    </div>
  );
}
