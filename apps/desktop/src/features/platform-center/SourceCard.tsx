import { useEffect, useState } from "react";
import { KeyRound, Globe, Terminal, ShieldCheck } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { StatusBadge } from "@/components/ui/StatusBadge";
import { formatDateTime } from "@/lib/format";
import {
  ACCESS_MODE_LABEL,
  SOURCE_STATE_META,
  SOURCE_TYPE_LABEL,
  type SourceAccessMode,
  type SourceSummaryViewModel,
} from "@/lib/types";

const SOURCE_TYPE_ICON: Record<string, LucideIcon> = {
  api_key: KeyRound,
  web_session: Globe,
  local_cli: Terminal,
  oauth: ShieldCheck,
};

/**
 * 单个来源卡片：来源类型、凭据状态、最后验证/成功、当前错误与能力覆盖。
 * 凭据只展示配置状态，永不展示明文。focused 时高亮边框（总览定位）。
 */
export function SourceCard({
  source,
  focused = false,
  apiBaseUrl,
  onEdit,
  onRefresh,
  onRename,
  refreshing = false,
  renaming = false,
}: {
  source: SourceSummaryViewModel;
  focused?: boolean;
  apiBaseUrl?: string | null;
  onEdit: () => void;
  onRefresh?: () => void;
  onRename?: (displayName: string) => void;
  refreshing?: boolean;
  renaming?: boolean;
}) {
  const Icon = SOURCE_TYPE_ICON[source.sourceType] ?? KeyRound;
  const meta = SOURCE_STATE_META[source.state];
  const [draftName, setDraftName] = useState(source.displayName);
  useEffect(() => {
    setDraftName(source.displayName);
  }, [source.displayName]);
  const commitRename = () => {
    const next = draftName.trim();
    if (!onRename) return;
    if (!next || next === source.displayName) {
      setDraftName(source.displayName);
      return;
    }
    onRename(next);
  };

  return (
    <div
      className={`glass-panel flex flex-col gap-3.5 p-4 ${
        focused ? "border-q-primary/60 ring-1 ring-q-primary/30" : ""
      }`}
      data-focused={focused || undefined}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-center gap-3">
          <div className="flex h-9 w-9 items-center justify-center rounded-[11px] border border-q-border bg-q-surface-strong text-q-primary shadow-q-sm">
            <Icon size={17} aria-hidden />
          </div>
          <div className="min-w-0">
            {onRename ? (
              <input
                value={draftName}
                onChange={(event) => setDraftName(event.target.value)}
                onBlur={commitRename}
                onKeyDown={(event) => {
                  if (event.key === "Enter") event.currentTarget.blur();
                  if (event.key === "Escape") {
                    setDraftName(source.displayName);
                    event.currentTarget.blur();
                  }
                }}
                disabled={renaming}
                maxLength={40}
                aria-label="账号名称"
                className="h-7 w-full min-w-0 rounded-md border border-transparent bg-transparent px-1 text-sm font-medium text-q-text-primary outline-none hover:border-q-border focus:border-q-primary"
              />
            ) : (
              <p className="text-sm font-medium text-q-text-primary">{source.displayName}</p>
            )}
            <p className="mt-0.5 text-xs text-q-text-muted">
              {source.accessMode ? ACCESS_MODE_LABEL[source.accessMode as SourceAccessMode] : SOURCE_TYPE_LABEL[source.sourceType]}
              {onRename ? " · 点击名称可重命名" : ""}
            </p>
          </div>
        </div>
        <StatusBadge tone={meta.tone}>{meta.label}</StatusBadge>
      </div>

      <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
        <div>
          <dt className="text-q-text-muted">凭据状态</dt>
          <dd className="mt-0.5 text-q-text-primary">
            {source.credentialConfigured ? "已配置" : "未配置"}
          </dd>
        </div>
        <div>
          <dt className="text-q-text-muted">能力覆盖</dt>
          <dd className="mt-0.5 text-q-text-primary">
            {source.capabilityIds.length > 0 ? `${source.capabilityIds.length} 项能力` : "—"}
          </dd>
        </div>
        {source.sourceType === "api_key" && apiBaseUrl ? (
          <div className="col-span-2">
            <dt className="text-q-text-muted">API 请求地址</dt>
            <dd className="mt-0.5 truncate text-q-text-primary" title={apiBaseUrl}>
              {apiBaseUrl}
            </dd>
          </div>
        ) : null}
        <div>
          <dt className="text-q-text-muted">最后验证</dt>
          <dd className="mt-0.5 text-q-text-primary">{formatDateTime(source.lastValidatedAt)}</dd>
        </div>
        <div>
          <dt className="text-q-text-muted">最后成功</dt>
          <dd className="mt-0.5 text-q-text-primary">{formatDateTime(source.lastSuccessAt)}</dd>
        </div>
      </dl>

      {source.state === "error" && source.errorMessage && (
        <div className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2">
          <p className="text-xs font-medium text-q-danger">
            {source.errorCode ?? "error"} · 刷新失败
          </p>
          <p className="mt-0.5 text-xs leading-relaxed text-q-danger/90">{source.errorMessage}</p>
        </div>
      )}

      {source.state === "auth_required" && (
        <div className="rounded-q-control border border-q-border bg-q-neutral-soft px-3 py-2">
          <p className="text-xs leading-relaxed text-q-neutral">尚未配置凭据，接入后可提供数据能力。</p>
        </div>
      )}

      <div className="mt-auto flex flex-wrap items-center gap-2">
        {(source.credentialInput || source.supportsCliLogin) && (
          <Button variant="secondary" size="sm" onClick={onEdit}>
            {source.supportsCliLogin ? "管理来源" : "编辑来源"}
          </Button>
        )}
        {source.supportsCliLogin && onRefresh && (
          <Button variant="secondary" size="sm" onClick={onRefresh} disabled={refreshing}>
            {refreshing ? "检测中…" : "检测并刷新"}
          </Button>
        )}
        {source.supportsInteractiveLogin && <Button variant="ghost" size="sm" onClick={onEdit}>网页登录</Button>}
      </div>
    </div>
  );
}
