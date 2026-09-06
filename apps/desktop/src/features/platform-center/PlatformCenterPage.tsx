import { useCallback, useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Boxes, Plus, Trash2 } from "lucide-react";
import { AddPlatformDialog } from "./AddPlatformDialog";
import { PlatformTabs, type PlatformTabId } from "./PlatformTabs";
import { ProviderHeader } from "./ProviderHeader";
import { ProviderRail } from "./ProviderRail";
import { SourcesView } from "./SourcesView";
import { UsageView } from "./UsageView";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { PlatformCenterSkeleton } from "@/components/ui/PageSkeletons";
import { fetchPlatformSummaries, ipcErrorMessage, refreshPlatform, removeUserPlatform } from "@/lib/ipc";
import { useContainerWidth } from "@/lib/use-container-width";
import { listen } from "@tauri-apps/api/event";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import type { PlatformCenterTarget } from "@/app/navigation";
import { PlatformMark } from "./ProviderRail";
import { StatusBadge } from "@/components/ui/StatusBadge";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";
import { AGGREGATE_STATUS_META, type PlatformAggregateStatus } from "@/lib/types";

/**
 * 平台中心：平台目录 + 页面头部 + 内部双 Tab。
 * - 切换平台保留当前 Tab
 * - 刷新按钮请求当前平台全部已配置 Source
 * - 支持从总览携带 providerId/sourceId/tab 定位（v5 交互 2）
 */
export function PlatformCenterPage({
  target,
  onTargetConsumed,
}: {
  target?: PlatformCenterTarget | null;
  onTargetConsumed?: () => void;
}) {
  const queryClient = useQueryClient();
  const { data: platforms = [], isLoading } = useQuery({
    queryKey: PLATFORM_SUMMARIES_QUERY_KEY,
    queryFn: fetchPlatformSummaries,
  });

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tab, setTab] = useState<PlatformTabId>("usage");
  const [focusSourceId, setFocusSourceId] = useState<string | undefined>(undefined);
  const [addOpen, setAddOpen] = useState(false);
  const [pendingRemoveId, setPendingRemoveId] = useState<string | null>(null);

  // 消费总览下发的定位目标（一次性）
  useEffect(() => {
    if (!target) return;
    if (platforms.some((p) => p.providerId === target.providerId)) {
      setSelectedId(target.providerId);
      setTab(target.tab);
      setFocusSourceId(target.focusSourceId);
    }
    onTargetConsumed?.();
  }, [target, platforms, onTargetConsumed]);

  const effectiveId = useMemo(() => {
    if (platforms.length === 0) return null;
    if (selectedId && platforms.some((p) => p.providerId === selectedId)) return selectedId;
    // 默认选中第一个已接入（非需配置）的平台，否则第一个
    return (platforms.find((p) => p.aggregateStatus !== "setup_required") ?? platforms[0])
      .providerId;
  }, [platforms, selectedId]);

  const platform = platforms.find((p) => p.providerId === effectiveId) ?? null;

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("source-credential-updated", () => {
      void queryClient.invalidateQueries({ queryKey: PLATFORM_SUMMARIES_QUERY_KEY });
    }).then((nextUnlisten) => {
      unlisten = nextUnlisten;
      if (disposed) nextUnlisten();
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [queryClient]);

  const removeMutation = useMutation({
    mutationFn: (platformId: string) => removeUserPlatform(platformId),
    onSuccess: (nextPlatforms) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, nextPlatforms);
      void queryClient.invalidateQueries({ queryKey: ["platform-catalog"] });
      setPendingRemoveId(null);
      setSelectedId((current) => {
        if (current && nextPlatforms.some((item) => item.providerId === current)) return current;
        return nextPlatforms[0]?.providerId ?? null;
      });
    },
  });

  const refreshMutation = useMutation({
    mutationFn: (providerId: string) => refreshPlatform(providerId),
    onSuccess: (nextPlatforms) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, nextPlatforms);
    },
  });

  const handleSelect = useCallback((providerId: string) => {
    setSelectedId(providerId);
    setFocusSourceId(undefined);
  }, []);

  // 页面内部布局以「扣除全局导航后的实际内容宽度」响应；Rail/选择器/双栏都由此驱动
  const page = useContainerWidth<HTMLDivElement>();

  if (isLoading) {
    return <PlatformCenterSkeleton />;
  }

  if (!platform) {
    return (
      <div ref={page.ref} className="flex min-h-0 min-w-0 flex-1 p-4 pt-2 animate-fade-in">
        {page.mode !== "compact" && (
          <ProviderRail
            mode={page.mode === "medium" ? "icons" : "full"}
            platforms={[]}
            selectedId={null}
            onSelect={handleSelect}
            onAdd={() => setAddOpen(true)}
          />
        )}
        <main className="flex min-h-0 min-w-0 flex-1 flex-col">
          <EmptyState
            icon={Boxes}
            tone="primary"
            title="还没有监控任何平台"
            description="从产品提供的平台列表中添加。大多数平台填写官方 API Key 并验证连接即可；个别没有官方额度接口的能力再使用网页登录。"
            action={<Button onClick={() => setAddOpen(true)}>添加平台</Button>}
          />
        </main>
        <AddPlatformDialog
          open={addOpen}
          onClose={() => setAddOpen(false)}
          onAdded={(ids) => {
            const next = ids[0];
            if (next) {
              setSelectedId(next);
              setTab("sources");
            }
          }}
        />
      </div>
    );
  }

  return (
    <div ref={page.ref} className="flex min-h-0 min-w-0 flex-1 p-4 pt-2 animate-fade-in">
      {page.mode !== "compact" && (
        <ProviderRail
          mode="full"
          platforms={platforms}
          selectedId={platform.providerId}
          onSelect={handleSelect}
          onAdd={() => setAddOpen(true)}
          onRemove={setPendingRemoveId}
        />
      )}

      <main className="flex min-h-0 min-w-0 flex-1 flex-col gap-4">
        {page.mode === "compact" && (
          <PlatformSelectBar
            platforms={platforms}
            selectedId={platform.providerId}
            onSelect={handleSelect}
            onAdd={() => setAddOpen(true)}
            onRemove={() => setPendingRemoveId(platform.providerId)}
          />
        )}
        <ErrorBoundary variant="inline">
          <ProviderHeader
            platform={platform}
            refreshing={refreshMutation.isPending}
            onRefresh={() => refreshMutation.mutate(platform.providerId)}
            onRemove={() => setPendingRemoveId(platform.providerId)}
          />
        </ErrorBoundary>
        {refreshMutation.error && (
          <p className="shrink-0 rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
            {ipcErrorMessage(refreshMutation.error, "平台刷新失败，请稍后重试。")}
          </p>
        )}
        <PlatformTabs value={tab} onChange={setTab} />
        {tab === "usage" ? (
          <div key={`${platform.providerId}-usage-scroll`} className="min-h-0 flex-1 overflow-y-auto pr-1">
            <ErrorBoundary title={`${platform.displayName} 额度与用量展示遇到问题`}>
              <UsageView
                key={`${platform.providerId}-usage`}
                platform={platform}
                onSwitchToSources={() => setTab("sources")}
              />
            </ErrorBoundary>
          </div>
        ) : (
          <div className="min-h-0 flex-1 overflow-y-auto pr-1">
            <ErrorBoundary title={`${platform.displayName} 接入与来源展示遇到问题`}>
              <SourcesView
                key={`${platform.providerId}-sources`}
                platform={platform}
                focusSourceId={focusSourceId}
              />
            </ErrorBoundary>
          </div>
        )}
      </main>
      <AddPlatformDialog
        open={addOpen}
        onClose={() => setAddOpen(false)}
        onAdded={(ids) => {
          const next = ids[0];
          if (next) {
            setSelectedId(next);
            setTab("sources");
          }
        }}
      />
      <RemovePlatformDialog
        platformName={platforms.find((item) => item.providerId === pendingRemoveId)?.displayName ?? ""}
        open={Boolean(pendingRemoveId)}
        pending={removeMutation.isPending}
        error={removeMutation.error ? ipcErrorMessage(removeMutation.error, "移除平台失败。") : null}
        onCancel={() => {
          if (!removeMutation.isPending) setPendingRemoveId(null);
        }}
        onConfirm={() => pendingRemoveId && removeMutation.mutate(pendingRemoveId)}
      />
    </div>
  );
}

function RemovePlatformDialog({
  platformName,
  open,
  pending,
  error,
  onCancel,
  onConfirm,
}: {
  platformName: string;
  open: boolean;
  pending: boolean;
  error: string | null;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-6 backdrop-blur-sm" role="presentation" onMouseDown={onCancel}>
      <div
        className="w-full max-w-md rounded-[18px] border border-q-border bg-q-surface-solid p-5 shadow-q-lg"
        role="dialog"
        aria-label="移除平台"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <p className="text-lg font-semibold text-q-text-primary">移除 {platformName}？</p>
        <p className="mt-2 text-sm leading-relaxed text-q-text-secondary">
          将移除该平台目录项，并删除快照与凭据引用。可稍后重新添加。
        </p>
        {error && <p className="mt-3 text-xs text-q-danger">{error}</p>}
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" onClick={onCancel} disabled={pending}>
            取消
          </Button>
          <Button onClick={onConfirm} disabled={pending}>
            {pending ? "移除中…" : "确认移除"}
          </Button>
        </div>
      </div>
    </div>
  );
}


/**
 * 紧凑模式（内容宽 < 720px）的平台选择器：替代左侧目录，展示当前平台 Logo、名称与
 * 状态，提供切换、添加、移除入口；不产生横向滚动。
 */
function PlatformSelectBar({
  platforms,
  selectedId,
  onSelect,
  onAdd,
  onRemove,
}: {
  platforms: Array<{ providerId: string; displayName: string; aggregateStatus: PlatformAggregateStatus }>;
  selectedId: string;
  onSelect: (providerId: string) => void;
  onAdd: () => void;
  onRemove: () => void;
}) {
  const current = platforms.find((item) => item.providerId === selectedId);
  const statusMeta = current ? AGGREGATE_STATUS_META[current.aggregateStatus] : null;
  return (
    <div className="glass-panel flex shrink-0 flex-wrap items-center gap-2.5 px-3.5 py-2.5">
      {current && <PlatformMark providerId={current.providerId} size={30} />}
      {current && (
        <span className="min-w-0 truncate text-[14px] font-semibold text-q-text-primary">
          {current.displayName}
        </span>
      )}
      {statusMeta && <StatusBadge tone={statusMeta.tone}>{statusMeta.label}</StatusBadge>}
      <select
        value={selectedId}
        onChange={(event) => onSelect(event.target.value)}
        aria-label="切换平台"
        className="h-8 max-w-[180px] cursor-pointer rounded-q-control border border-q-border bg-q-surface px-2 text-[12px] font-medium text-q-text-primary outline-none focus:border-q-primary"
      >
        {platforms.map((item) => (
          <option key={item.providerId} value={item.providerId}>
            {item.displayName}
          </option>
        ))}
      </select>
      <span className="ml-auto flex items-center gap-1.5">
        <button
          type="button"
          onClick={onAdd}
          title="添加平台"
          aria-label="添加平台"
          className="flex h-8 w-8 cursor-pointer items-center justify-center rounded-q-control border border-q-border bg-q-surface-strong text-q-text-secondary transition-colors hover:border-q-border-selected hover:text-q-primary"
        >
          <Plus size={16} aria-hidden />
        </button>
        <button
          type="button"
          onClick={onRemove}
          title="移除当前平台"
          aria-label="移除当前平台"
          className="flex h-8 w-8 cursor-pointer items-center justify-center rounded-q-control border border-q-border bg-q-surface-strong text-q-text-muted transition-colors hover:border-q-danger/50 hover:text-q-danger"
        >
          <Trash2 size={15} aria-hidden />
        </button>
      </span>
    </div>
  );
}
