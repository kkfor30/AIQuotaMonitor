import type { HTMLAttributes } from "react";
import { cn } from "@/lib/cn";

export interface SkeletonProps extends HTMLAttributes<HTMLDivElement> {
  className?: string;
  rounded?: "sm" | "md" | "lg" | "card" | "control" | "pill" | "full";
}

const ROUNDED_CLASSES = {
  sm: "rounded-sm",
  md: "rounded-md",
  lg: "rounded-lg",
  card: "rounded-q-card",
  control: "rounded-q-control",
  pill: "rounded-q-pill",
  full: "rounded-full",
};

/**
 * 骨架屏原子：Aurora Acrylic 半透明微光底座，配合 q-shimmer 流光。
 * 适应浅色（低饱和冰晶）与深色（午夜石墨蓝）双主题。
 */
export function Skeleton({
  className = "",
  rounded = "control",
  ...rest
}: SkeletonProps) {
  return (
    <div
      aria-hidden="true"
      className={cn(
        "skeleton-shimmer-container",
        ROUNDED_CLASSES[rounded],
        className,
      )}
      {...rest}
    />
  );
}

/** 模拟文本行骨架 */
export function SkeletonText({
  lines = 1,
  className = "",
  lastLineWidth = "65%",
}: {
  lines?: number;
  className?: string;
  lastLineWidth?: string;
}) {
  return (
    <div className={cn("flex flex-col gap-2", className)} aria-hidden="true">
      {Array.from({ length: lines }).map((_, index) => {
        const isLast = index === lines - 1;
        return (
          <Skeleton
            key={index}
            rounded="sm"
            className="h-3 w-full"
            style={isLast && lines > 1 ? { width: lastLineWidth } : undefined}
          />
        );
      })}
    </div>
  );
}

/** 模拟圆形徽标/头像骨架 */
export function SkeletonCircle({
  size = 36,
  className = "",
}: {
  size?: number;
  className?: string;
}) {
  return (
    <Skeleton
      rounded="full"
      className={className}
      style={{ width: size, height: size }}
    />
  );
}

/** 模拟玻璃面板/卡片骨架 */
export function SkeletonCard({
  className = "",
  children,
  ...rest
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      aria-hidden="true"
      className={cn(
        "glass-panel relative flex flex-col overflow-hidden p-4",
        className,
      )}
      {...rest}
    >
      {children}
    </div>
  );
}
