import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, Globe, KeyRound, Pencil, ShieldCheck, Terminal } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/Button";
import {
  addPlatformAccount,
  importLegacyConfig,
  inspectLegacyConfig,
  ipcErrorMessage,
  openExternalUrl,
  refreshPlatform,
  removePlatformAccount,
  renamePlatformAccount,
  startSourceLogin,
} from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import { cn } from "@/lib/cn";
import { formatDateTime } from "@/lib/format";
import {
  AGGREGATE_STATUS_META,
  type AccountKind,
  type AccountSummaryViewModel,
  type PlatformSummaryViewModel,
  type SourceSummaryViewModel,
  type SourceType,
} from "@/lib/types";
import { SourceEditorDrawer } from "./SourceEditorDrawer";

const ACCOUNT_KIND_LABEL: Record<AccountKind, string> = {
  local: "本机",
  default: "默认",
  additional: "额外",
};

/** 接入类型列展示文案（V7：网页会话在列表中表述为「网页登录」） */
const SOURCE_ACCESS_LABEL: Record<SourceType, string> = {
  api_key: "API Key",
  web_session: "网页登录",
  local_cli: "本地 CLI",
  oauth: "OAuth 订阅",
};

const SOURCE_TYPE_ICON: Record<string, LucideIcon> = {
  api_key: KeyRound,
  web_session: Globe,
  local_cli: Terminal,
  oauth: ShieldCheck,
};

/** 来源行操作（V7 矩阵）：CLI 可检测刷新，Codex CLI 另可重触发官方登录；API Key / Web 只编辑。 */
function sourceAction(source: SourceSummaryViewModel): "edit" | "refresh" | null {
  if (source.supportsCliLogin || source.sourceType === "local_cli") return "refresh";
  if (source.credentialInput || source.supportsInteractiveLogin) return "edit";
  return null;
}

/**
 * 平台中心 / 接入与来源（V7 设计稿 02）：
 * 顶部只读接入概览（D010 要求的官网与请求地址保持可见），账号/来源区是唯一配置入口。
 * 账号别名行内编辑（本机/默认/额外均可改名）；来源为紧凑行，动作符合单一入口矩阵。
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
  // Codex CLI 来源重新触发官方登录（中断后补救 / 已配置账号更换登录），完成后刷新平台
  const reloginMutation = useMutation({
    mutationFn: async (sourceId: string) => {
      await startSourceLogin(sourceId);
      return refreshPlatform(platform.providerId);
    },
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
    <div className="space-y-4 pb-4">
      <AccessOverview platform={platform} />

      {legacyQuery.data?.available && (
        <div className="rounded-q-card border border-q-warning/30 bg-q-warning-soft p-4">
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
        {platform.accounts.map((account) => {
          const sources = platform.sources.filter((source) => source.accountId === account.accountId);
          if (sources.length === 0) return null;
          const statusMeta = AGGREGATE_STATUS_META[account.status];
          return (
            <section key={account.accountId} className="glass-panel flex flex-col gap-1 p-4">
              <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1.5">
                <AccountNameEditor
                  account={account}
                  renaming={renameMutation.isPending && renameMutation.variables?.accountId === account.accountId}
                  onRename={(displayName) =>
                    renameMutation.mutate({ accountId: account.accountId, displayName })
                  }
                />
                <span className="rounded-q-pill bg-q-neutral-soft px-2 py-0.5 text-[11px] font-medium text-q-text-secondary">
                  {ACCOUNT_KIND_LABEL[account.kind]}
                </span>
                <span
                  className={cn(
                    "inline-flex items-center gap-1.5 rounded-q-pill px-2.5 py-[3px] text-xs font-medium",
                    statusMeta.tone === "success"
                      ? "bg-q-success-soft text-q-success-strong"
                      : statusMeta.tone === "warning"
                        ? "bg-q-warning-soft text-q-warning"
                        : statusMeta.tone === "danger"
                          ? "bg-q-danger-soft text-q-danger"
                          : "bg-q-neutral-soft text-q-neutral",
                  )}
                >
                  <span
                    aria-hidden
                    className={cn(
                      "h-1.5 w-1.5 rounded-full",
                      statusMeta.tone === "success"
                        ? "bg-q-success"
                        : statusMeta.tone === "warning"
                          ? "bg-q-warning"
                          : statusMeta.tone === "danger"
                            ? "bg-q-danger"
                            : "bg-q-neutral",
                    )}
                  />
                  {statusMeta.label}
                </span>
                {account.canRemove && (
                  <Button
                    variant="ghost"
                    size="sm"
                    className="ml-auto text-q-danger hover:text-q-danger"
                    onClick={() => setRemovingAccountId(account.accountId)}
                  >
                    移除账号
                  </Button>
                )}
              </div>

              {/* 来源紧凑行（V7）：来源名 / 接入类型 / 凭据状态 / 能力覆盖 / 最后成功 / 唯一操作 */}
              <div className="mt-1 overflow-x-auto">
                <div className="min-w-[640px]">
                  <div className="grid grid-cols-[minmax(0,1.5fr)_84px_72px_78px_88px_150px] items-center gap-x-3 border-b border-q-border px-3 pb-1.5 text-[11px] font-medium text-q-text-muted">
                    <span>来源</span>
                    <span>接入类型</span>
                    <span>凭据状态</span>
                    <span>能力覆盖</span>
                    <span>最后成功</span>
                    <span className="justify-self-end">操作</span>
                  </div>
                  {sources.map((source) => (
                    <SourceRow
                      key={source.sourceId}
                      source={source}
                      focused={source.sourceId === focusSourceId}
                      refreshing={refreshMutation.isPending}
                      reloginPending={reloginMutation.isPending && reloginMutation.variables === source.sourceId}
                      onEdit={() => setEditingSourceId(source.sourceId)}
                      onRefresh={() => refreshMutation.mutate()}
                      onRelogin={() => reloginMutation.mutate(source.sourceId)}
                    />
                  ))}
                </div>
              </div>
            </section>
          );
        })}
      </div>

      {refreshMutation.error && (
        <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
          {ipcErrorMessage(refreshMutation.error, "本地来源检测失败，请稍后重试。")}
        </p>
      )}
      {renameMutation.error && (
        <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
          {ipcErrorMessage(renameMutation.error, "重命名失败，请稍后重试。")}
        </p>
      )}
      {removeAccountMutation.error && (
        <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
          {ipcErrorMessage(removeAccountMutation.error, "移除账号失败，请稍后重试。")}
        </p>
      )}

      {reloginMutation.error && (
        <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
          {ipcErrorMessage(reloginMutation.error, "无法启动官方登录，请稍后重试。")}
        </p>
      )}

      {platform.supportsMultipleAccounts && (
        <div className="flex flex-col gap-2">
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

/** 顶部只读接入概览（V7）：官方平台 / 账户与来源 / API 请求地址 / 最后验证 / 官网入口。 */
function AccessOverview({ platform }: { platform: PlatformSummaryViewModel }) {
  const lastValidatedAt = platform.sources.reduce<number | null>((max, source) => {
    if (source.lastValidatedAt === null) return max;
    return max === null || source.lastValidatedAt > max ? source.lastValidatedAt : max;
  }, null);
  return (
    <section className="glass-panel flex flex-col gap-3 p-4">
      <h2 className="text-[14px] font-semibold tracking-tight text-q-text-primary">接入概览</h2>
      <div className="grid grid-cols-2 items-center gap-x-6 gap-y-3 md:grid-cols-[1.1fr_1.3fr_1.3fr_0.9fr_auto]">
        <OverviewField label="官方平台" value={platform.displayName} />
        <OverviewField
          label="账户与来源"
          value={`${platform.accounts.length} 个账户 · ${platform.sources.length} 个来源`}
        />
        <OverviewField
          label="API 请求地址"
          value={platform.apiBaseUrl?.trim() ? platform.apiBaseUrl : "官方默认地址"}
          title={platform.apiBaseUrl ?? undefined}
        />
        <OverviewField label="最后验证" value={formatDateTime(lastValidatedAt)} />
        {platform.officialUrl && (
          <button
            type="button"
            onClick={() => void openExternalUrl(platform.officialUrl!)}
            className="inline-flex cursor-pointer items-center gap-1 self-start justify-self-start text-[13px] font-medium text-q-primary hover:text-q-primary-hover md:justify-self-end"
          >
            官方页面
            <ExternalLink size={13} aria-hidden />
          </button>
        )}
      </div>
    </section>
  );
}

function OverviewField({
  label,
  value,
  title,
}: {
  label: string;
  value: string;
  title?: string;
}) {
  return (
    <div className="min-w-0">
      <p className="text-[11px] text-q-text-muted">{label}</p>
      <p className="mt-0.5 truncate text-[13px] font-medium text-q-text-primary" title={title ?? value}>
        {value}
      </p>
    </div>
  );
}

/** 账号别名的铅笔行内编辑：Enter/失焦提交，Esc 取消；本机/默认/额外均可改名。 */
function AccountNameEditor({
  account,
  renaming,
  onRename,
}: {
  account: AccountSummaryViewModel;
  renaming: boolean;
  onRename: (displayName: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(account.displayName);
  useEffect(() => {
    if (!editing) setDraft(account.displayName);
  }, [account.displayName, editing]);

  const commit = () => {
    setEditing(false);
    const next = draft.trim();
    if (!account.canRename || renaming || !next || next === account.displayName) return;
    onRename(next);
  };

  if (!editing) {
    return (
      <span className="flex min-w-0 items-center gap-1">
        <h3 className="truncate text-[15px] font-semibold tracking-tight text-q-text-primary">
          {account.displayName}
        </h3>
        {account.canRename && (
          <button
            type="button"
            aria-label={`重命名 ${account.displayName}`}
            title="重命名账号"
            onClick={() => setEditing(true)}
            className="flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-md text-q-text-muted transition-colors hover:bg-q-primary-softer hover:text-q-primary"
          >
            <Pencil size={13} aria-hidden />
          </button>
        )}
      </span>
    );
  }
  return (
    <input
      autoFocus
      value={draft}
      onChange={(event) => setDraft(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === "Enter") event.currentTarget.blur();
        if (event.key === "Escape") setEditing(false);
      }}
      onBlur={commit}
      disabled={renaming}
      maxLength={40}
      aria-label="账号名称"
      className="h-7 w-44 rounded-md border border-q-border bg-q-surface px-2 text-sm font-semibold text-q-text-primary outline-none focus:border-q-primary"
    />
  );
}

/** 单条来源紧凑行：凭据只展示配置状态，错误以小字跟随来源名；focused 时高亮（总览定位）。 */
function SourceRow({
  source,
  focused,
  refreshing,
  reloginPending,
  onEdit,
  onRefresh,
  onRelogin,
}: {
  source: SourceSummaryViewModel;
  focused: boolean;
  refreshing: boolean;
  reloginPending: boolean;
  onEdit: () => void;
  onRefresh: () => void;
  onRelogin: () => void;
}) {
  const rowRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (focused) rowRef.current?.scrollIntoView({ block: "nearest" });
  }, [focused]);
  const Icon = SOURCE_TYPE_ICON[source.sourceType] ?? KeyRound;
  const action = sourceAction(source);
  // Codex CLI 来源可重触发官方登录：登录中断的额外账号由此补救，已配置账号可更换登录
  const canRelogin = source.supportsCliLogin;

  return (
    <div
      ref={rowRef}
      data-focused={focused || undefined}
      className={cn(
        "grid grid-cols-[minmax(0,1.5fr)_84px_72px_78px_88px_150px] items-center gap-x-3 border-b border-q-border/50 px-3 py-2.5 text-xs last:border-b-0",
        focused && "bg-q-primary-softer/70 ring-1 ring-inset ring-q-primary/40",
      )}
    >
      <span className="flex min-w-0 items-center gap-2.5">
        <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-q-border bg-q-surface-strong text-q-primary">
          <Icon size={14} aria-hidden />
        </span>
        <span className="flex min-w-0 flex-col">
          <span className="truncate font-medium text-q-text-primary" title={source.displayName}>
            {source.displayName}
          </span>
          {source.state === "error" && (
            <span className="truncate text-[10px] leading-3.5 text-q-danger" title={source.errorMessage ?? "刷新失败"}>
              {source.errorMessage ?? "刷新失败"}
            </span>
          )}
          {source.state === "auth_required" && (
            <span className="text-[10px] leading-3.5 text-q-text-muted">凭据待配置</span>
          )}
        </span>
      </span>
      <span className="text-q-text-secondary">{SOURCE_ACCESS_LABEL[source.sourceType]}</span>
      <span className={source.credentialConfigured ? "font-medium text-q-success-strong" : "text-q-text-muted"}>
        {source.credentialConfigured ? "已配置" : "未配置"}
      </span>
      <span className="tabular-nums text-q-text-secondary">
        {source.capabilityIds.length > 0 ? `${source.capabilityIds.length} 项能力` : "—"}
      </span>
      <span className="tabular-nums text-q-text-secondary">{formatDateTime(source.lastSuccessAt)}</span>
      <span className="justify-self-end text-nowrap">
        {action === "refresh" && (
          <button
            type="button"
            onClick={onRefresh}
            disabled={refreshing}
            className="cursor-pointer text-xs font-medium text-q-primary transition-colors hover:text-q-primary-hover disabled:opacity-60"
          >
            {refreshing ? "检测中…" : "检测并刷新"}
          </button>
        )}
        {action === "refresh" && canRelogin && (
          <button
            type="button"
            onClick={onRelogin}
            disabled={reloginPending}
            className="ml-2 cursor-pointer text-xs font-medium text-q-text-secondary transition-colors hover:text-q-primary disabled:opacity-60"
          >
            {reloginPending ? "等待登录…" : source.credentialConfigured ? "更换登录" : "重新登录"}
          </button>
        )}
        {action === "edit" && (
          <button
            type="button"
            onClick={onEdit}
            className="cursor-pointer text-xs font-medium text-q-primary transition-colors hover:text-q-primary-hover"
          >
            编辑
          </button>
        )}
        {action === null && <span className="text-q-text-muted">—</span>}
      </span>
    </div>
  );
}
