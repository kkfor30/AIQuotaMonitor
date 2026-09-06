import { CapabilityDashboard } from "./CapabilityDashboard";
import { RefreshHistory } from "./RefreshHistory";
import { SourceHealthSummary } from "./SourceHealthSummary";
import { EmptyState } from "@/components/ui/EmptyState";
import { StatusBadge } from "@/components/ui/StatusBadge";
import { cn } from "@/lib/cn";
import { useContainerWidth } from "@/lib/use-container-width";
import type {
  AccountKind,
  AccountSummaryViewModel,
  CapabilitySnapshotViewModel,
  PlatformSummaryViewModel,
} from "@/lib/types";
import { AGGREGATE_STATUS_META } from "@/lib/types";

const ACCOUNT_KIND_LABEL: Record<AccountKind, string> = {
  local: "本机",
  default: "默认",
  additional: "额外",
};

/** 套餐徽章归类（与悬浮球同款语义配色）：pro/plus/lite/free，其余中性。 */
function planKey(plan: string): "pro" | "plus" | "lite" | "free" | "other" {
  const key = plan.trim().toLowerCase();
  return key === "plus" || key === "pro" || key === "lite" || key === "free" ? key : "other";
}

function planOf(capabilities: CapabilitySnapshotViewModel[]): string | null {
  return capabilities.find((capability) => capability.capabilityId === "plan_level")?.value.primary ?? null;
}

/**
 * 窗口能力可见性：从未成功获取过的窗口不展示（官方拿掉或尚未返回的窗口不写「暂不可用」），
 * 其余能力全部参与组合渲染。
 */
function isVisibleWindow(capability: CapabilitySnapshotViewModel): boolean {
  if (!capability.capabilityId.startsWith("quota_window_")) return true;
  return capability.freshness !== "missing" && Boolean(capability.value.primary);
}

/**
 * 平台中心 / 额度与用量：全部账号统一走 CapabilityDashboard 按 Capability 类型组合渲染，
 * 不按 providerId 选择布局（DeepSeek 与其他平台同构）。plan_level 由账号头渲染为套餐徽章。
 * 每个 Account 独立分组，账号头展示：账户名 + 类型 + 套餐 + 聚合状态。
 * 布局由本组件实际宽度驱动（不依赖全局视口断点）：
 * - wide（≥960）：主内容 + 最近刷新记录双栏；历史栏 sticky 跟随滚动容器，不被拉伸；
 * - medium/compact：单栏，刷新记录折叠卡放在来源状态之后、账号额度之前。
 * 滚动由外层 TabContent 统一承担，本组件自身不产生第二个滚动区。
 */
export function UsageView({ platform }: { platform: PlatformSummaryViewModel }) {
  const { ref, mode } = useContainerWidth<HTMLDivElement>();
  const wide = mode === "wide";

  if (platform.aggregateStatus === "setup_required") {
    return (
      <EmptyState
        title={`${platform.displayName} 尚未接入`}
        description="配置数据来源后即可在此查看额度与用量。切换到「接入与来源」开始配置。"
      />
    );
  }

  const accounts = platform.accounts ?? [];
  if (accounts.length === 0) {
    return (
      <EmptyState
        title={`${platform.displayName} 暂无可用账户`}
        description="该平台尚未初始化账户配置，请切换到「接入与来源」配置数据来源。"
      />
    );
  }

  const allCapabilities = platform.capabilities ?? [];
  const cardCapabilities = allCapabilities.filter(isVisibleWindow);
  const multiAccount = accounts.length > 1;
  const accountSections = accounts.map((account) => {
    const caps = cardCapabilities.filter((capability) => capability.accountId === account.accountId);
    return {
      account,
      plan: planOf(caps),
      // usage_trend 等趋势能力随账号能力一起进入组合渲染；plan_level 只作徽章
      capabilities: caps.filter((capability) => capability.capabilityId !== "plan_level"),
    };
  });

  const mainContent = (
    <div className="flex min-w-0 flex-col gap-4">
      <SourceHealthSummary platform={platform} />
      {!wide && <RefreshHistory platform={platform} variant="inline" />}
      {multiAccount ? (
        <div className="flex flex-col gap-6">
          {accountSections.map(({ account, plan, capabilities }) => (
            <section key={account.accountId} className="flex min-w-0 flex-col gap-3">
              <AccountHeader account={account} plan={plan} />
              <CapabilityDashboard capabilities={capabilities} wide={wide} />
            </section>
          ))}
        </div>
      ) : accountSections[0] ? (
        // 单账号平台同样展示账户头（套餐徽章挂在账户名旁，订阅计划不单独成卡）
        <div className="flex min-w-0 flex-col gap-3">
          <AccountHeader account={accountSections[0].account} plan={accountSections[0].plan} />
          <CapabilityDashboard capabilities={accountSections[0].capabilities} wide={wide} />
        </div>
      ) : null}
    </div>
  );

  return (
    <div
      ref={ref}
      className={cn(
        "min-w-0",
        wide
          ? "grid grid-cols-[minmax(0,1fr)_clamp(260px,28%,320px)] items-start gap-4"
          : "flex flex-col gap-4",
      )}
    >
      {mainContent}
      {wide && <RefreshHistory platform={platform} variant="panel" />}
    </div>
  );
}

/** 账号区块头：账号名 + 类型 + 套餐徽章（悬浮球同款配色）+ 账号聚合状态。顺序沿用后端 accounts 顺序。 */
function AccountHeader({ account, plan }: { account: AccountSummaryViewModel; plan?: string | null }) {
  if (!account) return null;
  const statusMeta = (account.status && AGGREGATE_STATUS_META[account.status]) ?? {
    label: "未知",
    icon: "settings" as const,
    tone: "neutral" as const,
  };
  const kindLabel = (account.kind && ACCOUNT_KIND_LABEL[account.kind]) ?? "默认";
  return (
    <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1.5 px-1">
      <h3 className="text-[15px] font-semibold tracking-tight text-q-text-primary">{account.displayName || "默认账户"}</h3>
      <span
        className={`rounded-q-pill px-2 py-0.5 text-[11px] font-medium ${
          account.kind === "local" ? "bg-q-primary-softer text-q-primary" : "bg-q-neutral-soft text-q-neutral"
        }`}
      >
        {kindLabel}
      </span>
      {plan && (
        <span className="plan-chip" data-plan={planKey(plan)} title="订阅计划">
          {plan}
        </span>
      )}
      <StatusBadge tone={statusMeta.tone}>{statusMeta.label}</StatusBadge>
    </div>
  );
}
