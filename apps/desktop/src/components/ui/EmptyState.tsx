import type { ReactNode } from "react";

/** 空态容器：未配置平台 / 无数据场景的统一占位。 */
export function EmptyState({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="glass-panel flex flex-1 flex-col items-center justify-center gap-3 p-10 text-center">
      <div
        aria-hidden
        className="flex h-12 w-12 items-center justify-center rounded-full bg-q-primary-soft"
      >
        <span className="text-xl">🔌</span>
      </div>
      <p className="text-[15px] font-medium text-q-text-primary">{title}</p>
      {description && <p className="max-w-sm text-[13px] leading-relaxed text-q-text-secondary">{description}</p>}
      {action}
    </div>
  );
}
