## 改动说明

（做了什么，为什么）

## 是否解决了对应 Issue

- [ ] 解决 #（编号）

## 验证

勾选并填写：

- [ ] `pnpm --filter @ai-quota-monitor/desktop typecheck` 通过
- [ ] `pnpm --filter @ai-quota-monitor/desktop build` 通过
- [ ] `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml` 通过
- [ ] 手工页面验证（说明：……）
- [ ] 涉及新平台/接口，已在 `docs/product/platform-access.md` 或文件头注释中记录来源

## 截图

（如为界面改动，请附前后对比或效果图）

## 说明（危险边界）

涉及金额精度、凭据、网络超时/重试、并发刷新的改动，请说明已识别的边界。
