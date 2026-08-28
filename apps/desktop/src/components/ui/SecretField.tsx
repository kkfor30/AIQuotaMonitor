import { useState } from "react";
import { Copy, Eye, EyeOff } from "lucide-react";
import { Button } from "./Button";

async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    const field = document.createElement("textarea");
    field.value = text;
    field.style.position = "fixed";
    field.style.left = "-9999px";
    document.body.appendChild(field);
    field.select();
    document.execCommand("copy");
    document.body.removeChild(field);
  }
}

export function SecretField({
  label,
  value,
  onChange,
  placeholder,
  helpText,
  configured,
  onReveal,
  disabled = false,
}: {
  label: string;
  value: string;
  onChange: (next: string) => void;
  placeholder: string;
  helpText?: string;
  configured: boolean;
  onReveal: () => Promise<string>;
  disabled?: boolean;
}) {
  const [visible, setVisible] = useState(false);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function ensureSecret() {
    if (value.trim()) return value;
    if (!configured) throw new Error("尚未保存凭据");
    return onReveal();
  }

  async function toggleVisible() {
    setError(null);
    if (visible) {
      setVisible(false);
      return;
    }
    if (value.trim()) {
      setVisible(true);
      return;
    }
    setBusy(true);
    try {
      const secret = await ensureSecret();
      onChange(secret);
      setVisible(true);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  async function handleCopy() {
    setError(null);
    setBusy(true);
    try {
      const secret = await ensureSecret();
      if (!value.trim()) onChange(secret);
      await copyText(secret);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <label className="flex flex-col gap-1.5 text-sm font-medium text-q-text-primary">
      {label}
      <div className="flex gap-2">
        <input
          type={visible ? "text" : "password"}
          autoComplete="off"
          value={value}
          disabled={disabled}
          onChange={(event) => onChange(event.target.value)}
          placeholder={configured ? placeholder : placeholder}
          className="h-10 min-w-0 flex-1 rounded-q-control border border-q-border bg-q-surface px-3 text-sm font-normal text-q-text-primary outline-none focus:border-q-primary"
        />
        <Button
          type="button"
          variant="secondary"
          size="sm"
          aria-label={visible ? "隐藏凭据" : "显示凭据"}
          title={visible ? "隐藏" : "显示"}
          disabled={disabled || busy || (!configured && !value.trim())}
          onClick={() => void toggleVisible()}
        >
          {visible ? <EyeOff size={14} aria-hidden /> : <Eye size={14} aria-hidden />}
        </Button>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          aria-label="复制凭据"
          title={copied ? "已复制" : "复制"}
          disabled={disabled || busy || (!configured && !value.trim())}
          onClick={() => void handleCopy()}
        >
          <Copy size={14} aria-hidden />
        </Button>
      </div>
      {helpText ? <p className="text-xs font-normal leading-relaxed text-q-text-muted">{helpText}</p> : null}
      {copied ? <p className="text-xs font-normal text-q-success">已复制到剪贴板</p> : null}
      {error ? <p className="text-xs font-normal text-q-danger">{error}</p> : null}
    </label>
  );
}
