import * as TabsPrimitive from "@radix-ui/react-tabs";
import { cn } from "@/lib/cn";

export type PlatformTabId = "usage" | "sources";

/**
 * 平台中心内部 Tab：额度与用量 / 接入与来源。
 * 切换平台时保留当前 Tab（不支持的 Tab 由上层回退）。
 */
export function PlatformTabs({
  value,
  onChange,
}: {
  value: PlatformTabId;
  onChange: (next: PlatformTabId) => void;
}) {
  return (
    <TabsPrimitive.Root value={value} onValueChange={(next) => onChange(next as PlatformTabId)}>
      <TabsPrimitive.List
        aria-label="平台中心视图"
        className="inline-flex items-center gap-1 rounded-q-control border border-q-border bg-q-surface-muted p-1"
      >
        <TabTrigger value="usage">额度与用量</TabTrigger>
        <TabTrigger value="sources">接入与来源</TabTrigger>
      </TabsPrimitive.List>
    </TabsPrimitive.Root>
  );
}

function TabTrigger({ value, children }: { value: PlatformTabId; children: React.ReactNode }) {
  return (
    <TabsPrimitive.Trigger
      value={value}
      className={cn(
        "cursor-pointer rounded-[7px] px-4 py-1.5 text-[13px] font-medium transition-colors duration-150",
        "text-q-text-secondary hover:text-q-text-primary",
        "data-[state=active]:bg-white data-[state=active]:text-q-primary data-[state=active]:shadow-q-sm",
      )}
    >
      {children}
    </TabsPrimitive.Trigger>
  );
}
