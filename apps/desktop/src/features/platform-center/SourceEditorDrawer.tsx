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
  refreshPlatform,
  removeCodexAccount,
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
    void listen<string>("source-login-closed", (event) => {
      if (!event.payload || event.payload === source.sourceId) {
        setLoginOpened(false);
      }
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
    onSuccess: async () => {
      if (source?.supportsCliLogin) {
        setLoginStatus("Codex 登录完成，正在检测额度…");
        try {
          updatePlatforms(await refreshPlatform(platformId));
          setLoginStatus(source.sourceId.startsWith("openai-codex-extra-") ? "已登录额外 ChatGPT 账号。" : "已重新连接本机 Codex 登录。");
        } catch (cause) {
          setError(ipcErrorMessage(cause, "登录已完成，但刷新额度失败。"));
        }
        return;
      }
      setLoginOpened(true);
    },
    onError: (cause) => setError(ipcErrorMessage(cause, source?.supportsCliLogin ? "无法启动 Codex 登录。" : "无法启动网页登录。")),
  });
  const removeExtraMutation = useMutation({
    mutationFn: () => removeCodexAccount(source!.sourceId),
    onSuccess: (platforms) => { updatePlatforms(platforms); close(); },
    onError: (cause) => setError(ipcErrorMessage(cause, "移除额外账号失败。")),
  });
  const closeLoginMutation = useMutation({
    mutationFn: () => closeSourceLogin(source!.sourceId),
    onSuccess: () => setLoginOpened(false),
    onError: (cause) => setError(ipcErrorMessage(cause, "无法关闭网页登录页。")),
  });

  if (!source) return null;
  const input = source.credentialInput ?? null;
  const isCli = source.supportsCliLogin === true;
  const isExtraCodex = source.sourceId.startsWith("openai-codex-extra-");
  const busy = verifyMutation.isPending || saveMutation.isPending || clearMutation.isPending || loginMutation.isPending || closeLoginMutation.isPending || removeExtraMutation.isPending;
  const setup = setupQuery.data;
  const showApiUrl = source.sourceType === "api_key" && source.sourceId !== "kimi-balance-api";
  const webLoginHint = webLoginCopy(source.sourceId);
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
          </> : isCli ? (
            <div className="rounded-q-control border border-q-border bg-q-neutral-soft px-3 py-2">
              <p className="text-sm text-q-text-secondary">
                {isExtraCodex
                  ? "额外账号使用独立的 Codex 登录目录，不会覆盖本机当前 CLI 登录。"
                  : "默认直接检测本机 Codex CLI 的 ChatGPT 登录，不必先开网页。"}
              </p>
              <p className="mt-1 text-xs leading-relaxed text-q-text-muted">
                {isExtraCodex
                  ? "点「登录另一个账号」会弹出官方 Codex 登录。额度与本机账号分开刷新、分开展示。"
                  : "点「检测并刷新」即可读取窗口额度。只有要更换本机 CLI 当前账号时，才需要重新登录。"}
              </p>
            </div>
          ) : (
            <p className="rounded-q-control border border-q-border bg-q-neutral-soft px-3 py-2 text-sm text-q-text-secondary">此本地来源由应用自动检测，无需输入凭据。</p>
          )}
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
          {isCli && (
            <div className="flex flex-col gap-2">
              <Button
                variant="secondary"
                size="sm"
                onClick={() => {
                  setError(null);
                  setLoginStatus(
                    isExtraCodex
                      ? "请在弹出的 Codex 登录窗口登录另一个 ChatGPT 账号。不会改写本机 ~/.codex。"
                      : "这会更换本机 Codex CLI 当前登录。若只想读取现有凭证，请关闭后点「检测并刷新」。",
                  );
                  loginMutation.mutate();
                }}
                disabled={busy}
              >
                {loginMutation.isPending && <LoaderCircle size={15} className="animate-spin" />}
                {loginMutation.isPending ? "等待 Codex 登录…" : isExtraCodex ? "登录另一个账号" : "更换本机 Codex 登录"}
              </Button>
            </div>
          )}
          {source.supportsInteractiveLogin === true && <div className="flex flex-col gap-2">
            <div className="flex gap-2">
              <Button variant="secondary" size="sm" onClick={() => { setError(null); setLoginStatus(loginOpened ? webLoginHint.reload : webLoginHint.start); loginMutation.mutate(); }} disabled={busy}>
                {loginMutation.isPending && <LoaderCircle size={15} className="animate-spin" />}
                {loginOpened ? "重新加载登录页" : "网页登录"}
              </Button>
              {loginOpened && <Button variant="ghost" size="sm" onClick={() => closeLoginMutation.mutate()} disabled={busy}>关闭登录页</Button>}
            </div>
            <p className="text-xs leading-relaxed text-q-text-muted">{webLoginHint.help}</p>
          </div>}
          {verifyMessage && <p className="rounded-q-control border border-q-success/25 bg-q-success-soft px-3 py-2 text-xs text-q-success">{verifyMessage}</p>}
          {loginStatus && <p className="rounded-q-control border border-q-warning/30 bg-q-warning-soft px-3 py-2 text-xs leading-relaxed text-q-text-secondary">{loginStatus}</p>}
          {error && <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">{error}</p>}
          {(source.credentialConfigured || isCli) && (confirmClear ? (
            <div className="rounded-q-control border border-q-danger/25 bg-q-danger-soft p-3">
              <p className="text-xs leading-relaxed text-q-danger">
                {isExtraCodex
                  ? "只清除这个额外账号的独立登录，不会退出本机 Codex CLI。确定继续？"
                  : source.sourceType === "local_cli"
                    ? "将退出本机 Codex CLI 的 ChatGPT 登录。Codex 命令行也需要重新登录，确定继续？"
                    : "清除后将无法通过此来源获取对应额度，确定继续？"}
              </p>
              <div className="mt-3 flex gap-2">
                <Button size="sm" onClick={() => clearMutation.mutate()} disabled={busy}>
                  {clearMutation.isPending ? "清除中…" : "确认清除"}
                </Button>
                <Button variant="ghost" size="sm" onClick={() => setConfirmClear(false)} disabled={busy}>
                  取消
                </Button>
              </div>
            </div>
          ) : (
            <Button variant="ghost" size="sm" onClick={() => setConfirmClear(true)} disabled={busy}>
              {isExtraCodex ? "清除这个账号登录" : source.sourceType === "local_cli" ? "清除本机登录" : "清除凭据"}
            </Button>
          ))}
          {isExtraCodex && (
            <Button variant="ghost" size="sm" onClick={() => removeExtraMutation.mutate()} disabled={busy}>
              {removeExtraMutation.isPending ? "移除中…" : "移除这个额外账号"}
            </Button>
          )}
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

function webLoginCopy(sourceId: string): { start: string; reload: string; help: string } {
  if (sourceId === "glm-web-balance") {
    return {
      start: "请在登录窗口完成 GLM 登录。同步成功后会验证并保存网页个人余额会话。",
      reload: "正在打开 GLM 财务页并同步…",
      help: "会打开隔离登录窗口。登录成功后自动验证控制台余额接口。清除凭据会退出该平台网页登录态。",
    };
  }
  if (sourceId === "mimo-web-session") {
    return {
      start: "请在登录窗口完成 MiMo 登录。同步成功后会验证并保存网页余额会话。",
      reload: "正在打开 MiMo 平台并同步…",
      help: "会打开隔离登录窗口并读取含 httpOnly 的 Cookie。清除凭据会退出该平台网页登录态。",
    };
  }
  return {
    start: "请在登录窗口完成登录并打开用量页。同步成功后会刷新 Token 与缓存。",
    reload: "正在打开 DeepSeek 用量页并同步…",
    help: "会打开 DeepSeek 用量页并同步 Token 与缓存。清除凭据会退出网页登录态；下次需要重新登录，不会静默复用旧会话。",
  };
}
