import { SourceCard } from "./SourceCard";
import { SourceEditorDrawer } from "./SourceEditorDrawer";
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/Button";
import {
  addCodexAccount,
  importLegacyConfig,
  inspectLegacyConfig,
  ipcErrorMessage,
  refreshPlatform,
  renameCodexAccount,
  startSourceLogin,
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
  const renameMutation = useMutation({
    mutationFn: ({ sourceId, displayName }: { sourceId: string; displayName: string }) =>
      renameCodexAccount(sourceId, displayName),
    onSuccess: (platforms) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, platforms);
    },
  });
  const addCodexMutation = useMutation({
    mutationFn: async () => {
      const platforms = await addCodexAccount();
      const openai = platforms.find((item) => item.providerId === "openai");
      const extra = openai?.sources
        .filter((source) => source.sourceId.startsWith("openai-codex-extra-"))
        .sort((left, right) => left.sourceId.localeCompare(right.sourceId))
        .at(-1);
      if (extra) {
        await startSourceLogin(extra.sourceId);
        return refreshPlatform(platform.providerId);
      }
      return platforms;
    },
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
            onRefresh={source.supportsCliLogin ? () => refreshMutation.mutate() : undefined}
            onRename={
              source.sourceId.startsWith("openai-codex-extra-")
                ? (displayName) => renameMutation.mutate({ sourceId: source.sourceId, displayName })
                : undefined
            }
            refreshing={refreshMutation.isPending}
            renaming={renameMutation.isPending && renameMutation.variables?.sourceId === source.sourceId}
          />
        ))}
      </div>
      {refreshMutation.error && (
        <p className="mt-3 rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
          {ipcErrorMessage(refreshMutation.error, "本地来源检测失败，请稍后重试。")}
        </p>
      )}
      {renameMutation.error && (
        <p className="mt-3 rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
          {ipcErrorMessage(renameMutation.error, "重命名失败，请稍后重试。")}
        </p>
      )}
      {platform.providerId === "openai" && (
        <div className="mt-4 flex flex-col gap-2">
          <Button
            variant="secondary"
            onClick={() => addCodexMutation.mutate()}
            disabled={addCodexMutation.isPending}
          >
            {addCodexMutation.isPending ? "等待登录另一个账号…" : "添加另一个 ChatGPT 账号"}
          </Button>
          <p className="text-xs leading-relaxed text-q-text-muted">
            本机账号默认直接检测 Codex CLI 登录。额外账号会弹出官方登录，并使用独立目录，不会覆盖 `~/.codex`。
          </p>
          {addCodexMutation.error && (
            <p className="text-xs text-q-danger">{ipcErrorMessage(addCodexMutation.error, "添加额外账号失败。")}</p>
          )}
        </div>
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
