import { SourceCard } from "./SourceCard";
import { SourceEditorDrawer } from "./SourceEditorDrawer";
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/Button";
import {
  importLegacyConfig,
  inspectLegacyConfig,
  ipcErrorMessage,
  refreshPlatform,
} from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import type { PlatformSummaryViewModel } from "@/lib/types";

/**
 * 平台中心 / 接入与来源：
 * 一个「来源」对应一个独立获取能力；每个 Source 独立保存凭据状态与验证结果。
 * focusSourceId 用于从总览关注项定位并高亮某个 Source。
 */
export function SourcesView({
  platform,
  focusSourceId,
}: {
  platform: PlatformSummaryViewModel;
  focusSourceId?: string;
}) {
  const queryClient = useQueryClient();
  const [editingSourceId, setEditingSourceId] = useState<string | null>(null);
  const [archiveOldFile, setArchiveOldFile] = useState(true);
  const editingSource = platform.sources.find((source) => source.sourceId === editingSourceId) ?? null;
  const legacyQuery = useQuery({
    queryKey: ["legacy-deepseek-config"],
    queryFn: inspectLegacyConfig,
    enabled: platform.providerId === "deepseek",
  });
  const legacyImport = useMutation({
    mutationFn: () => importLegacyConfig(archiveOldFile),
    onSuccess: (result) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, result.platforms);
      void queryClient.invalidateQueries({ queryKey: ["legacy-deepseek-config"] });
    },
  });
  const refreshMutation = useMutation({
    mutationFn: () => refreshPlatform(platform.providerId),
    onSuccess: (platforms) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, platforms);
    },
  });

  return (
    <div className="space-y-4">
      <p className="text-sm font-medium text-q-text-primary">数据来源</p>
      {legacyQuery.data?.available && (
        <div className="mb-4 rounded-q-card border border-q-warning/30 bg-q-warning-soft p-4">
          <p className="text-sm font-medium text-q-text-primary">检测到旧版 DeepSeek 配置</p>
          <p className="mt-1 text-xs leading-relaxed text-q-text-secondary">
            点击后才会读取并验证其中的 DeepSeek 凭据，成功后写入 Windows 凭据管理器。
          </p>
          <label className="mt-3 flex items-center gap-2 text-xs text-q-text-secondary">
            <input
              type="checkbox"
              checked={archiveOldFile}
              onChange={(event) => setArchiveOldFile(event.target.checked)}
            />
            导入成功后把旧明文配置重命名为可恢复的 .bak 文件
          </label>
          {legacyImport.error && (
            <p className="mt-2 text-xs text-q-danger">
              {legacyImport.error instanceof Error ? legacyImport.error.message : String(legacyImport.error)}
            </p>
          )}
          <Button
            className="mt-3"
            size="sm"
            onClick={() => legacyImport.mutate()}
            disabled={legacyImport.isPending}
          >
            {legacyImport.isPending ? "验证并导入中…" : "导入旧配置"}
          </Button>
        </div>
      )}
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {platform.sources.map((source) => (
          <SourceCard
            key={source.sourceId}
            source={source}
            focused={source.sourceId === focusSourceId}
            apiBaseUrl={platform.apiBaseUrl}
            onEdit={() => setEditingSourceId(source.sourceId)}
            onRefresh={source.sourceType === "local_cli" ? () => refreshMutation.mutate() : undefined}
            refreshing={refreshMutation.isPending}
          />
        ))}
      </div>
      {refreshMutation.error && (
        <p className="mt-3 rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
          {ipcErrorMessage(refreshMutation.error, "本地来源检测失败，请稍后重试。")}
        </p>
      )}
      {platform.sources.length === 0 && (
        <p className="px-1 py-3 text-xs text-q-text-muted">该平台暂未提供可配置来源。</p>
      )}
      <SourceEditorDrawer
        source={editingSource}
        platformId={platform.providerId}
        onClose={() => setEditingSourceId(null)}
      />
    </div>
  );
}
