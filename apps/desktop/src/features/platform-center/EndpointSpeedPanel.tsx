import { useMemo, useState } from "react";
import { AlertCircle, LoaderCircle, Plus, X, Zap } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { ipcErrorMessage, testApiEndpoints } from "@/lib/ipc";

function normalize(url: string) {
  return url.trim().replace(/\/+$/, "");
}

export function EndpointSpeedPanel({
  open,
  currentUrl,
  officialUrl,
  onApply,
  onClose,
}: {
  open: boolean;
  currentUrl: string;
  officialUrl: string;
  onApply: (url: string) => void;
  onClose: () => void;
}) {
  const initial = useMemo(() => {
    const urls = [officialUrl, currentUrl].map(normalize).filter(Boolean);
    return [...new Set(urls)];
  }, [officialUrl, currentUrl]);
  const [urls, setUrls] = useState<string[]>(initial);
  const [selected, setSelected] = useState(normalize(currentUrl) || initial[0] || "");
  const [customUrl, setCustomUrl] = useState("");
  const [latencies, setLatencies] = useState<Record<string, number | null>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [testing, setTesting] = useState(false);
  const [autoSelect, setAutoSelect] = useState(true);
  const [message, setMessage] = useState<string | null>(null);

  if (!open) return null;

  function addUrl() {
    const next = normalize(customUrl);
    if (!next) {
      setMessage("请填写完整 http(s) 地址");
      return;
    }
    if (!/^https?:\/\//i.test(next)) {
      setMessage("API 请求地址必须是 http(s) 完整 URL");
      return;
    }
    setUrls((current) => (current.includes(next) ? current : [...current, next]));
    setCustomUrl("");
    setMessage(null);
  }

  async function runTest() {
    setTesting(true);
    setMessage(null);
    try {
      const results = await testApiEndpoints(urls);
      const nextLatency: Record<string, number | null> = {};
      const nextErrors: Record<string, string> = {};
      let fastest: { url: string; ms: number } | null = null;
      for (const result of results) {
        const url = normalize(result.url);
        nextLatency[url] = result.latencyMs;
        if (result.error) nextErrors[url] = result.error;
        if (result.latencyMs != null && (fastest === null || result.latencyMs < fastest.ms)) {
          fastest = { url, ms: result.latencyMs };
        }
      }
      setLatencies(nextLatency);
      setErrors(nextErrors);
      if (autoSelect && fastest) setSelected(fastest.url);
    } catch (cause) {
      setMessage(ipcErrorMessage(cause, "测速失败，请稍后重试。"));
    } finally {
      setTesting(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/45 p-6 backdrop-blur-sm"
      role="presentation"
      onMouseDown={(event) => {
        event.stopPropagation();
        onClose();
      }}
    >
      <div
        className="flex max-h-[80vh] w-full max-w-lg flex-col rounded-q-card border border-q-border bg-q-surface-solid p-5 shadow-xl"
        role="dialog"
        aria-label="管理与测速"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-3">
          <div>
            <p className="text-lg font-semibold text-q-text-primary">管理与测速</p>
            <p className="mt-1 text-xs leading-relaxed text-q-text-secondary">
              对比额度查询地址的延迟，选中后写回「API 请求地址」。本应用只测连通性，不做请求代理。
            </p>
          </div>
          <Button variant="ghost" size="sm" onClick={onClose} aria-label="关闭">
            <X size={16} />
          </Button>
        </div>

        <div className="mt-4 flex items-center justify-between gap-3">
          <label className="flex items-center gap-1.5 text-xs text-q-text-secondary">
            <input
              type="checkbox"
              checked={autoSelect}
              onChange={(event) => setAutoSelect(event.target.checked)}
            />
            测速后选用最快
          </label>
          <Button size="sm" variant="secondary" disabled={testing || urls.length === 0} onClick={() => void runTest()}>
            {testing ? <LoaderCircle size={14} className="animate-spin" /> : <Zap size={14} aria-hidden />}
            {testing ? "测速中…" : "开始测速"}
          </Button>
        </div>

        <div className="mt-3 flex gap-2">
          <input
            value={customUrl}
            onChange={(event) => setCustomUrl(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                addUrl();
              }
            }}
            placeholder="添加备用 http(s) 地址"
            className="h-10 min-w-0 flex-1 rounded-q-control border border-q-border bg-q-surface px-3 text-sm text-q-text-primary outline-none focus:border-q-primary"
          />
          <Button type="button" variant="secondary" size="sm" onClick={addUrl} aria-label="添加地址">
            <Plus size={16} />
          </Button>
        </div>

        <div className="mt-3 min-h-0 flex-1 space-y-2 overflow-y-auto">
          {urls.map((url) => {
            const active = selected === url;
            const latency = latencies[url];
            const error = errors[url];
            return (
              <button
                key={url}
                type="button"
                onClick={() => setSelected(url)}
                className={`flex w-full items-center justify-between gap-3 rounded-q-control border px-3 py-2.5 text-left ${
                  active ? "border-q-border-selected bg-q-primary-softer" : "border-q-border hover:bg-q-surface-hover"
                }`}
              >
                <span className="min-w-0 flex-1 truncate text-sm text-q-text-primary">{url}</span>
                <span className="shrink-0 text-xs font-medium tabular-nums text-q-text-secondary">
                  {testing && latency == null && !error ? (
                    <LoaderCircle size={14} className="animate-spin" />
                  ) : latency != null ? (
                    <span className={latency < 300 ? "text-q-success" : latency < 800 ? "text-q-warning" : "text-q-danger"}>
                      {latency}ms
                    </span>
                  ) : error ? (
                    <span className="text-q-danger">{error}</span>
                  ) : (
                    "—"
                  )}
                </span>
              </button>
            );
          })}
        </div>

        {message && (
          <p className="mt-3 flex items-center gap-1.5 text-xs text-q-danger">
            <AlertCircle size={12} />
            {message}
          </p>
        )}

        <div className="mt-4 flex justify-end gap-2 border-t border-q-border pt-4">
          <Button variant="ghost" onClick={onClose}>
            取消
          </Button>
          <Button
            disabled={!selected}
            onClick={() => {
              onApply(selected);
              onClose();
            }}
          >
            使用此地址
          </Button>
        </div>
      </div>
    </div>
  );
}
