import { useEffect, useState } from "react";
import {
  Check,
  ChevronDown,
  Globe,
  KeyRound,
  Link2,
  LoaderCircle,
  X,
  Zap,
} from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/Button";
import { SecretField } from "@/components/ui/SecretField";
import { EndpointSpeedPanel } from "./EndpointSpeedPanel";
import {
  clearSourceCredential,
  closeSourceLogin,
  fetchPlatformSetup,
  ipcErrorMessage,
  openExternalUrl,
  refreshPlatform,
  revealSourceSecret,
  saveSourceCredential,
  startSourceLogin,
  takeCapturedSourceSecret,
} from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import { formatDateTime } from "@/lib/format";
import type { SourceSummaryViewModel } from "@/lib/types";

/** DeepSeek 网页来源捕获后需显式「验证并保存」；GLM/MiMo 由后端自动验证保存。 */
const DEEPSEEK_WEB_ADAPTER = "deepseek-web-session";

type LoginStage = "idle" | "opening" | "waiting" | "verifying";

/** 网页登录阶段文案（阶段来自真实事件，不做假进度）。 */
function loginStages(source: SourceSummaryViewModel): string[] {
  return [
    "打开登录页",
    "等待登录",
    "验证会话",
    source.adapterId === DEEPSEEK_WEB_ADAPTER ? "验证并保存" : "自动保存",
  ];
}

const STAGE_INDEX: Record<LoginStage, number> = {
  idle: -1,
  opening: 0,
  waiting: 1,
  verifying: 2,
};

function webLoginCopy(adapterId: string): string {
  if (adapterId === "glm-web-balance") {
    return "AIQuotaMonitor 将打开一个独立的官方 GLM 登录窗口。在该窗口完成登录，看到财务总览后系统会自动验证会话并保存，无需手动复制或粘贴任何信息。";
  }
  if (adapterId === "mimo-web-session") {
    return "将打开隔离的 MiMo 登录窗口并读取含 httpOnly 的 Cookie。登录成功后自动验证并保存，清除凭据会退出该平台网页登录态。";
  }
  return "将打开独立的 DeepSeek 用量页登录窗口。登录成功后自动捕获会话，请再点「验证并保存」完成保存；清除凭据会退出网页登录态。";
}

/**
 * 右侧覆盖式来源编辑抽屉（V7 设计稿 03）：
 * - 标题统一「编辑」，副标题为 账号别名 · 来源名。
 * - API Key：Key + 获取链接 + 高级请求地址，主按钮「验证并保存」（后端先验证再落库，失败保留输入）。
 * - Web Session：状态 + 登录阶段步进（来自真实事件）+ 高级手动粘贴；底部只有「关闭」。
 * - 本地 CLI 不打开本抽屉（来源行直接「检测并刷新」）。秘密仅在本组件瞬时内存中存在。
 */
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
  const isApiKeySource = source?.sourceType === "api_key" && Boolean(source?.credentialInput);
  const isWebSource = source?.supportsInteractiveLogin === true;
  const setupQuery = useQuery({
    queryKey: ["platform-setup", platformId],
    queryFn: () => fetchPlatformSetup(platformId),
    enabled: Boolean(source) && isApiKeySource,
  });

  const [secret, setSecret] = useState("");
  const [apiBaseUrl, setApiBaseUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loginStatus, setLoginStatus] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [speedOpen, setSpeedOpen] = useState(false);
  const [loginStage, setLoginStage] = useState<LoginStage>("idle");
  const [loginOpened, setLoginOpened] = useState(false);
  const [captured, setCaptured] = useState(false);

  useEffect(() => {
    setSecret("");
    setApiBaseUrl(setupQuery.data?.apiBaseUrl ?? "");
    setError(null);
    setLoginStatus(null);
    setConfirmClear(false);
    setAdvancedOpen(false);
    setSpeedOpen(false);
    setLoginStage("idle");
    setLoginOpened(false);
    setCaptured(false);
  }, [source?.sourceId, setupQuery.data?.apiBaseUrl]);

  // 网页登录阶段全部来自真实事件：打开/等待由命令结果驱动，验证会话来自 captured，自动保存来自 credential-updated
  useEffect(() => {
    if (!source?.supportsInteractiveLogin) return;
    const currentSourceId = source.sourceId;
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void listen<string>("source-login-error", (event) => {
      setError(event.payload);
      setLoginStage("idle");
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    void listen<string>("source-login-status", (event) => {
      setLoginStatus(event.payload);
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    void listen<string>("source-login-closed", (event) => {
      if (!event.payload || event.payload === currentSourceId) {
        setLoginOpened(false);
        setLoginStage("idle");
      }
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    void listen<string>("source-login-captured", (event) => {
      if (event.payload !== currentSourceId) return;
      setLoginStage("verifying");
      setLoginOpened(false);
      setError(null);
      void takeCapturedSourceSecret(currentSourceId)
        .then((capturedSecret) => {
          setSecret(capturedSecret);
          setCaptured(true);
          setLoginStatus(
            source.adapterId === DEEPSEEK_WEB_ADAPTER
              ? "已捕获网页会话，请点「验证并保存」。"
              : "已捕获网页会话，正在自动验证并保存…",
          );
        })
        .catch((cause) => {
          setError(ipcErrorMessage(cause, "已捕获登录态，但读取会话失败，请重新登录。"));
        });
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    void listen<string>("source-credential-updated", (event) => {
      if (event.payload === currentSourceId) {
        // 自动保存型登录完成（后端已验证并写入），关闭抽屉并刷新平台数据
        setLoginStage("idle");
        onClose();
      }
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [source?.sourceId, source?.supportsInteractiveLogin, source?.adapterId, onClose]);

  const close = () => {
    setSecret("");
    setError(null);
    setLoginStatus(null);
    setLoginOpened(false);
    setLoginStage("idle");
    setCaptured(false);
    onClose();
  };
  const applyPlatforms = (platforms: Awaited<ReturnType<typeof saveSourceCredential>>) => {
    queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, platforms);
  };

  // 验证并保存：save_source_credential 后端先验证再写入 Vault，失败整体报错且不落库
  const saveMutation = useMutation({
    mutationFn: async () => {
      const saved = await saveSourceCredential(
        source!.sourceId,
        secret,
        isApiKeySource ? apiBaseUrl || undefined : undefined,
      );
      try {
        return await refreshPlatform(platformId);
      } catch {
        return saved;
      }
    },
    onSuccess: (platforms) => {
      applyPlatforms(platforms);
      close();
    },
    onError: (cause) => setError(ipcErrorMessage(cause, "验证或保存失败，请检查凭据后重试。")),
  });
  const clearMutation = useMutation({
    mutationFn: () => clearSourceCredential(source!.sourceId),
    onSuccess: (platforms) => {
      applyPlatforms(platforms);
      close();
    },
    onError: (cause) => setError(ipcErrorMessage(cause, "清除凭据失败，请稍后重试。")),
  });
  const loginMutation = useMutation({
    mutationFn: () => startSourceLogin(source!.sourceId),
    onSuccess: () => {
      setLoginOpened(true);
      setLoginStage("waiting");
    },
    onError: (cause) => {
      setLoginStage("idle");
      setError(ipcErrorMessage(cause, "无法启动网页登录。"));
    },
  });
  const closeLoginMutation = useMutation({
    mutationFn: () => closeSourceLogin(source!.sourceId),
    onSuccess: () => {
      setLoginOpened(false);
      setLoginStage("idle");
    },
    onError: (cause) => setError(ipcErrorMessage(cause, "无法关闭网页登录页。")),
  });

  if (!source) return null;
  // 本地 CLI 不进入抽屉：来源行直接「检测并刷新」，不制造空抽屉
  if (source.supportsCliLogin && !source.credentialInput && !source.supportsInteractiveLogin) return null;

  const input = source.credentialInput ?? null;
  const busy = saveMutation.isPending || clearMutation.isPending || loginMutation.isPending || closeLoginMutation.isPending;
  const showApiUrl = isApiKeySource && source.adapterId !== "kimi-balance-api";
  const isDeepSeekWeb = source.adapterId === DEEPSEEK_WEB_ADAPTER;
  const canSave = Boolean(input && secret.trim() && (!showApiUrl || apiBaseUrl.trim()) && !busy);
  const stages = loginStages(source);
  const activeStage = STAGE_INDEX[loginStage];

  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-black/45 backdrop-blur-sm" role="presentation" onMouseDown={close}>
      <aside
        className="flex h-full w-full max-w-md flex-col border-l border-q-border bg-q-surface-solid p-5 shadow-xl"
        role="dialog"
        aria-modal="true"
        aria-label="编辑来源"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <p className="text-lg font-semibold text-q-text-primary">编辑</p>
            <p className="mt-1 truncate text-sm text-q-text-secondary">
              {source.accountName} · {source.displayName}
            </p>
          </div>
          <Button variant="ghost" size="sm" onClick={close} aria-label="关闭">
            <X size={16} />
          </Button>
        </div>

        <div className="mt-5 flex flex-1 flex-col gap-4 overflow-y-auto pr-0.5">
          {isApiKeySource && input && (
            <>
              <SecretField
                label={input.label}
                value={secret}
                onChange={(next) => setSecret(next)}
                placeholder={source.credentialConfigured ? "已保存，点击眼睛查看或重新输入以更换" : input.placeholder}
                helpText={input.helpText}
                configured={source.credentialConfigured}
                onReveal={() => revealSourceSecret(source.sourceId)}
                disabled={busy}
              />
              {setupQuery.data?.apiKeyUrl && (
                <button
                  type="button"
                  className="self-start text-xs text-q-primary"
                  onClick={() =>
                    void openExternalUrl(setupQuery.data!.apiKeyUrl!).catch((cause) =>
                      setError(ipcErrorMessage(cause, "无法打开外部链接。")),
                    )
                  }
                >
                  获取 API Key
                </button>
              )}
              <details
                open={advancedOpen}
                onToggle={(event) => setAdvancedOpen((event.target as HTMLDetailsElement).open)}
                className="rounded-q-control border border-q-border bg-q-surface px-3 py-2"
              >
                <summary className="flex cursor-pointer list-none items-center justify-between text-[13px] font-medium text-q-text-primary">
                  高级设置
                  <ChevronDown size={14} aria-hidden className={advancedOpen ? "rotate-180 transition-transform" : "transition-transform"} />
                </summary>
                {showApiUrl && (
                  <div className="mt-3 flex flex-col gap-1.5">
                    <span className="flex items-center justify-between gap-2">
                      <span className="flex items-center gap-2 text-xs font-medium text-q-text-secondary">
                        API 请求地址
                        <span className="inline-flex items-center gap-1 rounded-full border border-q-border bg-q-neutral-soft px-2 py-0.5 text-[10px] text-q-text-muted">
                          <Link2 size={11} aria-hidden />
                          完整 URL
                        </span>
                      </span>
                      <button
                        type="button"
                        className="inline-flex cursor-pointer items-center gap-1 text-[11px] text-q-text-muted hover:text-q-text-primary"
                        onClick={() => setSpeedOpen(true)}
                      >
                        <Zap size={12} aria-hidden />
                        管理与测速
                      </button>
                    </span>
                    <input
                      value={apiBaseUrl}
                      onChange={(event) => setApiBaseUrl(event.target.value)}
                      placeholder={setupQuery.data?.officialApiBaseUrl || "https://api.deepseek.com"}
                      className="h-9 rounded-q-control border border-q-border bg-q-surface px-3 text-sm text-q-text-primary outline-none focus:border-q-primary"
                    />
                    <span className="text-[11px] leading-relaxed text-q-text-muted">
                      {setupQuery.data?.apiEndpointHint || "默认预填官方端点，用于查询余额或 Token Plan。"}
                    </span>
                  </div>
                )}
              </details>
            </>
          )}

          {isWebSource && (
            <>
              <dl className="grid grid-cols-2 gap-x-4 gap-y-3 rounded-q-control border border-q-border bg-q-surface px-3.5 py-3 text-xs">
                <div className="flex items-start gap-2">
                  <Globe size={14} aria-hidden className="mt-0.5 shrink-0 text-q-text-muted" />
                  <div>
                    <dt className="text-[11px] text-q-text-muted">接入方式</dt>
                    <dd className="mt-0.5 font-medium text-q-text-primary">网页登录</dd>
                  </div>
                </div>
                <div className="flex items-start gap-2">
                  <KeyRound size={14} aria-hidden className="mt-0.5 shrink-0 text-q-text-muted" />
                  <div>
                    <dt className="text-[11px] text-q-text-muted">凭据状态</dt>
                    <dd className="mt-0.5 font-medium text-q-text-primary">
                      {source.credentialConfigured ? "已配置" : "未配置"}
                    </dd>
                  </div>
                </div>
                <div>
                  <dt className="text-[11px] text-q-text-muted">能力覆盖</dt>
                  <dd className="mt-0.5 font-medium text-q-text-primary">
                    {source.capabilityIds.length > 0 ? `${source.capabilityIds.length} 项能力` : "—"}
                  </dd>
                </div>
                <div>
                  <dt className="text-[11px] text-q-text-muted">最后验证</dt>
                  <dd className="mt-0.5 font-medium tabular-nums text-q-text-primary">{formatDateTime(source.lastValidatedAt)}</dd>
                </div>
                <div>
                  <dt className="text-[11px] text-q-text-muted">最后成功</dt>
                  <dd className="mt-0.5 font-medium tabular-nums text-q-text-primary">{formatDateTime(source.lastSuccessAt)}</dd>
                </div>
              </dl>

              <p className="rounded-q-control border border-q-primary/20 bg-q-primary-softer px-3 py-2.5 text-xs leading-relaxed text-q-text-secondary">
                {webLoginCopy(source.adapterId)}
              </p>

              <div className="flex flex-col gap-2">
                <Button onClick={() => { setError(null); setLoginStage("opening"); loginMutation.mutate(); }} disabled={busy}>
                  {loginMutation.isPending && <LoaderCircle size={15} className="animate-spin" />}
                  <Globe size={15} aria-hidden />
                  {loginOpened ? "重新网页登录" : source.credentialConfigured ? "重新网页登录" : "网页登录"}
                </Button>
                {loginOpened && (
                  <button
                    type="button"
                    className="self-center text-[11px] text-q-text-muted hover:text-q-text-primary"
                    onClick={() => closeLoginMutation.mutate()}
                    disabled={busy}
                  >
                    关闭登录页
                  </button>
                )}
              </div>

              {/* 登录阶段：来自真实事件，不假推进 */}
              <div className="flex items-start px-1" aria-label="登录阶段">
                {stages.map((label, index) => (
                  <div key={label} className="flex min-w-0 flex-1 flex-col items-center gap-1.5 last:flex-none">
                    <div className="flex w-full items-center">
                      <span className={`h-px flex-1 ${index === 0 ? "invisible" : activeStage >= index ? "bg-q-primary/50" : "bg-q-border"}`} />
                      <span
                        className={`mx-1 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border ${
                          activeStage > index
                            ? "border-q-success bg-q-success text-white"
                            : activeStage === index
                              ? "border-q-primary bg-q-primary text-white"
                              : "border-q-border-strong bg-q-surface"
                        }`}
                      >
                        {activeStage > index && <Check size={10} aria-hidden />}
                        {activeStage === index && loginMutation.isPending && <LoaderCircle size={10} className="animate-spin" />}
                      </span>
                      <span className={`h-px flex-1 ${index === stages.length - 1 ? "invisible" : activeStage >= index ? "bg-q-primary/50" : "bg-q-border"}`} />
                    </div>
                    <span className={`whitespace-nowrap text-[10px] ${activeStage >= index ? "font-medium text-q-text-primary" : "text-q-text-muted"}`}>
                      {label}
                    </span>
                  </div>
                ))}
              </div>

              {isDeepSeekWeb && captured && (
                <Button onClick={() => { setError(null); saveMutation.mutate(); }} disabled={!canSave}>
                  {saveMutation.isPending ? "验证并保存中…" : "验证并保存"}
                </Button>
              )}

              {input && (
                <details
                  open={advancedOpen}
                  onToggle={(event) => setAdvancedOpen((event.target as HTMLDetailsElement).open)}
                  className="rounded-q-control border border-q-border bg-q-surface px-3 py-2"
                >
                  <summary className="flex cursor-pointer list-none items-center justify-between text-[13px] font-medium text-q-text-primary">
                    手动粘贴（高级）
                    <ChevronDown size={14} aria-hidden className={advancedOpen ? "rotate-180 transition-transform" : "transition-transform"} />
                  </summary>
                  <div className="mt-3">
                    <SecretField
                      label={input.label}
                      value={secret}
                      onChange={(next) => { setSecret(next); setCaptured(false); }}
                      placeholder={input.placeholder}
                      helpText={input.helpText}
                      configured={source.credentialConfigured}
                      onReveal={() => revealSourceSecret(source.sourceId)}
                      disabled={busy}
                    />
                    {/* 手动粘贴与捕获会话共用同一条「验证并保存」路径（自动保存型也可手动兜底） */}
                    <button
                      type="button"
                      className="mt-2 text-xs font-medium text-q-primary disabled:opacity-60"
                      onClick={() => { setError(null); saveMutation.mutate(); }}
                      disabled={!canSave}
                    >
                      {saveMutation.isPending ? "验证并保存中…" : "验证并保存"}
                    </button>
                  </div>
                </details>
              )}
            </>
          )}

          {loginStatus && (
            <p className="rounded-q-control border border-q-warning/30 bg-q-warning-soft px-3 py-2 text-xs leading-relaxed text-q-text-secondary">
              {loginStatus}
            </p>
          )}
          {error && (
            <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">{error}</p>
          )}

          {/* 危险区：清除凭据需二次确认，并说明会失去的能力 */}
          {source.credentialConfigured && (
            <div className="rounded-q-control border border-q-danger/25 bg-q-danger-soft/60 px-3 py-2.5">
              {confirmClear ? (
                <div className="flex flex-col gap-2">
                  <p className="text-xs leading-relaxed text-q-danger">
                    清除后将删除已保存的{source.sourceType === "web_session" ? "网页登录会话" : "API Key"}，
                    该来源的 {source.capabilityIds.length} 项能力会暂停更新，下次使用需重新配置。确定继续？
                  </p>
                  <div className="flex gap-2">
                    <Button size="sm" onClick={() => clearMutation.mutate()} disabled={busy}>
                      {clearMutation.isPending ? "清除中…" : "确认清除"}
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => setConfirmClear(false)} disabled={busy}>
                      取消
                    </Button>
                  </div>
                </div>
              ) : (
                <button
                  type="button"
                  className="cursor-pointer text-xs font-medium text-q-danger hover:underline"
                  onClick={() => setConfirmClear(true)}
                  disabled={busy}
                >
                  {source.sourceType === "web_session" ? "清除登录状态" : "清除凭据"}
                </button>
              )}
            </div>
          )}
        </div>

        {/* 底部：API Key 抽屉有主按钮；网页抽屉自动保存型只有「关闭」 */}
        <div className="mt-4 flex justify-end gap-2 border-t border-q-border pt-4">
          <Button variant="ghost" onClick={close} disabled={saveMutation.isPending || clearMutation.isPending}>
            关闭
          </Button>
          {isApiKeySource && input && (
            <Button onClick={() => { setError(null); saveMutation.mutate(); }} disabled={!canSave}>
              {saveMutation.isPending ? (
                <>
                  <LoaderCircle size={14} className="animate-spin" />
                  验证并保存中…
                </>
              ) : (
                "验证并保存"
              )}
            </Button>
          )}
        </div>
      </aside>
      {speedOpen && (
        <EndpointSpeedPanel
          open
          currentUrl={apiBaseUrl}
          officialUrl={setupQuery.data?.officialApiBaseUrl || ""}
          onApply={(url) => setApiBaseUrl(url)}
          onClose={() => setSpeedOpen(false)}
        />
      )}
    </div>
  );
}
