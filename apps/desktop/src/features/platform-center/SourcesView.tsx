import { SourceCard } from "./SourceCard";
import { SourceEditorDrawer } from "./SourceEditorDrawer";
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/Button";
import { StatusBadge } from "@/components/ui/StatusBadge";
import {
  addPlatformAccount,
  importLegacyConfig,
  inspectLegacyConfig,
  ipcErrorMessage,
  refreshPlatform,
  removePlatformAccount,
  renamePlatformAccount,
  startSourceLogin,
} from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import {
  AGGREGATE_STATUS_META,
  type AccountKind,
  type AccountSummaryViewModel,
  type PlatformSummaryViewModel,
} from "@/lib/types";

const ACCOUNT_KIND_LABEL: Record<AccountKind, string> = {
  local: "本机",
  default: "默认",
  additional: "额外",
};

/**
 * 平台中心 / 接入与来源：
 * 按账号分组展示 Source；账号顺序沿用后端（local → default → additional），
 * 前端不从 Source ID 推断账号归属。仅 additional 账号可重命名与移除。
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
  const [removingAccountId, setRemovingAccountId] = useState<string | null>(null);
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
    mutationFn: ({ accountId, displayName }: { accountId: string; displayName: string }) =>
      renamePlatformAccount(accountId, displayName),
    onSuccess: (platforms) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, platforms);
    },
  });
  const removeAccountMutation = useMutation({
    mutationFn: (accountId: string) => removePlatformAccount(accountId),
    onSuccess: (platforms) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, platforms);
      setRemovingAccountId(null);
    },
  });
  const addAccountMutation = useMutation({
    mutationFn: async () => {
      const result = await addPlatformAccount(platform.providerId);
      const source = result.platforms
        .find((item) => item.providerId === platform.providerId)
        ?.sources.find((candidate) => result.sourceIds.includes(candidate.sourceId));
      // GPT 额外账号直接走独立 Codex 登录；单网页会话来源打开对应登录窗。
      if (platform.providerId === "openai" || (result.sourceIds.length === 1 && source?.supportsInteractiveLogin)) {
        await startSourceLogin(result.sourceIds[0]);
        return refreshPlatform(platform.providerId);
      }
      return result.platforms;
    },
    onSuccess: (platforms) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, platforms);
    },
  });

  const accountsById = new Map(platform.accounts.map((account) => [account.accountId, account]));
  const removingAccount =
    removingAccountId !== null ? (accountsById.get(removingAccountId) ?? null) : null;

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
      <div className="flex flex-col gap-5">
        {platform.accounts.map((account) => (
          <AccountSourcesSection
            key={account.accountId}
            account={account}
            platform={platform}
            focusSourceId={focusSourceId}
            onRemoveRequest={() => setRemovingAccountId(account.accountId)}
            onEdit={(sourceId) => setEditingSourceId(sourceId)}
            onRefresh={() => refreshMutation.mutate()}
            refreshing={refreshMutation.isPending}
            onRename={(displayName) =>
              renameMutation.mutate({ accountId: account.accountId, displayName })}
            renaming={renameMutation.isPending && renameMutation.variables?.accountId === account.accountId}
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
      {removeAccountMutation.error && (
        <p className="mt-3 rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
          {ipcErrorMessage(removeAccountMutation.error, "移除账号失败，请稍后重试。")}
        </p>
      )}
      {platform.supportsMultipleAccounts && (
        <div className="mt-1 flex flex-col gap-2">
          <Button
            variant="secondary"
            onClick={() => addAccountMutation.mutate()}
            disabled={addAccountMutation.isPending}
          >
            {addAccountMutation.isPending
              ? "等待登录另一个账号…"
              : platform.providerId === "openai"
                ? "添加另一个 ChatGPT 账号"
                : "再添加一个账号"}
          </Button>
          <p className="text-xs leading-relaxed text-q-text-muted">
            {platform.providerId === "openai"
              ? "本机账号默认直接检测 Codex CLI 登录。额外账号会弹出官方登录，并使用独立目录，不会覆盖 `~/.codex`。"
              : "新账号会创建独立来源，凭据与额度快照互不影响。"}
          </p>
          {addAccountMutation.error && (
            <p className="text-xs text-q-danger">{ipcErrorMessage(addAccountMutation.error, "添加账号失败。")}</p>
          )}
        </div>
      )}
      {platform.sources.length === 0 && (
        <p className="px-1 py-3 text-xs text-q-text-muted">该平台暂未提供可配置来源。</p>
      )}
      {removingAccount && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-6 backdrop-blur-sm" role="presentation" onMouseDown={() => setRemovingAccountId(null)}>
          <div
            className="w-full max-w-sm rounded-[18px] border border-q-border bg-q-surface-solid p-5 shadow-q-lg"
            role="dialog"
            aria-modal="true"
            aria-label={`移除 ${removingAccount.displayName}`}
            onMouseDown={(event) => event.stopPropagation()}
          >
            <p className="text-sm font-semibold text-q-text-primary">移除账号</p>
            <p className="mt-2 text-xs leading-relaxed text-q-text-secondary">
              将删除「{removingAccount.displayName}」的来源、独立登录目录和已缓存额度，其他账号不受影响。确定继续？
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" size="sm" onClick={() => setRemovingAccountId(null)} disabled={removeAccountMutation.isPending}>
                取消
              </Button>
              <Button size="sm" onClick={() => removeAccountMutation.mutate(removingAccount.accountId)} disabled={removeAccountMutation.isPending}>
                {removeAccountMutation.isPending ? "移除中…" : "确认移除"}
              </Button>
            </div>
          </div>
        </div>
      )}
      <SourceEditorDrawer
        source={editingSource}
        platformId={platform.providerId}
        onClose={() => setEditingSourceId(null)}
      />
    </div>
  );
}

/** 单个账号的来源分组：账号标题 + 状态 + 该账号自己的 Source 卡片。 */
function AccountSourcesSection({
  account,
  platform,
  focusSourceId,
  onEdit,
  onRefresh,
  onRename,
  renaming,
  refreshing,
  onRemoveRequest,
}: {
  account: AccountSummaryViewModel;
  platform: PlatformSummaryViewModel;
  focusSourceId?: string;
  onEdit: (sourceId: string) => void;
  onRefresh: () => void;
  onRename: (displayName: string) => void;
  renaming: boolean;
  refreshing: boolean;
  onRemoveRequest: () => void;
}) {
  const sources = platform.sources.filter((source) => source.accountId === account.accountId);
  if (sources.length === 0) return null;
  const statusMeta = AGGREGATE_STATUS_META[account.status];

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1.5 px-1">
        <h3 className="text-[14px] font-semibold tracking-tight text-q-text-primary">{account.displayName}</h3>
        <span
          className={`rounded-q-pill px-2 py-0.5 text-[11px] font-medium ${
            account.kind === "local" ? "bg-q-primary-softer text-q-primary" : "bg-q-neutral-soft text-q-neutral"
          }`}
        >
          {ACCOUNT_KIND_LABEL[account.kind]}
        </span>
        <StatusBadge tone={statusMeta.tone}>{statusMeta.label}</StatusBadge>
        {account.canRemove && (
          <Button
            variant="ghost"
            size="sm"
            className="ml-auto text-q-danger hover:text-q-danger"
            onClick={onRemoveRequest}
          >
            移除账号
          </Button>
        )}
      </div>
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {sources.map((source) => (
          <SourceCard
            key={source.sourceId}
            source={source}
            focused={source.sourceId === focusSourceId}
            apiBaseUrl={platform.apiBaseUrl}
            onEdit={() => onEdit(source.sourceId)}
            onRefresh={source.supportsCliLogin ? onRefresh : undefined}
            onRename={account.canRename ? onRename : undefined}
            refreshing={refreshing}
            renaming={renaming}
          />
        ))}
      </div>
    </section>
  );
}
