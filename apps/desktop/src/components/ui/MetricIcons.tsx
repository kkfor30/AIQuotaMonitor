/**
 * 指标小图标（DeepSeek 用量重设计 V2，取代 V1；视觉基准 docs/ui-design/2026-09-01-deepseek-usage-redesign-v2 三张最终稿）：
 * - FlashCrystalIcon：蓝青晶体翼/速度火花，V4 Flash 专属身份（旗舰轻量模型）。
 * - VisionApertureIcon：青色相机光圈/视觉棱镜，V4 Flash Vision 专属身份（视觉模型），禁止与 Flash 混用。
 * - ProCoreIcon：紫色分层神经旋涡/推理核心，V4 Pro 专属身份（深度思考模型）。
 * - TargetRingIcon：蓝青靶心数据环，专用于缓存命中率（总/分模型）。
 * - EfficiencyGaugeIcon：深蓝仪表弧线/指针，只用于「调用与缓存效率」面板标题，不做靶心构图。
 * 全部为轻量 SVG，统一视觉重量；装饰性元素一律 aria-hidden，旁边必须保留文字标签，
 * 禁止单独以图标传达数据。模型身份图标只用于「模型用量」，缓存值一律靶心。
 */
import { useId } from "react";

type MetricIconProps = {
  size?: number;
  className?: string;
};

/** 按模型能力 id 返回身份图标（vision 需在 flash 之前判定，避免前缀误配）。 */
export type ModelIconKind = "flash" | "vision" | "pro";

export function modelIconKind(capabilityId: string): ModelIconKind | null {
  if (capabilityId.includes("_vision")) return "vision";
  if (capabilityId.includes("_pro")) return "pro";
  if (capabilityId.includes("_flash")) return "flash";
  return null;
}

/**
 * V4 Flash 身份：蓝青晶体翼（斜四芒分面，速度火花语义，非普通闪电）。
 * 左/上翼深蓝晶体面，右/下翼青蓝亮面，中脊高光。
 */
export function FlashCrystalIcon({ size = 16, className }: MetricIconProps) {
  const gradientId = useId();
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden="true" className={className}>
      <defs>
        <linearGradient id={`${gradientId}-deep`} x1="5" y1="4" x2="12" y2="20" gradientUnits="userSpaceOnUse">
          <stop stopColor="#2E6BE6" />
          <stop offset="1" stopColor="#0A4FD6" />
        </linearGradient>
        <linearGradient id={`${gradientId}-cyan`} x1="12" y1="6" x2="21" y2="20" gradientUnits="userSpaceOnUse">
          <stop stopColor="#3FC0F2" />
          <stop offset="1" stopColor="#1467D6" />
        </linearGradient>
      </defs>
      {/* 晶体翼全形：斜四芒（上尖 → 右尖 → 下尖 → 左尖），中心略收成翼面 */}
      <path
        d="M13.1 2 15.3 8.7 22 12l-6.7 3.3L13.1 22l-2.2-6.7L4.2 12l6.7-3.3L13.1 2Z"
        fill={`url(#${gradientId}-deep)`}
      />
      {/* 右上/右下亮面：与深面拼接出晶体折面 */}
      <path d="M13.1 2 15.3 8.7 22 12h-8.9V2Z" fill={`url(#${gradientId}-cyan)`} />
      <path d="M13.1 22l-2.2-6.7 8.9-3.3-6.7 10Z" fill={`url(#${gradientId}-cyan)`} opacity={0.72} />
      {/* 中脊高光 */}
      <path d="M13.1 4.4v15.2" stroke="#9FDFF8" strokeWidth={0.9} opacity={0.6} strokeLinecap="round" />
    </svg>
  );
}

/**
 * V4 Flash Vision 身份：青色相机光圈（六叶旋转光圈，视觉/棱镜语义）。
 * 六片弧形光圈叶绕中心排布，中心取景孔，外圈细环收边。
 */
export function VisionApertureIcon({ size = 16, className }: MetricIconProps) {
  const gradientId = useId();
  // 单片光圈叶：外缘 60° 弧切入中心，旋转六次成完整光圈
  const blade = "M12 2.6 A9.4 9.4 0 0 1 20.14 7.3 L12 12 Z";
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden="true" className={className}>
      <defs>
        <linearGradient id={gradientId} x1="5" y1="4" x2="19" y2="20" gradientUnits="userSpaceOnUse">
          <stop stopColor="#38D0F0" />
          <stop offset="1" stopColor="#0E86C8" />
        </linearGradient>
      </defs>
      <g fill={`url(#${gradientId})`}>
        {[0, 60, 120, 180, 240, 300].map((angle) => (
          <path key={angle} d={blade} transform={`rotate(${angle} 12 12)`} opacity={angle % 120 === 0 ? 1 : 0.82} />
        ))}
      </g>
      {/* 中心取景孔 */}
      <circle cx={12} cy={12} r={2.5} fill="#EAF9FF" />
      <circle cx={12} cy={12} r={9.4} stroke={`url(#${gradientId})`} strokeWidth={1.1} fill="none" opacity={0.55} />
    </svg>
  );
}

/**
 * V4 Pro 身份：紫色分层神经旋涡（推理核心语义，强调深度思考）。
 * 三层异角螺旋弧由内向外展开 + 中心亮核，打破对称形成旋涡感。
 */
export function ProCoreIcon({ size = 16, className }: MetricIconProps) {
  const gradientId = useId();
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden="true" className={className}>
      <defs>
        <linearGradient id={gradientId} x1="5" y1="5" x2="20" y2="19" gradientUnits="userSpaceOnUse">
          <stop stopColor="#A855F7" />
          <stop offset="1" stopColor="#7C3AED" />
        </linearGradient>
      </defs>
      {/* 内层弧 */}
      <path
        d="M13.9 10.1 A2.7 2.7 0 1 0 14.6 13.6"
        stroke={`url(#${gradientId})`}
        strokeWidth={1.9}
        strokeLinecap="round"
        fill="none"
      />
      {/* 中层弧：异角错位 */}
      <path
        d="M9.3 8.5 A5.9 5.9 0 1 0 16.5 9.9"
        stroke={`url(#${gradientId})`}
        strokeWidth={1.7}
        strokeLinecap="round"
        fill="none"
        opacity={0.85}
      />
      {/* 外层弧 */}
      <path
        d="M17.9 6.3 A8.9 8.9 0 1 0 20.4 13.9"
        stroke={`url(#${gradientId})`}
        strokeWidth={1.5}
        strokeLinecap="round"
        fill="none"
        opacity={0.7}
      />
      <circle cx={12} cy={12} r={1.5} fill={`url(#${gradientId})`} />
    </svg>
  );
}

/** 蓝青靶心数据环：专用于缓存命中率（总/分模型），中心亮核 + 分段双环。 */
export function TargetRingIcon({ size = 16, className }: MetricIconProps) {
  const gradientId = useId();
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden="true" className={className}>
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

/**
 * 深蓝效率仪表：只用于「调用与缓存效率」面板标题。
 * 开口向下的 220° 弧线 + 三段刻度 + 指向右上高区的指针，非靶心构图。
 */
export function EfficiencyGaugeIcon({ size = 16, className }: MetricIconProps) {
  const gradientId = useId();
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden="true" className={className}>
      <defs>
        <linearGradient id={gradientId} x1="3" y1="6" x2="21" y2="20" gradientUnits="userSpaceOnUse">
          <stop stopColor="#1D4ED8" />
          <stop offset="1" stopColor="#0A2F8F" />
        </linearGradient>
      </defs>
      {/* 仪表弧 */}
      <path
        d="M3.4 16.2 A9.4 9.4 0 1 1 20.6 16.2"
        stroke={`url(#${gradientId})`}
        strokeWidth={2}
        strokeLinecap="round"
        fill="none"
      />
      {/* 刻度短线：左、中、右 */}
      <path d="M5.2 9.4l1.3 0.9" stroke={`url(#${gradientId})`} strokeWidth={1.4} strokeLinecap="round" />
      <path d="M12 6.6v1.7" stroke={`url(#${gradientId})`} strokeWidth={1.4} strokeLinecap="round" />
      <path d="M18.8 9.4l-1.3 0.9" stroke={`url(#${gradientId})`} strokeWidth={1.4} strokeLinecap="round" />
      {/* 指针：指向右上高区 + 轴心 */}
      <path d="M12 14.6l4.4-5" stroke={`url(#${gradientId})`} strokeWidth={2} strokeLinecap="round" />
      <circle cx={12} cy={14.9} r={1.7} fill={`url(#${gradientId})`} />
    </svg>
  );
}
