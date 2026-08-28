import { useEffect, useState } from "react";
import { ExternalLink, Link2, LoaderCircle, X } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/Button";
import {
  clearSourceCredential,
  closeSourceLogin,
  fetchPlatformSetup,
  ipcErrorMessage,
  saveSourceCredential,
  startSourceLogin,
  validateSourceCredential,
} from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import type { SourceSummaryViewModel } from "@/lib/types";

/** 右侧覆盖式来源配置抽屉；秘密仅在该组件的瞬时内存中存在。 */
export function SourceEditorDrawer({
  source,
  platformId,
  onClose,
}: {
  source: SourceSummaryViewModel | null;
  platformId: string;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const setupQuery = useQuery({
    queryKey: ["platform-setup", platformId],
    queryFn: () => fetchPlatformSetup(platformId),
    enabled: Boolean(source),
  });
  const [secret, setSecret] = useState("");
  const [apiBaseUrl, setApiBaseUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loginStatus, setLoginStatus] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [loginOpened, setLoginOpened] = useState(false);
  const [verifiedSecret, setVerifiedSecret] = useState<string | null>(null);
  const [verifyMessage, setVerifyMessage] = useState<string | null>(null);

  useEffect(() => {
    setSecret("");
    setApiBaseUrl(setupQuery.data?.apiBaseUrl ?? "");
    setError(null);
    setLoginStatus(null);
    setConfirmClear(false);
    setLoginOpened(false);
    setVerifiedSecret(null);
    setVerifyMessage(null);
  }, [source?.sourceId, setupQuery.data?.apiBaseUrl]);

  useEffect(() => {
    if (!source?.supportsInteractiveLogin) return;
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void listen<string>("source-login-error", (event) => {
      setError(event.payload);
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    void listen<string>("source-login-status", (event) => {
      setLoginStatus(event.payload);
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    void listen("source-login-closed", () => {
      setLoginOpened(false);
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    void listen<string>("source-credential-updated", (event) => {
      if (event.payload === source.sourceId) {
        setLoginOpened(false);
        onClose();
      }
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [source?.sourceId, source?.supportsInteractiveLogin, onClose]);

  const close = () => {
    if (source?.supportsInteractiveLogin) void closeSourceLogin(source.sourceId);
    setSecret("");
    setError(null);
    setLoginStatus(null);
    setLoginOpened(false);
    setVerifiedSecret(null);
    setVerifyMessage(null);
    onClose();
  };
  const updatePlatforms = (platforms: Awaited<ReturnType<typeof saveSourceCredential>>) => {
    queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, platforms);
  };
  const verifyMutation = useMutation({
    mutationFn: () => validateSourceCredential(source!.sourceId, secret, apiBaseUrl || undefined),
    onSuccess: (message) => {
      setError(null);
      setVerifiedSecret(secret);
      setVerifyMessage(message);
    },
    onError: (cause) => {
      setVerifiedSecret(null);
      setVerifyMessage(null);
      setError(ipcErrorMessage(cause, "连接验证失败，请检查凭据后重试。"));
    },
  });
  const saveMutation = useMutation({
    mutationFn: async () => {
      const sourceId = source!.sourceId;
      return { sourceId, platforms: await saveSourceCredential(sourceId, secret, apiBaseUrl || undefined) };
    },
    onSuccess: (result) => {
      updatePlatforms(result.platforms);
      if (source?.sourceId === result.sourceId) close();
    },
    onError: (cause) => setError(ipcErrorMessage(cause, "保存失败，请检查凭据后重试。")),
  });
  const clearMutation = useMutation({
    mutationFn: () => clearSourceCredential(source!.sourceId),
    onSuccess: (platforms) => { updatePlatforms(platforms); close(); },
    onError: (cause) => setError(ipcErrorMessage(cause, "清除凭据失败，请稍后重试。")),
  });
  const loginMutation = useMutation({
    mutationFn: () => startSourceLogin(source!.sourceId),
    onSuccess: () => setLoginOpened(true),
    onError: (cause) => setError(ipcErrorMessage(cause, "无法启动网页登录。")),
  });
  const closeLoginMutation = useMutation({
    mutationFn: () => closeSourceLogin(source!.sourceId),
    onSuccess: () => setLoginOpened(false),
    onError: (cause) => setError(ipcErrorMessage(cause, "无法关闭网页登录页。")),
  });

  if (!source) return null;
  const input = source.credentialInput ?? null;
  const busy = verifyMutation.isPending || saveMutation.isPending || clearMutation.isPending || loginMutation.isPending || closeLoginMutation.isPending;
  const setup = setupQuery.data;
  const showApiUrl = source.sourceType === "api_key";
  const canSave = Boolean(input && secret.trim() && verifiedSecret === secret && (!showApiUrl || apiBaseUrl.trim()) && !busy);

  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-black/20" role="presentation" onMouseDown={close}>
      <aside className="flex h-full w-full max-w-md flex-col border-l border-q-border bg-q-surface p-5 shadow-xl" role="dialog" aria-modal="true" aria-label={`编辑 ${source.displayName}`} onMouseDown={(event) => event.stopPropagation()}>
        <div className="flex items-start justify-between gap-3">
          <div><p className="text-lg font-semibold text-q-text-primary">编辑来源</p><p className="mt-1 text-sm text-q-text-secondary">{source.displayName}</p></div>
          <Button variant="ghost" size="sm" onClick={close} aria-label="关闭编辑来源"><X size={16} /></Button>
        </div>
        <div className="mt-6 flex flex-1 flex-col gap-4">
          {setup?.officialUrl && (
            <label className="flex flex-col gap-2 text-sm font-medium text-q-text-primary">
              官网链接
              <div className="flex gap-2">
                <input readOnly value={setup.officialUrl} className="h-10 min-w-0 flex-1 rounded-q-control border border-q-border bg-q-neutral-soft px-3 text-sm font-normal text-q-text-primary" />
                <Button type="button" variant="secondary" size="sm" onClick={() => window.open(setup.officialUrl, "_blank", "noopener,noreferrer")}>
                  <ExternalLink size={14} aria-hidden />
                  打开
                </Button>
              </div>
            </label>
          )}
          {input ? <>
            <label className="flex flex-col gap-2 text-sm font-medium text-q-text-primary">
              {input.label}
              <input type="password" autoComplete="off" value={secret} onChange={(event) => { setSecret(event.target.value); setVerifiedSecret(null); setVerifyMessage(null); }} placeholder={input.placeholder} className="h-10 rounded-q-control border border-q-border bg-q-surface px-3 text-sm font-normal text-q-text-primary outline-none focus:border-q-primary" />
            </label>
            {setup?.apiKeyUrl && source.sourceType === "api_key" && (
              <button type="button" className="self-start text-xs text-q-primary" onClick={() => window.open(setup.apiKeyUrl!, "_blank", "noopener,noreferrer")}>
                获取 API Key
              </button>
            )}
            <p className="text-xs leading-relaxed text-q-text-muted">{input.helpText}</p>
          </> : <p className="rounded-q-control border border-q-border bg-q-neutral-soft px-3 py-2 text-sm text-q-text-secondary">此本地来源由应用自动检测，无需输入凭据。</p>}
          {showApiUrl && (
            <label className="flex flex-col gap-2 text-sm font-medium text-q-text-primary">
              <span className="flex items-center gap-2">
                API 请求地址
                <span className="inline-flex items-center gap-1 rounded-full border border-q-border bg-q-neutral-soft px-2 py-0.5 text-[11px] font-medium text-q-text-secondary">
                  <Link2 size={12} aria-hidden />
                  完整 URL
                </span>
              </span>
              <input
                value={apiBaseUrl}
                onChange={(event) => { setApiBaseUrl(event.target.value); setVerifiedSecret(null); setVerifyMessage(null); }}
                placeholder={setup?.officialApiBaseUrl || "https://api.deepseek.com"}
                className="h-10 rounded-q-control border border-q-border bg-q-surface px-3 text-sm font-normal text-q-text-primary outline-none focus:border-q-primary"
              />
              <span className="text-xs font-normal leading-relaxed text-q-text-muted">
                {setup?.apiEndpointHint || "默认预填官方端点，用于查询余额或 Token Plan。"}
              </span>
            </label>
          )}
          {source.supportsInteractiveLogin === true && <div className="flex flex-col gap-2">
            <div className="flex gap-2">
              <Button variant="secondary" size="sm" onClick={() => { setError(null); setLoginStatus(loginOpened ? "正在打开 DeepSeek 用量页并同步…" : "请在登录窗口完成登录并打开用量页。同步成功后会刷新 Token 与缓存。"); loginMutation.mutate(); }} disabled={busy}>
                {loginMutation.isPending && <LoaderCircle size={15} className="animate-spin" />}
                {loginOpened ? "重新加载登录页" : "网页登录"}
              </Button>
              {loginOpened && <Button variant="ghost" size="sm" onClick={() => closeLoginMutation.mutate()} disabled={busy}>关闭登录页</Button>}
            </div>
            <p className="text-xs leading-relaxed text-q-text-muted">会打开 DeepSeek 用量页并同步 Token 与缓存。清除凭据会退出网页登录态；下次需要重新登录，不会静默复用旧会话。</p>
          </div>}
          {verifyMessage && <p className="rounded-q-control border border-q-success/25 bg-q-success-soft px-3 py-2 text-xs text-q-success">{verifyMessage}</p>}
          {loginStatus && <p className="rounded-q-control border border-q-warning/30 bg-q-warning-soft px-3 py-2 text-xs leading-relaxed text-q-text-secondary">{loginStatus}</p>}
          {error && <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">{error}</p>}
          {source.credentialConfigured && (confirmClear ? <div className="rounded-q-control border border-q-danger/25 bg-q-danger-soft p-3"><p className="text-xs leading-relaxed text-q-danger">清除后将无法通过此来源获取对应额度，确定继续？</p><div className="mt-3 flex gap-2"><Button size="sm" onClick={() => clearMutation.mutate()} disabled={busy}>{clearMutation.isPending ? "清除中…" : "确认清除"}</Button><Button variant="ghost" size="sm" onClick={() => setConfirmClear(false)} disabled={busy}>取消</Button></div></div> : <Button variant="ghost" size="sm" onClick={() => setConfirmClear(true)} disabled={busy}>清除凭据</Button>)}
        </div>
        <div className="flex justify-end gap-2 border-t border-q-border pt-4">
          <Button variant="ghost" onClick={close}>取消</Button>
          {input && (
            <>
              <Button variant="secondary" onClick={() => { setError(null); verifyMutation.mutate(); }} disabled={!secret.trim() || (showApiUrl && !apiBaseUrl.trim()) || busy}>
                {verifyMutation.isPending ? "验证中…" : "验证连接"}
              </Button>
              <Button onClick={() => { setError(null); saveMutation.mutate(); }} disabled={!canSave}>
                {saveMutation.isPending ? "保存中…" : "保存"}
              </Button>
            </>
          )}
        </div>
      </aside>
    </div>
  );
}
