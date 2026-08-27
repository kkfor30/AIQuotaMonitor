import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Info, Monitor, Palette, RefreshCw, ShieldAlert, Sparkles } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useState } from "react";
import { Switch } from "@/components/ui/Switch";
import { fetchHoverbarPreferences, setHoverbarEnabled } from "@/lib/ipc";
import { HOVERBAR_PREFERENCES_QUERY_KEY } from "@/lib/query-client";
import { cn } from "@/lib/cn";

/**
 * 设置（product-shell-v5，设计稿 08）：
 * 左侧二级分区导航 + 右侧内容区。
 * 平台登录和 API Key 不进入设置，统一留在「平台中心 / 接入与来源」；
 * 危险操作只出现在「数据与隐私」。
 */
export function SettingsPage() {
  const [section, setSection] = useState<SettingsSectionId>("general");

  return (
    <div className="flex min-h-0 flex-1 gap-4 p-6 pt-2">
      <SettingsSectionRail active={section} onSelect={setSection} />

      <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
        {section === "general" && <GeneralSection />}
        {section === "appearance" && <AppearanceSection />}
        {section === "hoverbar" && <HoverbarSettingsSection />}
        {section === "refresh" && <RefreshSection />}
        {section === "privacy" && <PrivacySection />}
        {section === "about" && <AboutSection />}
      </div>
    </div>
  );
}

type SettingsSectionId = "general" | "appearance" | "hoverbar" | "refresh" | "privacy" | "about";

const SECTIONS: Array<{ id: SettingsSectionId; label: string; icon: LucideIcon }> = [
  { id: "general", label: "常规", icon: Sparkles },
  { id: "appearance", label: "外观", icon: Palette },
  { id: "hoverbar", label: "悬浮球", icon: Monitor },
  { id: "refresh", label: "刷新与通知", icon: RefreshCw },
  { id: "privacy", label: "数据与隐私", icon: ShieldAlert },
  { id: "about", label: "关于", icon: Info },
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
                ? "bg-white text-q-primary shadow-q-sm"
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

/** 通用设置行容器。 */
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

function PendingTag() {
  return (
    <span className="rounded-q-pill bg-q-neutral-soft px-2.5 py-1 text-xs text-q-neutral">
      后续版本
    </span>
  );
}

function GeneralSection() {
  return (
    <>
      <SectionHeader title="常规" description="启动行为、语言与通用偏好。设置项默认自动保存。" />
      <div className="glass-panel flex flex-col gap-4 p-5">
        <SettingRow title="开机自启" description="登录 Windows 后自动启动并显示主窗口">
          <PendingTag />
        </SettingRow>
        <SettingRow title="启动时显示" description="主窗口或仅悬浮球">
          <PendingTag />
        </SettingRow>
        <SettingRow title="界面语言" description="跟随系统或手动指定">
          <PendingTag />
        </SettingRow>
      </div>
    </>
  );
}

function AppearanceSection() {
  return (
    <>
      <SectionHeader title="外观" description="主题、密度与动效偏好。" />
      <div className="glass-panel flex flex-col gap-4 p-5">
        <SettingRow title="主题" description="浅色 / 深色 / 跟随系统；悬浮球主题将随之联动">
          <PendingTag />
        </SettingRow>
        <SettingRow title="动效" description="悬浮球展开与页面过渡动画，遵循系统减弱动态设置">
          <PendingTag />
        </SettingRow>
      </div>
    </>
  );
}

/** 悬浮球分区（设计稿 08）：开关、停靠、触发与排序。 */
function HoverbarSettingsSection() {
  const queryClient = useQueryClient();
  const { data: prefs } = useQuery({
    queryKey: HOVERBAR_PREFERENCES_QUERY_KEY,
    queryFn: fetchHoverbarPreferences,
  });

  const toggleMutation = useMutation({
    mutationFn: (enabled: boolean) => setHoverbarEnabled(enabled),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: HOVERBAR_PREFERENCES_QUERY_KEY });
    },
  });

  const enabled = prefs?.enabled ?? false;

  return (
    <>
      <SectionHeader
        title="悬浮球"
        description="桌面悬浮球独立于主窗口运行：拖动重新停靠、悬停展开平台速览；全屏应用时自动隐藏。"
      />
      <div className="glass-panel flex flex-col gap-4 p-5">
        <SettingRow
          title="启用悬浮球"
          description="关闭后立即隐藏悬浮球与详情窗口，偏好保存并在下次启动时生效"
        >
          <Switch
            checked={enabled}
            disabled={toggleMutation.isPending}
            onCheckedChange={(next) => toggleMutation.mutate(next)}
            label="启用悬浮球"
          />
        </SettingRow>
        <SettingRow
          title="停靠边缘与显示器"
          description="直接拖动悬浮球即可重新停靠任意屏幕边缘；固定显示器选择将在后续版本提供"
        >
          <PendingTag />
        </SettingRow>
        <SettingRow
          title="展开触发"
          description="悬停延迟与点击展开行为"
        >
          <PendingTag />
        </SettingRow>
        <SettingRow
          title="平台排序"
          description="手动排序与智能排序（套餐平台优先）；排序保存后实时同步到悬浮球"
        >
          <PendingTag />
        </SettingRow>
      </div>
    </>
  );
}

function RefreshSection() {
  return (
    <>
      <SectionHeader title="刷新与通知" description="各平台刷新频率、失败重试与通知策略。" />
      <div className="glass-panel flex flex-col gap-4 p-5">
        <SettingRow title="自动刷新" description="按平台独立配置刷新间隔；仅刷新已配置 Source">
          <PendingTag />
        </SettingRow>
        <SettingRow title="余额预警" description="低于阈值时通过系统通知提醒">
          <PendingTag />
        </SettingRow>
      </div>
    </>
  );
}

function PrivacySection() {
  return (
    <>
      <SectionHeader
        title="数据与隐私"
        description="本地数据与凭据管理。危险操作仅出现在本分区，执行前均需二次确认。"
      />
      <div className="glass-panel flex flex-col gap-4 p-5">
        <SettingRow title="凭据存储" description="API Key 与会话凭据保存在 Windows 安全存储，不进入明文文件">
          <PendingTag />
        </SettingRow>
        <SettingRow title="清除本地缓存" description="删除快照与刷新记录（不影响凭据），操作需二次确认">
          <PendingTag />
        </SettingRow>
        <SettingRow title="重置全部数据" description="危险操作：清除全部配置、凭据与历史，不可恢复">
          <span className="rounded-q-pill bg-q-danger-soft px-2.5 py-1 text-xs font-medium text-q-danger">
            后续版本 · 高风险
          </span>
        </SettingRow>
      </div>
    </>
  );
}

function AboutSection() {
  return (
    <>
      <SectionHeader title="关于" description="版本、开源许可与更新。" />
      <div className="glass-panel flex flex-col gap-3 p-5">
        <div className="flex items-center justify-between text-sm">
          <span className="text-q-text-secondary">版本</span>
          <span className="font-medium text-q-text-primary" data-selectable="true">0.1.0（阶段一骨架）</span>
        </div>
        <div className="flex items-center justify-between text-sm">
          <span className="text-q-text-secondary">第三方许可</span>
          <span className="text-q-text-primary">见仓库 THIRD_PARTY_NOTICES.md</span>
        </div>
      </div>
    </>
  );
}
