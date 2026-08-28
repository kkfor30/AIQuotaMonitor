import { useEffect, useState } from "react";
import { ExternalLink, Link2, Zap } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/Button";
import {
  clearSourceCredential,
  fetchPlatformSetup,
  ipcErrorMessage,
  refreshPlatform,
  savePlatformSetup,
  startSourceLogin,
  validateSourceCredential,
} from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import type { PlatformSetupViewModel } from "@/lib/types";

function openExternal(url: string) {
  window.open(url, "_blank", "noopener,noreferrer");
}

const fieldClass =
  "h-10 w-full rounded-q-control border border-q-border bg-q-surface px-3 text-sm text-q-text-primary outline-none focus:border-q-primary";

export function PlatformSetupForm({ platformId }: { platformId: string }) {
  const queryClient = useQueryClient();
  const setupQuery = useQuery({
    queryKey: ["platform-setup", platformId],
    queryFn: () => fetchPlatformSetup(platformId),
  });
  const setup = setupQuery.data;
  const [displayName, setDisplayName] = useState("");
  const [notes, setNotes] = useState("");
  const [apiBaseUrl, setApiBaseUrl] = useState("");
  const [secret, setSecret] = useState("");
  const [verifiedSecret, setVerifiedSecret] = useState<string | null>(null);
  const [verifyMessage, setVerifyMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmCliClear, setConfirmCliClear] = useState(false);

  useEffect(() => {
    if (!setup) return;
    applySetup(setup);
  }, [setup]);

  function applySetup(next: PlatformSetupViewModel) {
    setDisplayName(next.displayName);
    setNotes(next.notes);
    setApiBaseUrl(next.apiBaseUrl);
    setSecret("");
    setVerifiedSecret(null);
    setVerifyMessage(null);
    setError(null);
    setConfirmCliClear(false);
  }

  const verifyMutation = useMutation({
    mutationFn: () => validateSourceCredential(setup!.apiKeySourceId!, secret, apiBaseUrl),
    onSuccess: (message) => {
      setError(null);
      setVerifiedSecret(secret);
      setVerifyMessage(message);
    },
    onError: (cause) => {
      setVerifiedSecret(null);
      setVerifyMessage(null);
      setError(ipcErrorMessage(cause, "连接验证失败，请检查 API Key 或请求地址。"));
    },
  });
  const saveMutation = useMutation({
    mutationFn: () =>
      savePlatformSetup({
        platformId,
        displayName,
        notes,
        apiBaseUrl,
        sourceId: setup?.apiKeySourceId,
        secret,
      }),
    onSuccess: (platforms) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, platforms);
      void queryClient.invalidateQueries({ queryKey: ["platform-setup", platformId] });
      setSecret("");
      setVerifiedSecret(null);
    },
    onError: (cause) => setError(ipcErrorMessage(cause, "保存失败，请检查后重试。")),
  });
  const cliLoginMutation = useMutation({
    mutationFn: async () => {
      await startSourceLogin(setup!.localCliSourceId!);
      return refreshPlatform(platformId);
    },
    onSuccess: (platforms) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, platforms);
      void queryClient.invalidateQueries({ queryKey: ["platform-setup", platformId] });
      setError(null);
      setVerifyMessage("已重新连接本机 Codex 登录。");
    },
    onError: (cause) => setError(ipcErrorMessage(cause, "无法启动 Codex 登录。")),
  });
  const cliRefreshMutation = useMutation({
    mutationFn: () => refreshPlatform(platformId),
    onSuccess: (platforms) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, platforms);
      setError(null);
      setVerifyMessage("已检测本机 Codex 登录并刷新。");
    },
    onError: (cause) => setError(ipcErrorMessage(cause, "检测本机 Codex 登录失败。")),
  });
  const cliClearMutation = useMutation({
    mutationFn: () => clearSourceCredential(setup!.localCliSourceId!),
    onSuccess: (platforms) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, platforms);
      void queryClient.invalidateQueries({ queryKey: ["platform-setup", platformId] });
      setConfirmCliClear(false);
      setError(null);
      setVerifyMessage("已退出本机 Codex 登录。");
    },
    onError: (cause) => setError(ipcErrorMessage(cause, "清除本机 Codex 登录失败。")),
  });

  if (setupQuery.isLoading) {
    return <p className="text-sm text-q-text-muted">正在加载接入表单…</p>;
  }
  if (setupQuery.isError || !setup) {
    return (
      <p className="text-sm text-q-danger">
        {ipcErrorMessage(setupQuery.error, "无法加载接入表单。")}
      </p>
    );
  }

  const busy =
    verifyMutation.isPending
    || saveMutation.isPending
    || cliLoginMutation.isPending
    || cliRefreshMutation.isPending
    || cliClearMutation.isPending;
  const replacingKey = Boolean(secret.trim());
  const keyReady =
    !setup.needsApiKey || (replacingKey ? verifiedSecret === secret : setup.apiKeyConfigured);
  const canSave =
    Boolean(displayName.trim())
    && (!setup.needsApiKey || Boolean(apiBaseUrl.trim()))
    && keyReady
    && !busy
    && (!setup.needsApiKey || Boolean(setup.apiKeySourceId));

  return (
    <div className="flex w-full max-w-2xl flex-col gap-4">
      <div>
        <h2 className="text-xl font-semibold text-q-text-primary">接入 {setup.displayName}</h2>
        <p className="mt-1 text-sm text-q-text-secondary">
          填写供应商信息、官网链接、官方 API 请求地址和 API Key。先验证连接，成功后再保存。
        </p>
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <label className="flex flex-col gap-1.5 text-sm font-medium text-q-text-primary">
          供应商名称
          <input value={displayName} onChange={(event) => setDisplayName(event.target.value)} className={fieldClass} />
        </label>
        <label className="flex flex-col gap-1.5 text-sm font-medium text-q-text-primary">
          备注
          <input
            value={notes}
            onChange={(event) => setNotes(event.target.value)}
            placeholder="例如：公司专用账号"
            className={fieldClass}
          />
        </label>
      </div>

      <label className="flex flex-col gap-1.5 text-sm font-medium text-q-text-primary">
        官网链接
        <div className="flex gap-2">
          <input value={setup.officialUrl} readOnly className={`${fieldClass} flex-1 bg-q-neutral-soft`} />
          <Button
            type="button"
            variant="secondary"
            size="sm"
            disabled={!setup.officialUrl}
            onClick={() => openExternal(setup.officialUrl)}
          >
            <ExternalLink size={14} aria-hidden />
            打开
          </Button>
        </div>
      </label>

      {setup.needsApiKey && setup.apiKeySourceId && (
        <>
          <label className="flex flex-col gap-1.5 text-sm font-medium text-q-text-primary">
            API Key
            <input
              type="password"
              autoComplete="off"
              value={secret}
              onChange={(event) => {
                setSecret(event.target.value);
                setVerifiedSecret(null);
                setVerifyMessage(null);
              }}
              placeholder={setup.apiKeyConfigured ? "已保存，更换时请重新输入" : "sk-…"}
              className={fieldClass}
            />
          </label>
          {setup.apiKeyUrl && (
            <button
              type="button"
              className="self-start text-xs text-q-primary"
              onClick={() => openExternal(setup.apiKeyUrl!)}
            >
              获取 API Key
            </button>
          )}

          <div className="flex flex-col gap-2">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex flex-wrap items-center gap-3">
                <span className="text-sm font-medium text-q-text-primary">API 请求地址</span>
                <span className="inline-flex items-center gap-1.5 rounded-full border border-q-border bg-q-neutral-soft px-2.5 py-1 text-xs font-medium text-q-text-secondary">
                  <Link2 size={13} aria-hidden />
                  完整 URL
                </span>
              </div>
              <button
                type="button"
                className="inline-flex items-center gap-1 text-xs text-q-text-muted hover:text-q-text-primary"
                title="打开官网管理账号。本应用只查询额度，不提供测速代理。"
                onClick={() => {
                  if (setup.officialApiBaseUrl) setApiBaseUrl(setup.officialApiBaseUrl);
                  if (setup.officialUrl) openExternal(setup.officialUrl);
                }}
              >
                <Zap size={13} aria-hidden />
                管理与测速
              </button>
            </div>
            <input
              value={apiBaseUrl}
              onChange={(event) => {
                setApiBaseUrl(event.target.value);
                setVerifiedSecret(null);
                setVerifyMessage(null);
              }}
              placeholder={setup.officialApiBaseUrl || "https://api.deepseek.com"}
              className={fieldClass}
            />
            <div className="rounded-q-control border border-q-warning/30 bg-q-warning-soft px-3 py-2">
              <p className="text-xs leading-relaxed text-q-warning">
                {setup.apiEndpointHint || "默认预填该平台官方端点，用于查询余额或 Token Plan。请填写完整 URL，不要以斜杠结尾。"}
              </p>
            </div>
          </div>
        </>
      )}

      {setup.needsLocalCli && (
        <div className="rounded-q-control border border-q-border bg-q-neutral-soft px-3 py-3">
          <p className="text-sm text-q-text-secondary">此平台检测本机 Codex CLI 的 ChatGPT 登录，无需填写 API Key。</p>
          <p className="mt-1 text-xs leading-relaxed text-q-text-muted">
            刷新优先走本机 `codex app-server`。只有 CLI 读不到额度时才连接 chatgpt.com；连不上时请检查代理，或重新登录后再检测。
          </p>
          {setup.localCliSourceId && (
            <div className="mt-3 flex flex-wrap gap-2">
              <Button variant="secondary" size="sm" disabled={busy} onClick={() => cliLoginMutation.mutate()}>
                {cliLoginMutation.isPending ? "等待登录…" : "重新登录 Codex CLI"}
              </Button>
              <Button variant="secondary" size="sm" disabled={busy} onClick={() => cliRefreshMutation.mutate()}>
                {cliRefreshMutation.isPending ? "检测中…" : "检测并刷新"}
              </Button>
              {confirmCliClear ? (
                <>
                  <Button size="sm" disabled={busy} onClick={() => cliClearMutation.mutate()}>
                    {cliClearMutation.isPending ? "清除中…" : "确认退出本机登录"}
                  </Button>
                  <Button variant="ghost" size="sm" disabled={busy} onClick={() => setConfirmCliClear(false)}>
                    取消
                  </Button>
                </>
              ) : (
                <Button variant="ghost" size="sm" disabled={busy} onClick={() => setConfirmCliClear(true)}>
                  清除本机登录
                </Button>
              )}
            </div>
          )}
        </div>
      )}
      {setup.needsWebLogin && (
        <p className="rounded-q-control border border-q-border bg-q-neutral-soft px-3 py-2 text-sm text-q-text-secondary">
          用量、缓存等官方未开放的字段，在下方来源中使用网页登录后再同步。
        </p>
      )}

      {verifyMessage && (
        <p className="rounded-q-control border border-q-success/25 bg-q-success-soft px-3 py-2 text-xs text-q-success">{verifyMessage}</p>
      )}
      {error && <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">{error}</p>}

      <div className="flex justify-end gap-2 pt-2">
        <Button variant="ghost" onClick={() => applySetup(setup)} disabled={busy}>
          取消
        </Button>
        {setup.needsApiKey && (
          <Button
            variant="secondary"
            onClick={() => verifyMutation.mutate()}
            disabled={!secret.trim() || !apiBaseUrl.trim() || busy}
          >
            {verifyMutation.isPending ? "验证中…" : "验证连接"}
          </Button>
        )}
        <Button onClick={() => saveMutation.mutate()} disabled={!canSave}>
          {saveMutation.isPending ? "保存中…" : "保存"}
        </Button>
      </div>
    </div>
  );
}
