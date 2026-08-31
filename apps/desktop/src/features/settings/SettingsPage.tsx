import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { GripVertical, Info, Monitor, Palette, RefreshCw, ShieldAlert } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
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
  setHoverbarAutoRadarCheck,
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
import { PlatformMark } from "@/features/platform-center/ProviderRail";

/**
 * 精简设置：常规（自启 / 主题 / 悬浮球开关）、悬浮球排序、刷新间隔、数据与关于。
 * 平台登录和 API Key 留在平台中心。
 */
export function SettingsPage() {
  const [section, setSection] = useState<SettingsSectionId>("general");

  return (
    <div className="flex min-h-0 flex-1 gap-4 p-4 pt-2">
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
      className="flex w-[208px] shrink-0 flex-col gap-1 rounded-[18px] border border-q-border bg-q-surface p-2.5 shadow-q-sm backdrop-blur-xl"
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
  const autoRadarMutation = useMutation({
    mutationFn: setHoverbarAutoRadarCheck,
    onSuccess: (next) => queryClient.setQueryData(APP_SETTINGS_QUERY_KEY, next),
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
        <SettingRow
          title="展开悬浮详情时自动检查重置雷达"
          description="悬浮球展开详情时自动同步 Tibo 动态并按偏好运行 AI 分析；距上次检查不足 10 分钟时跳过"
        >
          <Switch
            checked={settings?.hoverbarAutoRadarCheck ?? false}
            disabled={autoRadarMutation.isPending}
            onCheckedChange={(next) => autoRadarMutation.mutate(next)}
            label="展开悬浮详情时自动检查重置雷达"
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

  // 垂直列表拖拽排序：被拖行跟随指针，其余行按目标位让位；松手一次性提交顺序。
  // 与主窗口卡组同款思路：拖拽期间零渲染（样式命令式写入），capture 丢失也不会挂起。
  type RowDrag = { id: string; startIndex: number; startY: number; target: number; step: number };
  const rowDrag = useRef<RowDrag | null>(null);
  const rowRefs = useRef(new Map<string, HTMLDivElement>());
  const commitRef = useRef<(next: string[]) => void>(() => {});

  const applyRowShifts = (state: RowDrag, nextTarget: number) => {
    if (nextTarget === state.target) return;
    state.target = nextTarget;
    for (let i = 0; i < order.length; i++) {
      const oid = order[i];
      if (oid === state.id) continue;
      const el = rowRefs.current.get(oid);
      if (!el) continue;
      const shift =
        nextTarget > state.startIndex && i > state.startIndex && i <= nextTarget
          ? -state.step
          : nextTarget < state.startIndex && i >= nextTarget && i < state.startIndex
            ? state.step
            : 0;
      el.style.transition = "transform 160ms cubic-bezier(0.2, 0.78, 0.24, 1)";
      el.style.transform = shift !== 0 ? `translateY(${shift}px, 0)` : "translateY(0px, 0)";
    }
  };

  const onRowPointerDown = (event: React.PointerEvent<HTMLDivElement>, id: string) => {
    if (event.button !== 0) return;
    const row = rowRefs.current.get(id);
    const index = order.indexOf(id);
    if (!row || index < 0 || rowDrag.current) return;
    const rowH = row.getBoundingClientRect().height;
    const nextRow = rowRefs.current.get(order[index + 1] ?? "") ?? rowRefs.current.get(order[index - 1] ?? "");
    const step = Math.max(
      rowH + 4,
      nextRow ? Math.abs(nextRow.getBoundingClientRect().top - row.getBoundingClientRect().top) : rowH + 8,
    );
    rowDrag.current = { id, startIndex: index, startY: event.clientY, target: index, step };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const onRowPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const state = rowDrag.current;
    const row = state ? rowRefs.current.get(state.id) : null;
    if (!state || !row) return;
    const dy = event.clientY - state.startY;
    if (Math.abs(dy) < 4 && state.target === state.startIndex) return;
    row.classList.add("sort-row-dragging");
    row.style.transition = "none";
    row.style.transform = "translateY(" + dy + "px, 0)";
    row.style.zIndex = "20";
    const rawTarget = Math.max(0, Math.min(order.length - 1, state.startIndex + Math.round(dy / state.step)));
    applyRowShifts(state, rawTarget);
  };

  const settleRowDrag = (commit: boolean) => {
    const state = rowDrag.current;
    rowDrag.current = null;
    if (!state) return;
    const row = rowRefs.current.get(state.id);
    if (row) {
      row.style.transition = "";
      row.style.transform = "";
      row.style.zIndex = "";
      row.classList.remove("sort-row-dragging");
    }
    for (const el of rowRefs.current.values()) {
      el.style.transition = "";
      el.style.transform = "";
      el.style.zIndex = "";
    }
    if (!commit || state.target === state.startIndex) return;
    const next = [...order];
    const moved = next.splice(state.startIndex, 1)[0];
    if (!moved) return;
    next.splice(state.target, 0, moved);
    setOrder(next);
    commitRef.current(next);
  };

  commitRef.current = (next: string[]) => reorderMutation.mutate(next);

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
            按住平台行拖动调整顺序。未接入的平台也会参与排序，但不会出现在悬浮详情里。
          </p>
          <div className="mt-3 space-y-2" data-sort-rows>
            {order.length === 0 && (
              <p className="text-xs text-q-text-muted">请先在平台中心添加平台。</p>
            )}
            {order.map((id) => {
              const platform = byId.get(id);
              if (!platform) return null;
              return (
                <div
                  key={id}
                  ref={(el) => {
                    if (el) rowRefs.current.set(id, el);
                    else rowRefs.current.delete(id);
                  }}
                  onPointerDown={(event) => onRowPointerDown(event, id)}
                  onPointerMove={onRowPointerMove}
                  onPointerUp={() => settleRowDrag(true)}
                  onPointerCancel={() => settleRowDrag(false)}
                  className="flex cursor-grab select-none items-center gap-3 rounded-q-control border border-q-border bg-q-surface-muted px-3 py-2"
                  title="拖动调整顺序"
                >
                  <GripVertical size={15} aria-hidden className="shrink-0 text-q-text-muted" />
                  <div className="flex min-w-0 flex-1 items-center gap-2.5">
                    <PlatformMark providerId={id} size={30} />
                    <div className="min-w-0">
                      <p className="truncate text-sm font-medium text-q-text-primary">{platform.displayName}</p>
                      <p className="truncate text-[11px] text-q-text-muted">{platform.accessSummary}</p>
                    </div>
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
  const [confirmClear, setConfirmClear] = useState(false);
  const clearMutation = useMutation({
    mutationFn: clearLocalCache,
    onSuccess: () => {
      setConfirmClear(false);
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
              setMessage(null);
              setConfirmClear(true);
            }}
            disabled={clearMutation.isPending}
          >
            {clearMutation.isPending ? "清除中…" : "清除缓存"}
          </Button>
        </SettingRow>
        {message ? <p className="text-xs text-q-text-secondary">{message}</p> : null}
      </div>
      {confirmClear && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-6 backdrop-blur-sm"
          role="presentation"
          onMouseDown={() => {
            if (!clearMutation.isPending) setConfirmClear(false);
          }}
        >
          <div
            className="w-full max-w-sm rounded-[18px] border border-q-border bg-q-surface-solid p-5 shadow-q-lg"
            role="dialog"
            aria-modal="true"
            aria-label="清除本地缓存"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <p className="text-sm font-semibold text-q-text-primary">清除本地额度缓存？</p>
            <p className="mt-2 text-xs leading-relaxed text-q-text-secondary">
              将删除额度快照与刷新历史；凭据和平台配置会保留，下次刷新会重新拉取真实值。
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <Button variant="ghost" size="sm" onClick={() => setConfirmClear(false)} disabled={clearMutation.isPending}>
                取消
              </Button>
              <Button size="sm" onClick={() => clearMutation.mutate()} disabled={clearMutation.isPending}>
                {clearMutation.isPending ? "清除中…" : "确认清除"}
              </Button>
            </div>
          </div>
        </div>
      )}
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
