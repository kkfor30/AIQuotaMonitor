/**
 * 迁移来源：DeepSeekMonitorWindows-final/src/hoverbar-interaction.ts（提交 f3ab3ec6，MIT）
 * 阻止原生图片/资源拖拽与窗口拖动冲突。
 */
export function preventNativeAssetDrag(event: { preventDefault(): void }): void {
  event.preventDefault();
}
