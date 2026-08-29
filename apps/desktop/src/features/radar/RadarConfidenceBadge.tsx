import { radarConfidenceLabel } from "@/features/hoverbar/hoverbar-state";

/** 统一的把握度徽章：高=绿、中=橙、低=中性，悬浮条/详情/主窗口共用。 */
export function RadarConfidenceBadge({ confidence }: { confidence: string | null | undefined }) {
  if (!confidence) return null;
  const level = confidence === "high" || confidence === "medium" || confidence === "low" ? confidence : "low";
  return (
    <span className="radar-confidence-badge" data-level={level}>
      {radarConfidenceLabel(confidence)}把握
    </span>
  );
}
