import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, ChevronUp, Info, Monitor, Palette, RefreshCw, ShieldAlert } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/Button";
import { Switch } from "@/components/ui/Switch";
import {
  clearLocalCache,
  fetchAppSettings,
  fetchHoverbarPreferences,
  fetchPlatformSummaries,
  ipcErrorMessage,
  reorderPlatforms,
  setAppTheme,
  setAutostart,
  setHoverbarEnabled,
  setHoverbarSortMode,
  setRefreshInterval,
} from "@/lib/ipc";
import {
  APP_SETTINGS_QUERY_KEY,
  HOVERBAR_PREFERENCES_QUERY_KEY,
  PLATFORM_SUMMARIES_QUERY_KEY,
} from "@/lib/query-client";
import { applyAppTheme } from "@/lib/theme";
import { cn } from "@/lib/cn";

/**
 * 精简设置：常规（自启 / 主题 / 悬浮球开关）、悬浮球排序、刷新间隔、数据与关于。
 * 平台登录和 API Key 留在平台中心。
 */
export function SettingsPage() {
  const [section, setSection] = useState<SettingsSectionId>("general");

  return (
    <div className="flex min-h-0 flex-1 gap-4 p-6 pt-2">
      <SettingsSectionRail active={section} onSelect={setSection} />

      <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
        {section === "general" && <GeneralSection />}
        {section === "hoverbar" && <HoverbarSettingsSection />}
        {section === "refresh" && <RefreshSection />}
        {section === "data" && <DataSection />}
      </div>
    </div>
  );
}

type SettingsSectionId = "general" | "hoverbar" | "refresh" | "data";

const SECTIONS: Array<{ id: SettingsSectionId; label: string; icon: LucideIcon }> = [
  { id: "general", label: "常规与外观", icon: Palette },
  { id: "hoverbar", label: "悬浮球排序", icon: Monitor },
  { id: "refresh", label: "自动刷新", icon: RefreshCw },
  { id: "data", label: "数据与关于", icon: ShieldAlert },
];

function SettingsSectionRail({
  active,
  onSelect,
}: {
  active: SettingsSectionId;
  onSelect: (id: SettingsSectionId) => void;
}) {
  return (
    <nav
      aria-label="设置分区"
      className="flex w-[200px] shrink-0 flex-col gap-1 rounded-q-card border border-q-border bg-q-surface-muted/60 p-2 backdrop-blur-xl"
    >
      {SECTIONS.map((item) => {
        const Icon = item.icon;
        const selected = item.id === active;
        return (
          <button
            key={item.id}
            type="button"
            onClick={() => onSelect(item.id)}
            aria-current={selected ? "true" : undefined}
            className={cn(
              "flex cursor-pointer items-center gap-2.5 rounded-q-control px-3 py-2.5 text-left text-[13px] font-medium transition-colors duration-150",
              selected
                ? "bg-q-surface-solid text-q-primary shadow-q-sm"
                : "text-q-text-secondary hover:bg-q-surface-hover hover:text-q-text-primary",
            )}
          >
            <Icon size={16} aria-hidden className={selected ? "text-q-primary" : "text-q-text-muted"} />
            {item.label}
          </button>
        );
      })}
    </nav>
  );
}

function SettingRow({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 border-t border-q-border pt-4 first:border-t-0 first:pt-0">
      <div className="min-w-0">
        <p className="text-sm font-medium text-q-text-primary">{title}</p>
        <p className="mt-0.5 text-xs leading-relaxed text-q-text-secondary">{description}</p>
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function SectionHeader({ title, description }: { title: string; description: string }) {
  return (
    <div className="glass-panel flex flex-col gap-1 px-5 py-4">
      <h1 className="text-lg font-semibold tracking-tight text-q-text-primary">{title}</h1>
      <p className="max-w-2xl text-[13px] leading-relaxed text-q-text-secondary">{description}</p>
    </div>
  );
}

function useSettings() {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: APP_SETTINGS_QUERY_KEY,
    queryFn: fetchAppSettings,
  });

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: APP_SETTINGS_QUERY_KEY });
  };

  return { ...query, invalidate, queryClient };
}

function GeneralSection() {
  const { data: settings, queryClient } = useSettings();
  const { data: prefs } = useQuery({
    queryKey: HOVERBAR_PREFERENCES_QUERY_KEY,
    queryFn: fetchHoverbarPreferences,
  });

  const themeMutation = useMutation({
    mutationFn: setAppTheme,
    onSuccess: (next) => {
      applyAppTheme(next.theme);
      queryClient.setQueryData(APP_SETTINGS_QUERY_KEY, next);
    },
  });
  const autostartMutation = useMutation({
    mutationFn: setAutostart,
    onSuccess: (next) => queryClient.setQueryData(APP_SETTINGS_QUERY_KEY, next),
  });
  const hoverbarMutation = useMutation({
    mutationFn: setHoverbarEnabled,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: HOVERBAR_PREFERENCES_QUERY_KEY });
    },
  });

  const theme = settings?.theme ?? "system";
  const error =
    themeMutation.error ?? autostartMutation.error ?? hoverbarMutation.error;

  return (
    <>
      <SectionHeader title="常规与外观" description="启动行为、主题和悬浮球开关会立即保存。" />
      {error ? <ErrorText error={error} fallback="保存设置失败" /> : null}
      <div className="glass-panel flex flex-col gap-4 p-5">
        <SettingRow title="开机自启" description="登录 Windows 后自动启动本应用">
          <Switch
            checked={settings?.autostart ?? false}
            disabled={autostartMutation.isPending}
            onCheckedChange={(next) => autostartMutation.mutate(next)}
            label="开机自启"
          />
        </SettingRow>
        <SettingRow title="启用悬浮球" description="关闭后立即隐藏悬浮球与详情窗口">
          <Switch
            checked={prefs?.enabled ?? false}
            disabled={hoverbarMutation.isPending}
            onCheckedChange={(next) => hoverbarMutation.mutate(next)}
            label="启用悬浮球"
          />
        </SettingRow>
        <SettingRow title="主题" description="主窗口与悬浮详情一起切换；跟随系统时尊重 Windows 深浅色">
          <div className="inline-flex gap-1 rounded-q-control border border-q-border bg-q-surface-muted p-1">
            {[
              ["light", "浅色"],
              ["dark", "深色"],
              ["system", "跟随系统"],
            ].map(([id, label]) => (
              <button
                key={id}
                type="button"
                onClick={() => themeMutation.mutate(id)}
                className={cn(
                  "cursor-pointer rounded-[7px] px-3 py-1.5 text-[13px] font-medium",
                  theme === id
                    ? "bg-q-surface-solid text-q-primary shadow-q-sm"
                    : "text-q-text-secondary hover:text-q-text-primary",
                )}
              >
                {label}
              </button>
            ))}
          </div>
        </SettingRow>
      </div>
    </>
  );
}

function HoverbarSettingsSection() {
  const { data: settings, queryClient } = useSettings();
  const { data: platforms = [] } = useQuery({
    queryKey: PLATFORM_SUMMARIES_QUERY_KEY,
    queryFn: fetchPlatformSummaries,
  });
  const [order, setOrder] = useState<string[]>([]);

  useEffect(() => {
    setOrder(platforms.map((platform) => platform.providerId));
  }, [platforms]);

  const sortMutation = useMutation({
    mutationFn: setHoverbarSortMode,
    onSuccess: (next) => queryClient.setQueryData(APP_SETTINGS_QUERY_KEY, next),
  });
  const reorderMutation = useMutation({
    mutationFn: reorderPlatforms,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PLATFORM_SUMMARIES_QUERY_KEY });
    },
  });

  const mode = settings?.hoverbarSortMode === "smart" ? "smart" : "manual";
  const byId = new Map(platforms.map((platform) => [platform.providerId, platform]));

  const move = (id: string, direction: -1 | 1) => {
    const index = order.indexOf(id);
    const nextIndex = index + direction;
    if (index < 0 || nextIndex < 0 || nextIndex >= order.length) return;
    const next = [...order];
    const current = next[index];
    const swap = next[nextIndex];
    if (!current || !swap) return;
    next[index] = swap;
    next[nextIndex] = current;
    setOrder(next);
    reorderMutation.mutate(next);
  };

  return (
    <>
      <SectionHeader
        title="悬浮球排序"
        description="手动顺序会同步到主窗口平台列表和悬浮详情。智能排序把套餐制平台提前，组内仍按手动顺序。"
      />
      {(sortMutation.error || reorderMutation.error) && (
        <ErrorText error={sortMutation.error ?? reorderMutation.error} fallback="保存排序失败" />
      )}
      <div className="glass-panel flex flex-col gap-4 p-5">
        <SettingRow title="排序方式" description="智能排序优先展示 GPT、Claude、GLM、MiniMax 等套餐平台">
          <div className="inline-flex gap-1 rounded-q-control border border-q-border bg-q-surface-muted p-1">
            {[
              ["manual", "手动"],
              ["smart", "智能"],
            ].map(([id, label]) => (
              <button
                key={id}
                type="button"
                onClick={() => sortMutation.mutate(id as "manual" | "smart")}
                className={cn(
                  "cursor-pointer rounded-[7px] px-3 py-1.5 text-[13px] font-medium",
                  mode === id
                    ? "bg-q-surface-solid text-q-primary shadow-q-sm"
                    : "text-q-text-secondary hover:text-q-text-primary",
                )}
              >
                {label}
              </button>
            ))}
          </div>
        </SettingRow>
        <div>
          <p className="text-sm font-medium text-q-text-primary">平台顺序</p>
          <p className="mt-0.5 text-xs leading-relaxed text-q-text-secondary">
            用上下箭头调整。未接入的平台也会参与排序，但不会出现在悬浮详情里。
          </p>
          <div className="mt-3 space-y-2">
            {order.length === 0 && (
              <p className="text-xs text-q-text-muted">请先在平台中心添加平台。</p>
            )}
            {order.map((id, index) => {
              const platform = byId.get(id);
              if (!platform) return null;
              return (
                <div
                  key={id}
                  className="flex items-center justify-between gap-3 rounded-q-control border border-q-border bg-q-surface-muted/70 px-3 py-2"
                >
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium text-q-text-primary">{platform.displayName}</p>
                    <p className="truncate text-[11px] text-q-text-muted">{platform.accessSummary}</p>
                  </div>
                  <div className="flex gap-1">
                    <button
                      type="button"
                      aria-label={`上移 ${platform.displayName}`}
                      disabled={index === 0 || reorderMutation.isPending}
                      onClick={() => move(id, -1)}
                      className="inline-flex h-8 w-8 cursor-pointer items-center justify-center rounded-q-control border border-q-border text-q-text-secondary hover:bg-q-surface-hover disabled:cursor-not-allowed disabled:opacity-40"
                    >
                      <ChevronUp size={16} aria-hidden />
                    </button>
                    <button
                      type="button"
                      aria-label={`下移 ${platform.displayName}`}
                      disabled={index === order.length - 1 || reorderMutation.isPending}
                      onClick={() => move(id, 1)}
                      className="inline-flex h-8 w-8 cursor-pointer items-center justify-center rounded-q-control border border-q-border text-q-text-secondary hover:bg-q-surface-hover disabled:cursor-not-allowed disabled:opacity-40"
                    >
                      <ChevronDown size={16} aria-hidden />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </>
  );
}

function RefreshSection() {
  const { data: settings, queryClient } = useSettings();
  const mutation = useMutation({
    mutationFn: setRefreshInterval,
    onSuccess: (next) => queryClient.setQueryData(APP_SETTINGS_QUERY_KEY, next),
  });
  const minutes = settings?.refreshIntervalMinutes ?? 15;

  return (
    <>
      <SectionHeader
        title="自动刷新"
        description="仅刷新已配置 Source。关闭后只支持手动刷新；雷达检查始终需要手动触发。"
      />
      {mutation.error ? <ErrorText error={mutation.error} fallback="保存刷新间隔失败" /> : null}
      <div className="glass-panel flex flex-col gap-4 p-5">
        <SettingRow title="刷新间隔" description="5 / 15 / 30 分钟，或关闭自动刷新">
          <select
            value={String(minutes)}
            onChange={(event) => mutation.mutate(Number(event.target.value))}
            className="h-10 min-w-[140px] rounded-q-control border border-q-border bg-q-surface px-3 text-sm"
          >
            <option value="0">关闭</option>
            <option value="5">5 分钟</option>
            <option value="15">15 分钟</option>
            <option value="30">30 分钟</option>
          </select>
        </SettingRow>
      </div>
    </>
  );
}

function DataSection() {
  const queryClient = useQueryClient();
  const [message, setMessage] = useState<string | null>(null);
  const clearMutation = useMutation({
    mutationFn: clearLocalCache,
    onSuccess: () => {
      setMessage("已清除额度快照和刷新记录。凭据与平台配置仍保留。");
      void queryClient.invalidateQueries({ queryKey: PLATFORM_SUMMARIES_QUERY_KEY });
    },
    onError: (error) => {
      setMessage(ipcErrorMessage(error, "清除缓存失败"));
    },
  });

  return (
    <>
      <SectionHeader
        title="数据与关于"
        description="危险操作只出现在这里。清除缓存不会删除 API Key、Cookie 或本机 Codex 登录。"
      />
      <div className="glass-panel flex flex-col gap-4 p-5">
        <SettingRow
          title="清除本地缓存"
          description="删除额度快照与刷新历史，不影响凭据。下次刷新会重新拉取真实值。"
        >
          <Button
            variant="secondary"
            onClick={() => {
              if (!window.confirm("清除本地额度缓存？凭据和平台配置会保留。")) return;
              setMessage(null);
              clearMutation.mutate();
            }}
            disabled={clearMutation.isPending}
          >
            {clearMutation.isPending ? "清除中…" : "清除缓存"}
          </Button>
        </SettingRow>
        {message ? <p className="text-xs text-q-text-secondary">{message}</p> : null}
      </div>
      <div className="glass-panel flex flex-col gap-3 p-5">
        <div className="flex items-center gap-2 text-sm font-medium text-q-text-primary">
          <Info size={16} aria-hidden />
          关于
        </div>
        <div className="flex items-center justify-between text-sm">
          <span className="text-q-text-secondary">版本</span>
          <span className="font-medium text-q-text-primary" data-selectable="true">
            0.1.0
          </span>
        </div>
        <div className="flex items-center justify-between text-sm">
          <span className="text-q-text-secondary">第三方许可</span>
          <span className="text-q-text-primary">见仓库 THIRD_PARTY_NOTICES.md</span>
        </div>
      </div>
    </>
  );
}

function ErrorText({ error, fallback }: { error: unknown; fallback: string }) {
  return (
    <p className="rounded-q-control border border-q-danger/25 bg-q-danger-soft px-3 py-2 text-xs text-q-danger">
      {ipcErrorMessage(error, fallback)}
    </p>
  );
}
