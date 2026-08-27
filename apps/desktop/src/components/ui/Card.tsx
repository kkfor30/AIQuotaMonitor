import type { HTMLAttributes, ReactNode } from "react";

/** 玻璃卡片容器：统一的表面、描边、阴影层级。 */
export function Card({
  className = "",
  children,
  ...rest
}: HTMLAttributes<HTMLDivElement> & { children?: ReactNode }) {
  return (
    <div className={`glass-panel p-5 ${className}`} {...rest}>
      {children}
    </div>
  );
}
