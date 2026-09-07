import { Skeleton, SkeletonCard, SkeletonCircle, SkeletonText } from "./Skeleton";

/** 总览页加载骨架屏：与 Apple Glass V6/V7 总览结构 1:1 对齐 */
export function OverviewSkeleton() {
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4 pt-2 pr-2" aria-busy="true">
      {/* 页头骨架 */}
      <header className="flex items-center justify-between gap-4 px-1">
        <Skeleton rounded="sm" className="h-7 w-20" />
        <div className="flex items-center gap-3">
          <Skeleton rounded="pill" className="h-7 w-16" />
          <Skeleton rounded="sm" className="h-4 w-32" />
        </div>
      </header>

      {/* 四段状态条骨架 */}
      <section className="glass-panel flex flex-wrap items-center gap-x-10 gap-y-3 px-6 py-3.5">
        {[1, 2, 3, 4].map((i) => (
          <div key={i} className="flex items-center gap-2.5">
            <SkeletonCircle size={28} />
            <div className="flex items-baseline gap-1.5">
              <Skeleton rounded="sm" className="h-6 w-8" />
              <Skeleton rounded="sm" className="h-3.5 w-12" />
            </div>
          </div>
        ))}
      </section>

      {/* 关键平台卡片组骨架 */}
      <section className="flex flex-col gap-2.5">
        <div className="flex items-center justify-between px-1">
          <div className="flex items-center gap-2.5">
            <Skeleton rounded="sm" className="h-5 w-20" />
            <Skeleton rounded="sm" className="h-3.5 w-24" />
          </div>
          <div className="flex items-center gap-1.5">
            <Skeleton rounded="pill" className="h-6 w-6" />
            <Skeleton rounded="pill" className="h-6 w-6" />
          </div>
        </div>

        <div className="no-scrollbar flex items-stretch gap-[14px] overflow-x-hidden py-1 pl-0.5 pr-0.5">
          {[1, 2, 3, 4].map((i) => (
            <div
              key={i}
              className="glass-panel flex shrink-0 flex-col p-4"
              style={{ width: 258, height: 296 }}
            >
              {/* 卡片头部 */}
              <div className="flex items-center gap-2.5 pb-3">
                <Skeleton rounded="control" className="h-9 w-9 shrink-0" />
                <div className="flex flex-1 flex-col gap-1">
                  <Skeleton rounded="sm" className="h-4 w-24" />
                  <Skeleton rounded="pill" className="h-3 w-16" />
                </div>
              </div>

              {/* 卡片额度行 */}
              <div className="mt-2 flex flex-1 flex-col gap-3.5">
                <div className="flex flex-col gap-1.5">
                  <div className="flex justify-between">
                    <Skeleton rounded="sm" className="h-3 w-12" />
                    <Skeleton rounded="sm" className="h-3 w-10" />
                  </div>
                  <Skeleton rounded="full" className="h-1.5 w-full" />
                </div>
                <div className="flex flex-col gap-1.5">
                  <div className="flex justify-between">
                    <Skeleton rounded="sm" className="h-3 w-14" />
                    <Skeleton rounded="sm" className="h-3 w-10" />
                  </div>
                  <Skeleton rounded="full" className="h-1.5 w-full" />
                </div>
              </div>

              {/* 卡底资金与操作 */}
              <div className="mt-auto border-t border-q-border/60 pt-3">
                <div className="flex items-center justify-between">
                  <Skeleton rounded="sm" className="h-3.5 w-14" />
                  <Skeleton rounded="sm" className="h-5 w-20" />
                </div>
              </div>
            </div>
          ))}
        </div>
      </section>

      {/* 下方三栏骨架 */}
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(280px,0.95fr)_minmax(0,1.25fr)_minmax(0,1fr)]">
        <SkeletonCard className="h-64">
          <Skeleton rounded="sm" className="h-4 w-24 mb-3" />
          <div className="flex flex-col gap-2.5">
            {[1, 2, 3].map((i) => (
              <Skeleton rounded="control" key={i} className="h-12 w-full" />
            ))}
          </div>
        </SkeletonCard>

        <SkeletonCard className="h-64">
          <div className="flex items-center justify-between mb-4">
            <Skeleton rounded="sm" className="h-4 w-28" />
            <div className="flex gap-2">
              <Skeleton rounded="control" className="h-6 w-20" />
              <Skeleton rounded="control" className="h-6 w-20" />
            </div>
          </div>
          <Skeleton rounded="control" className="h-40 w-full" />
        </SkeletonCard>

        <SkeletonCard className="h-64">
          <div className="flex items-center justify-between mb-4">
            <Skeleton rounded="sm" className="h-4 w-20" />
            <Skeleton rounded="sm" className="h-3.5 w-16" />
          </div>
          <Skeleton rounded="control" className="h-40 w-full" />
        </SkeletonCard>
      </div>

      {/* 最近刷新记录骨架 */}
      <SkeletonCard className="h-44">
        <Skeleton rounded="sm" className="h-4 w-28 mb-3" />
        <div className="flex flex-col gap-2">
          {[1, 2, 3].map((i) => (
            <Skeleton rounded="sm" key={i} className="h-6 w-full" />
          ))}
        </div>
      </SkeletonCard>
    </div>
  );
}

/** 平台中心加载骨架屏 */
export function PlatformCenterSkeleton() {
  return (
    <div className="flex min-h-0 min-w-0 flex-1 p-4 pt-2 gap-3" aria-busy="true">
      {/* 左侧平台目录栏 */}
      <aside className="hidden md:flex w-52 shrink-0 flex-col gap-2 rounded-q-card border border-q-border bg-q-surface p-2.5">
        <div className="flex items-center justify-between px-1 pb-1">
          <Skeleton rounded="sm" className="h-4 w-16" />
          <Skeleton rounded="pill" className="h-6 w-6" />
        </div>
        {[1, 2, 3, 4].map((i) => (
          <div key={i} className="flex items-center gap-2.5 rounded-q-control p-2">
            <Skeleton rounded="control" className="h-8 w-8 shrink-0" />
            <div className="flex flex-1 flex-col gap-1">
              <Skeleton rounded="sm" className="h-3.5 w-16" />
              <Skeleton rounded="sm" className="h-2.5 w-10" />
            </div>
          </div>
        ))}
      </aside>

      {/* 右侧主区域 */}
      <main className="flex min-h-0 min-w-0 flex-1 flex-col gap-3 overflow-y-auto pr-1">
        {/* 头部面板 */}
        <header className="glass-panel flex flex-col gap-3 p-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <Skeleton rounded="control" className="h-12 w-12 shrink-0" />
              <div className="flex flex-col gap-1.5">
                <div className="flex items-center gap-2">
                  <Skeleton rounded="sm" className="h-5 w-28" />
                  <Skeleton rounded="pill" className="h-4 w-16" />
                </div>
                <Skeleton rounded="sm" className="h-3.5 w-44" />
              </div>
            </div>
            <div className="flex items-center gap-2">
              <Skeleton rounded="control" className="h-8 w-18" />
              <Skeleton rounded="control" className="h-8 w-18" />
            </div>
          </div>
          {/* Tabs */}
          <div className="flex gap-2 pt-2 border-t border-q-border/60">
            <Skeleton rounded="pill" className="h-7 w-24" />
            <Skeleton rounded="pill" className="h-7 w-24" />
          </div>
        </header>

        {/* 能力展示网格 */}
        <div className="flex flex-col gap-3">
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            <SkeletonCard className="h-40">
              <Skeleton rounded="sm" className="h-4 w-24 mb-3" />
              <Skeleton rounded="full" className="h-2 w-full mb-3" />
              <SkeletonText lines={2} />
            </SkeletonCard>
            <SkeletonCard className="h-40">
              <Skeleton rounded="sm" className="h-4 w-20 mb-3" />
              <Skeleton rounded="sm" className="h-7 w-32 mb-2" />
              <SkeletonText lines={2} />
            </SkeletonCard>
          </div>
          <SkeletonCard className="h-52">
            <Skeleton rounded="sm" className="h-4 w-28 mb-3" />
            <div className="flex flex-col gap-2.5">
              {[1, 2, 3].map((i) => (
                <Skeleton rounded="control" key={i} className="h-9 w-full" />
              ))}
            </div>
          </SkeletonCard>
        </div>
      </main>
    </div>
  );
}

/** GPT 重置雷达加载骨架屏 */
export function RadarSkeleton() {
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4 pt-2 pr-2" aria-busy="true">
      {/* 页头与动态范围 */}
      <header className="flex items-center justify-between gap-4 px-1">
        <div className="flex items-center gap-3">
          <Skeleton rounded="sm" className="h-6 w-32" />
          <Skeleton rounded="pill" className="h-5 w-16" />
        </div>
        <div className="flex items-center gap-1.5">
          <Skeleton rounded="pill" className="h-7 w-14" />
          <Skeleton rounded="pill" className="h-7 w-16" />
          <Skeleton rounded="pill" className="h-7 w-16" />
          <Skeleton rounded="pill" className="h-7 w-20" />
        </div>
      </header>

      {/* 三状态卡网格 */}
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        <SkeletonCard className="h-32">
          <Skeleton rounded="sm" className="h-3.5 w-20 mb-2" />
          <Skeleton rounded="sm" className="h-6 w-36 mb-2" />
          <Skeleton rounded="sm" className="h-3 w-28 mt-auto" />
        </SkeletonCard>
        <SkeletonCard className="h-32">
          <Skeleton rounded="sm" className="h-3.5 w-24 mb-2" />
          <Skeleton rounded="sm" className="h-6 w-32 mb-2" />
          <Skeleton rounded="sm" className="h-3 w-20 mt-auto" />
        </SkeletonCard>
        <SkeletonCard className="h-32">
          <Skeleton rounded="sm" className="h-3.5 w-20 mb-2" />
          <Skeleton rounded="sm" className="h-5 w-28 mb-2" />
          <Skeleton rounded="sm" className="h-3 w-32 mt-auto" />
        </SkeletonCard>
      </div>

      {/* 公告细条骨架 */}
      <Skeleton rounded="control" className="h-9 w-full" />

      {/* AI 分析主卡骨架 */}
      <SkeletonCard className="h-56">
        <div className="flex items-center justify-between mb-3">
          <Skeleton rounded="sm" className="h-4 w-28" />
          <Skeleton rounded="pill" className="h-5 w-16" />
        </div>
        <Skeleton rounded="sm" className="h-5 w-3/4 mb-3" />
        <SkeletonText lines={3} className="mb-4" />
        <div className="flex gap-2 mt-auto">
          <Skeleton rounded="pill" className="h-6 w-24" />
          <Skeleton rounded="pill" className="h-6 w-32" />
        </div>
      </SkeletonCard>
    </div>
  );
}
