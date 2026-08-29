import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { LineChart as LineChartIcon } from "lucide-react";

export type TrendSeries = {
  id: string;
  name: string;
  color: string;
  points: Array<{ label: string; value: number }>;
};

export type TrendValueKind = "money" | "percent" | "plain";

function formatValue(value: number, kind: TrendValueKind): string {
  if (kind === "money") return `¥${value}`;
  if (kind === "percent") return `${value}%`;
  return `${value}`;
}

/**
 * 多平台折线图（Apple Glass V6 总览趋势卡）。
 * 只渲染传入的真实序列；序列为空时显示空状态，不生成演示曲线。
 */
export function TrendLineChart({
  series,
  valueKind = "money",
  height = 200,
  emptyTitle,
  emptyDescription,
}: {
  series: TrendSeries[];
  valueKind?: TrendValueKind;
  height?: number;
  emptyTitle: string;
  emptyDescription: string;
}) {
  if (series.length === 0) {
    return (
      <div
        className="flex flex-col items-center justify-center gap-2 rounded-q-control border border-dashed border-q-border bg-q-surface-muted/50 px-6 text-center"
        style={{ height }}
      >
        <LineChartIcon size={22} className="text-q-text-muted" aria-hidden />
        <p className="text-[13px] font-medium text-q-text-secondary">{emptyTitle}</p>
        <p className="max-w-xs text-xs leading-relaxed text-q-text-muted">{emptyDescription}</p>
      </div>
    );
  }

  // 按序列出现顺序合并 x 轴标签，缺失点为 null（断点，不补零）。
  const labels: string[] = [];
  for (const item of series) {
    for (const point of item.points) {
      if (!labels.includes(point.label)) labels.push(point.label);
    }
  }
  const data = labels.map((label) => {
    const row: Record<string, string | number | null> = { label };
    for (const item of series) {
      row[item.id] = item.points.find((point) => point.label === label)?.value ?? null;
    }
    return row;
  });
  const unitSuffix = valueKind === "money" ? "（¥）" : "";

  return (
    <div className="flex min-w-0 flex-col gap-2" data-selectable="true">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5">
        {series.map((item) => (
          <span
            key={item.id}
            className="inline-flex items-center gap-1.5 text-[11px] font-medium text-q-text-secondary"
          >
            <span
              aria-hidden
              className="h-2 w-2 rounded-full"
              style={{ backgroundColor: item.color }}
            />
            {item.name}
            {unitSuffix}
          </span>
        ))}
      </div>
      <div style={{ height }}>
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: -14 }}>
            <CartesianGrid strokeDasharray="3 6" stroke="rgba(13,27,54,0.07)" vertical={false} />
            <XAxis
              dataKey="label"
              tickLine={false}
              axisLine={false}
              tick={{ fill: "#6b7890", fontSize: 11 }}
              dy={6}
            />
            <YAxis
              tickLine={false}
              axisLine={false}
              tick={{ fill: "#6b7890", fontSize: 11 }}
              width={46}
              domain={valueKind === "percent" ? [0, 100] : ["auto", "auto"]}
              ticks={valueKind === "percent" ? [0, 25, 50, 75, 100] : undefined}
              tickFormatter={(value: number) => formatValue(value, valueKind)}
            />
            <Tooltip
              cursor={{ stroke: "rgba(10,102,255,0.3)", strokeDasharray: "4 4" }}
              contentStyle={{
                borderRadius: 12,
                border: "1px solid rgba(13,27,54,0.08)",
                background: "rgba(255,255,255,0.94)",
                boxShadow: "var(--q-shadow-md)",
                fontSize: 12,
                padding: "6px 10px",
              }}
              formatter={(value, name) => {
                const item = series.find((entry) => entry.name === name);
                return [formatValue(Number(value), valueKind), item?.name ?? String(name)];
              }}
            />
            {series.map((item) => (
              <Line
                key={item.id}
                dataKey={item.id}
                name={item.name}
                stroke={item.color}
                strokeWidth={2}
                type="monotone"
                connectNulls
                dot={{ r: 2.5, fill: item.color, strokeWidth: 0 }}
                activeDot={{ r: 4, fill: item.color, strokeWidth: 0 }}
              />
            ))}
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
