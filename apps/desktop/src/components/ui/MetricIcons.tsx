/**
 * 指标小图标（DeepSeek 用量重设计 V1，视觉基准 03-metric-icons-target-final.png）：
 * - Flash：蓝青晶体闪电（双切面）。
 * - Pro：紫色六边神经核心（外环 + 顶点节点 + 中心六边核）。
 * - 靶心数据环：蓝青同心分段环 + 中心点亮，用于缓存命中率。
 * 仅作为轻量 SVG 实现，统一视觉重量；装饰性元素一律 aria-hidden，
 * 旁边必须保留文字标签，禁止单独以图标传达数据。
 */
import { useId } from "react";

type MetricIconProps = {
  size?: number;
  className?: string;
};

/** 蓝青晶体闪电：上切面深蓝、下切面青蓝，模拟晶体折面。 */
export function FlashCrystalIcon({ size = 16, className }: MetricIconProps) {
  const gradientId = useId();
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
      className={className}
    >
      <defs>
        <linearGradient id={`${gradientId}-deep`} x1="10" y1="2" x2="8" y2="14" gradientUnits="userSpaceOnUse">
          <stop stopColor="#2E6BE6" />
          <stop offset="1" stopColor="#0A4FD6" />
        </linearGradient>
        <linearGradient id={`${gradientId}-cyan`} x1="9" y1="13" x2="19" y2="22" gradientUnits="userSpaceOnUse">
          <stop stopColor="#37B6F0" />
          <stop offset="1" stopColor="#1467D6" />
        </linearGradient>
      </defs>
      {/* 上切面：晶体上半段 */}
      <path d="M13.4 2 4.9 13.3h5.3L13.4 8V2Z" fill={`url(#${gradientId}-deep)`} />
      {/* 下切面：晶体下半段，含右移的下半笔 */}
      <path d="M10.2 13.3 8.7 22l10.7-12.5h-6L10.2 13.3Z" fill={`url(#${gradientId}-cyan)`} />
      {/* 中脊高光：细亮线提升晶体感 */}
      <path d="M13.4 2v6l-3.2 5.3h1.9l1.3-2.1V2Z" fill="#8FD4F6" opacity={0.55} />
    </svg>
  );
}

/** 紫色六边神经核心：外六边环 + 六个顶点节点 + 中心实心六边核与白色芯点。 */
export function ProCoreIcon({ size = 16, className }: MetricIconProps) {
  const gradientId = useId();
  // 尖顶六边形顶点（外环 r=8.5 / 内核 r=4.1，圆心 12,12）
  const outer: Array<[number, number]> = [
    [12, 3.5],
    [19.36, 7.75],
    [19.36, 16.25],
    [12, 20.5],
    [4.64, 16.25],
    [4.64, 7.75],
  ];
  const inner: Array<[number, number]> = [
    [12, 7.9],
    [15.55, 9.95],
    [15.55, 14.05],
    [12, 16.1],
    [8.45, 14.05],
    [8.45, 9.95],
  ];
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
      className={className}
    >
      <defs>
        <linearGradient id={gradientId} x1="6" y1="5" x2="19" y2="19" gradientUnits="userSpaceOnUse">
          <stop stopColor="#A855F7" />
          <stop offset="1" stopColor="#7C3AED" />
        </linearGradient>
      </defs>
      {/* 辐射连线：外顶点 → 内核角 */}
      {outer.map(([x, y], index) => {
        const [cx, cy] = inner[index];
        return <line key={index} x1={x} y1={y} x2={cx} y2={cy} stroke="#A855F7" strokeWidth={1.1} opacity={0.65} />;
      })}
      {/* 外六边环 */}
      <polygon
        points={outer.map(([x, y]) => `${x},${y}`).join(" ")}
        stroke={`url(#${gradientId})`}
        strokeWidth={1.4}
        strokeLinejoin="round"
        fill="none"
      />
      {/* 顶点节点 */}
      {outer.map(([x, y], index) => (
        <circle key={index} cx={x} cy={y} r={1.9} fill={`url(#${gradientId})`} />
      ))}
      {/* 中心六边核 */}
      <polygon
        points={inner.map(([x, y]) => `${x},${y}`).join(" ")}
        fill={`url(#${gradientId})`}
        strokeLinejoin="round"
      />
      <circle cx={12} cy={12} r={1.7} fill="#F3E8FF" />
    </svg>
  );
}

/** 蓝青靶心数据环：中心亮核 + 中环 + 分段外环，非额度语义的效率图标。 */
export function TargetRingIcon({ size = 16, className }: MetricIconProps) {
  const gradientId = useId();
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
      className={className}
    >
      <defs>
        <linearGradient id={gradientId} x1="4" y1="4" x2="20" y2="20" gradientUnits="userSpaceOnUse">
          <stop stopColor="#2E6BE6" />
          <stop offset="1" stopColor="#38BDF0" />
        </linearGradient>
      </defs>
      {/* 分段外环：四段弧，留缺口模拟数据环 */}
      <circle
        cx={12}
        cy={12}
        r={9.4}
        stroke={`url(#${gradientId})`}
        strokeWidth={1.9}
        strokeLinecap="round"
        strokeDasharray="10.5 4.3"
        strokeDashoffset={5}
        fill="none"
      />
      {/* 中环 */}
      <circle
        cx={12}
        cy={12}
        r={5.9}
        stroke={`url(#${gradientId})`}
        strokeWidth={1.9}
        strokeLinecap="round"
        strokeDasharray="6.4 2.9"
        strokeDashoffset={3}
        fill="none"
        opacity={0.85}
      />
      {/* 中心亮核 */}
      <circle cx={12} cy={12} r={3.1} fill={`url(#${gradientId})`} />
      <circle cx={12} cy={12} r={1.15} fill="#EAF6FF" />
    </svg>
  );
}
