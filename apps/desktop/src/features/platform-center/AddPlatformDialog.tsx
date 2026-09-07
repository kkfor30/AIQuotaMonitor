import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2 } from "lucide-react";
import { Button } from "@/components/ui/Button";
import {
  addUserPlatforms,
  fetchPlatformCatalog,
  ipcErrorMessage,
} from "@/lib/ipc";
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

  const items = catalogQuery.data ?? [];
  const unaddedItems = items.filter((item) => !item.added);

  const addMutation = useMutation({
    mutationFn: async () => {
      const selectedItems = unaddedItems.filter((item) => selected.includes(item.id));
      const newPlatformIds = selectedItems.map((item) => item.id);
      if (newPlatformIds.length === 0) return [];
      const platforms = await addUserPlatforms(newPlatformIds);
      return platforms ?? [];
    },
    onSuccess: (platforms) => {
      queryClient.setQueryData(PLATFORM_SUMMARIES_QUERY_KEY, platforms);
      void queryClient.invalidateQueries({ queryKey: ["platform-catalog"] });
      onAdded(selected);
      setSelected([]);
      onClose();
    },
  });

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-6 backdrop-blur-sm animate-fade-in" role="presentation" onMouseDown={onClose}>
      <div
        className="flex max-h-[80vh] w-full max-w-xl flex-col rounded-[18px] border border-q-border bg-q-surface-solid p-5 shadow-q-lg animate-scale-in"
        role="dialog"
        aria-label="添加平台"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <p className="text-lg font-semibold text-q-text-primary">添加平台</p>
        <p className="mt-1 text-xs leading-relaxed text-q-text-secondary">
          选择要监控的新平台。添加后会预填官网链接和官方 API 请求地址，再填写 API Key 并验证连接。如需配置已有平台的多账号，请在对应平台的「接入与来源」中操作。
        </p>
        <div className="mt-4 min-h-0 flex-1 space-y-1 overflow-y-auto">
          {unaddedItems.length === 0 ? (
            <div className="flex flex-col items-center justify-center gap-2 py-10 text-center">
              <span className="flex h-10 w-10 items-center justify-center rounded-full bg-q-success-soft text-q-success">
                <CheckCircle2 size={20} />
              </span>
              <p className="text-[14px] font-semibold text-q-text-primary">所有支持的平台均已添加</p>
              <p className="max-w-xs text-xs text-q-text-muted">
                您已添加当前支持的全部平台。如需为现有平台配置多账号，请前往对应平台的「接入与来源」进行添加。
              </p>
            </div>
          ) : (
            unaddedItems.map((item) => {
              const checked = selected.includes(item.id);
              return (
                <label
                  key={item.id}
                  className="flex cursor-pointer items-center gap-3 rounded-q-control px-3 py-2.5 transition-colors hover:bg-q-surface-hover"
                >
                  <PlatformMark providerId={item.id} size={32} />
                  <div className="min-w-0 flex-1">
                    <p className="text-sm font-medium text-q-text-primary">{item.displayName}</p>
                    <p className="text-xs text-q-text-muted">{item.accessHint}</p>
                  </div>
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={(event) => {
                      setSelected((current) =>
                        event.target.checked ? [...current, item.id] : current.filter((id) => id !== item.id),
                      );
                    }}
                  />
                </label>
              );
            })
          )}
        </div>
        {addMutation.error && (
          <p className="mt-3 text-xs text-q-danger">{ipcErrorMessage(addMutation.error, "添加平台失败")}</p>
        )}
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            取消
          </Button>
          <Button
            onClick={() => addMutation.mutate()}
            disabled={selected.length === 0 || addMutation.isPending || unaddedItems.length === 0}
          >
            {addMutation.isPending ? "添加中…" : "添加所选平台"}
          </Button>
        </div>
      </div>
    </div>
  );
}
