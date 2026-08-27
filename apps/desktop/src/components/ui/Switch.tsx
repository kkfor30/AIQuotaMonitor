import { cn } from "@/lib/cn";

/** 无障碍开关（不引入 Radix Switch，保持依赖最小）。 */
export function Switch({
  checked,
  onCheckedChange,
  disabled = false,
  label,
}: {
  checked: boolean;
  onCheckedChange: (next: boolean) => void;
  disabled?: boolean;
  label?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(
        "relative h-6 w-11 cursor-pointer rounded-full border transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-50",
        checked
          ? "border-transparent bg-q-primary"
          : "border-q-border-strong bg-q-neutral-soft",
      )}
    >
      <span
        aria-hidden
        className={cn(
          "absolute top-1/2 -translate-y-1/2 rounded-full bg-white shadow-q-sm transition-all duration-200",
          checked ? "left-[calc(100%-21px)]" : "left-[3px]",
        )}
        style={{ height: 18, width: 18 }}
      />
    </button>
  );
}
