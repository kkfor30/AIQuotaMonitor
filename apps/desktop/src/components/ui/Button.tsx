import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";

type Variant = "primary" | "secondary" | "ghost";
type Size = "sm" | "md";

const VARIANT_CLASS: Record<Variant, string> = {
  primary:
    "bg-q-primary text-white border border-transparent hover:bg-q-primary-hover shadow-[0_6px_16px_rgba(10,102,255,0.26)]",
  secondary:
    "bg-q-surface-strong text-q-text-primary border border-q-border-strong backdrop-blur hover:border-q-border-selected hover:text-q-primary",
  ghost:
    "bg-transparent text-q-text-secondary border border-transparent hover:bg-q-primary-softer hover:text-q-primary",
};

const SIZE_CLASS: Record<Size, string> = {
  sm: "h-8 px-3 text-[13px] gap-1.5",
  md: "h-9 px-4 text-sm gap-2",
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
  children?: ReactNode;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "primary", size = "md", className = "", children, ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      className={`inline-flex cursor-pointer items-center justify-center rounded-q-control font-medium transition-colors duration-150 disabled:cursor-not-allowed disabled:opacity-50 ${VARIANT_CLASS[variant]} ${SIZE_CLASS[size]} ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
});
