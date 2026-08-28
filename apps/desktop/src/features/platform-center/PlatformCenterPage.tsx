import { useCallback, useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AddPlatformDialog } from "./AddPlatformDialog";
import { PlatformSetupForm } from "./PlatformSetupForm";
import { PlatformTabs, type PlatformTabId } from "./PlatformTabs";
import { ProviderHeader } from "./ProviderHeader";
import { ProviderRail } from "./ProviderRail";
import { SourcesView } from "./SourcesView";
import { UsageView } from "./UsageView";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "@/components/ui/EmptyState";
import { fetchPlatformSummaries, ipcErrorMessage, refreshPlatform } from "@/lib/ipc";
import { listen } from "@tauri-apps/api/event";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import type { PlatformCenterTarget } from "@/app/navigation";

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

  if (isLoading) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-q-text-muted">
        正在加载平台数据…
      </div>
    );
  }

  if (!platform) {
    return (
      <div className="flex min-h-0 flex-1">
        <ProviderRail platforms={[]} selectedId={null} onSelect={handleSelect} onAdd={() => setAddOpen(true)} />
        <main className="flex min-h-0 min-w-0 flex-1 flex-col p-5">
          <EmptyState
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
    <div className="flex min-h-0 flex-1">
      <ProviderRail platforms={platforms} selectedId={platform.providerId} onSelect={handleSelect} onAdd={() => setAddOpen(true)} />

      <main className="flex min-h-0 min-w-0 flex-1 flex-col gap-4 p-5">
        <ProviderHeader
          platform={platform}
          refreshing={refreshMutation.isPending}
          onRefresh={() => refreshMutation.mutate(platform.providerId)}
        />
        {refreshMutation.error && (
          <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
            {ipcErrorMessage(refreshMutation.error, "平台刷新失败，请稍后重试。")}
          </p>
        )}
        <PlatformTabs value={tab} onChange={setTab} />
        {tab === "usage" ? (
          <UsageView key={`${platform.providerId}-usage`} platform={platform} />
        ) : (
          <div className="min-h-0 flex-1 space-y-6 overflow-y-auto pr-1">
            <PlatformSetupForm key={`${platform.providerId}-setup`} platformId={platform.providerId} />
            <SourcesView
              key={`${platform.providerId}-sources`}
              platform={platform}
              focusSourceId={focusSourceId}
            />
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
    </div>
  );
}
