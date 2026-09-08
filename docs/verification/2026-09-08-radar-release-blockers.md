# 雷达发布阻塞修复验收

## 2026-09-08 发布前两项 P1 定向修复

- 历史结论修复改为 v3：停止按强信号推断已落地；仅恢复旧 v1/v2 写入的固定错误文案，优先读取同一分析 ID 的原始结果，缺失时标注依据缺失。逐条保存修改前后文案，不修改事件、证据或消费状态。
- 核心分析不再等待翻译。仅在 success/cached 后启动独立翻译任务；skipped/off/failed/cancelled 不启动。最多一批并发，单请求 10 秒、整批 30 秒，失败停止且不重试；更新后通知前端。
- 定向验证：`cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib conclusion_repair -- --test-threads=1`（3 通过）；`cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib enrichment_runs_after_success -- --test-threads=1`（1 通过）；`git diff --check` 通过。
- 按用户要求未跑全量测试、前端构建或发布打包。UI 和接口未改；未调用真实模型做联调。
- 回滚：revert 本次提交可回退代码；已执行 v3 的文案可从 `radar_repairs` 中 `repair:damaged_conclusions:v3:<analysisId>` 的 beforeConclusion/beforeBasis 恢复。先备份数据库，仅恢复仍等于该条 after 值的字段，避免覆盖后续修改；不需要回退 schema。

