import { CapabilityCard } from "./CapabilityCard";
import { RefreshHistory } from "./RefreshHistory";
import { SourceHealthSummary } from "./SourceHealthSummary";
import { UsageTrend } from "./UsageTrend";
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

/** 能力语义分组：与呈现顺序无关的业务类别（纯展示层归类）。 */
function isVisibleQuotaCard(capability: CapabilitySnapshotViewModel): boolean {
  if (!capability.capabilityId.startsWith("quota_window_")) return true;
  return capability.freshness !== "missing" && Boolean(capability.value.primary);
}

function capabilitySection(capabilityId: string): { title: string; order: number } | null {
  if (capabilityId === "usage_trend") return null;
  // plan_level 不再单独成卡：作为套餐徽章挂在账户名旁（悬浮球同款配色）
  if (capabilityId === "plan_level") return null;
  if (capabilityId.startsWith("quota_window")) return { title: "窗口额度", order: 0 };
  if (capabilityId === "credits") return { title: "订阅信息", order: 1 };
  if (capabilityId === "balance" || capabilityId === "month_spend" || capabilityId === "today_spend" || capabilityId === "total_spend") {
    return { title: "余额与消费", order: 2 };
  }
  return { title: "用量统计", order: 3 };
}

function groupedCapabilitySections(
  capabilities: CapabilitySnapshotViewModel[],
): Array<{ title: string; capabilityNames: string[]; capabilities: CapabilitySnapshotViewModel[] }> {
  const sections = new Map<string, { order: number; capabilities: CapabilitySnapshotViewModel[] }>();
  for (const capability of capabilities) {
    const section = capabilitySection(capability.capabilityId);
    if (!section) continue;
    const existing = sections.get(section.title);
    if (existing) {
      existing.capabilities.push(capability);
    } else {
      sections.set(section.title, { order: section.order, capabilities: [capability] });
    }
  }
  return [...sections.entries()]
    .sort((left, right) => left[1].order - right[1].order)
    .map(([title, { capabilities }]) => ({
      title,
      capabilityNames: capabilities.map((capability) => capability.displayName),
      capabilities,
    }));
}

/**
 * 语义分组能力卡列表；账号区块与单账号平台共用。
 * 列数由实际容器宽度决定（auto-fit + 单卡最小可读 250px），不依赖全局 xl/2xl 断点：
 * 放不下两张自动单列，能放两张才双列，极宽时三列。
 */
function CapabilitySections({ capabilities }: { capabilities: CapabilitySnapshotViewModel[] }) {
  return (
    <>
      {groupedCapabilitySections(capabilities).map((section) => (
        <section key={section.title} className="flex min-w-0 flex-col gap-3">
          <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-0.5 px-1">
            <h3 className="text-[14px] font-semibold tracking-tight text-q-text-primary">
              {section.title}
            </h3>
            <span className="break-words text-xs text-q-text-muted">
              {section.capabilityNames.join(" / ")}
            </span>
          </div>
          <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,250px),1fr))] gap-4">
            {section.capabilities.map((capability) => (
              <CapabilityCard
                key={`${capability.sourceId}-${capability.capabilityId}`}
                capability={capability}
              />
            ))}
          </div>
        </section>
      ))}
    </>
  );
}

/** 账号区块头：账号名 + 类型 + 套餐徽章（悬浮球同款配色）+ 账号聚合状态。顺序沿用后端 accounts 顺序。 */
function AccountHeader({ account, plan }: { account: AccountSummaryViewModel; plan?: string | null }) {
  const statusMeta = AGGREGATE_STATUS_META[account.status];
  return (
    <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1.5 px-1">
      <h3 className="text-[15px] font-semibold tracking-tight text-q-text-primary">{account.displayName}</h3>
      <span
        className={`rounded-q-pill px-2 py-0.5 text-[11px] font-medium ${
          account.kind === "local" ? "bg-q-primary-softer text-q-primary" : "bg-q-neutral-soft text-q-neutral"
        }`}
      >
        {ACCOUNT_KIND_LABEL[account.kind]}
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

/**
 * 平台中心 / 额度与用量（Apple Glass V6/V7）：
 * 多账号平台先按账号分组（本机 → 默认 → 额外，顺序由后端决定），账号内再语义分组；
 * 单账号平台保持原布局。Capability 通过 accountId 归属账号，跨账号不合并。
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

  const trendCapability = platform.capabilities.find(
    (capability) => capability.capabilityId === "usage_trend",
  );
  const cardCapabilities = platform.capabilities.filter(
    (capability) => capability.capabilityId !== "usage_trend" && isVisibleQuotaCard(capability),
  );
  const multiAccount = platform.accounts.length > 1;
  const accountSections = platform.accounts.map((account) => {
    const capabilities = cardCapabilities.filter((capability) => capability.accountId === account.accountId);
    return {
      account,
      plan: planOf(capabilities),
      capabilities: capabilities.filter((capability) => capability.capabilityId !== "plan_level"),
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
              {capabilities.length > 0 ? (
                <CapabilitySections capabilities={capabilities} />
              ) : (
                <p className="px-1 text-xs text-q-text-muted">该账号暂无额度数据。</p>
              )}
            </section>
          ))}
        </div>
      ) : (
        // 单账号平台同样展示账户头（套餐徽章挂在账户名旁，订阅计划不再单独成卡）
        <div className="flex min-w-0 flex-col gap-3">
          <AccountHeader account={platform.accounts[0]} plan={planOf(cardCapabilities)} />
          <CapabilitySections
            capabilities={cardCapabilities.filter((capability) => capability.capabilityId !== "plan_level")}
          />
        </div>
      )}
      {trendCapability && trendCapability.trend.length > 0 && (
        <UsageTrend capability={trendCapability} />
      )}
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
