import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/Button";
import { addUserPlatforms, fetchPlatformCatalog, ipcErrorMessage } from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import { PlatformMark } from "./ProviderRail";

export function AddPlatformDialog({
  open,
  onClose,
  onAdded,
}: {
  open: boolean;
  onClose: () => void;
  onAdded: (platformIds: string[]) => void;
}) {
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<string[]>([]);
  const catalogQuery = useQuery({
    queryKey: ["platform-catalog"],
    queryFn: fetchPlatformCatalog,
    enabled: open,
  });
  const addMutation = useMutation({
    mutationFn: () => addUserPlatforms(selected),
    onSuccess: (platforms) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, platforms);
      void queryClient.invalidateQueries({ queryKey: ["platform-catalog"] });
      onAdded(selected);
      setSelected([]);
      onClose();
    },
  });

  if (!open) return null;
  const items = catalogQuery.data ?? [];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-6 backdrop-blur-sm" role="presentation" onMouseDown={onClose}>
      <div
        className="flex max-h-[80vh] w-full max-w-xl flex-col rounded-q-card border border-q-border bg-q-surface-solid p-5 shadow-xl"
        role="dialog"
        aria-label="添加平台"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <p className="text-lg font-semibold text-q-text-primary">添加平台</p>
        <p className="mt-1 text-xs leading-relaxed text-q-text-secondary">
          选择要监控的平台。添加后会预填官网链接和官方 API 请求地址，再填写 API Key 并验证连接。不支持注册表以外的中转站。
        </p>
        <div className="mt-4 min-h-0 flex-1 space-y-1 overflow-y-auto">
          {items.map((item) => {
            const checked = selected.includes(item.id) || item.added;
            return (
              <label
                key={item.id}
                className={`flex cursor-pointer items-center gap-3 rounded-q-control px-3 py-2.5 ${
                  item.added ? "opacity-50" : "hover:bg-q-surface-hover"
                }`}
              >
                <PlatformMark providerId={item.id} size={32} />
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium text-q-text-primary">{item.displayName}</p>
                  <p className="text-xs text-q-text-muted">{item.accessHint}</p>
                </div>
                <input
                  type="checkbox"
                  checked={checked}
                  disabled={item.added}
                  onChange={(event) => {
                    setSelected((current) =>
                      event.target.checked ? [...current, item.id] : current.filter((id) => id !== item.id),
                    );
                  }}
                />
              </label>
            );
          })}
        </div>
        {addMutation.error && (
          <p className="mt-3 text-xs text-q-danger">{ipcErrorMessage(addMutation.error, "添加平台失败")}</p>
        )}
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            取消
          </Button>
          <Button onClick={() => addMutation.mutate()} disabled={selected.length === 0 || addMutation.isPending}>
            {addMutation.isPending ? "添加中…" : "添加所选平台"}
          </Button>
        </div>
      </div>
    </div>
  );
}
