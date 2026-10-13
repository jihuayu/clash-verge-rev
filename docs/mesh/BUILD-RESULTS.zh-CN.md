# 本地构建结果（2026-10-13）

已生成 macOS arm64 `.app`，版本 2.5.3，bundle ID 为 `io.github.clash-verge-rev.clash-verge-rev.coexistence-test`。使用 `fast-release` profile、两个 Rust 编译任务和专用 `coexistence-test` feature。未启动应用、安装服务或改动 OS 网络。

## 可复现步骤

在隔离 checkout 中安装锁定依赖（开发时使用 `pnpm install --frozen-lockfile --ignore-scripts`），执行 `node scripts/prebuild.mjs aarch64-apple-darwin` 下载项目既定资源。本次稳定核心 v1.19.32、alpha 核心 alpha-bc76c13、service IPC v2.6.3；这些下载没有执行安装器。发布前应重新核验外部二进制供应链来源。

清除本次构建子进程中的 `APPLE_*` 和 `TAURI_SIGNING_*` 环境变量，避免继承签名/公证凭据。然后运行：

```sh
CARGO_BUILD_JOBS=2 node node_modules/@tauri-apps/cli/tauri.js build \
  --features coexistence-test \
  --config src-tauri/tauri.coexistence-test.conf.json \
  --bundles app -- --profile fast-release
```

开发机器使用 Node 26.4.0；仓库建议的 24.18.1 在本机不可用。前端 TypeScript/Vite 检查和原生 bundle 编译均成功。输出在 `target/fast-release/bundle/macos/Clash Verge Tailscale Test.app`。

Documents 所在文件提供器自动附加 FinderInfo，导致原目录严格签名校验报错。复制到 `/tmp/clash-verge-coexistence-artifact/` 后，仅删除该副本的 `com.apple.FinderInfo` 元数据，严格校验通过。没有删除 quarantine、关闭 Gatekeeper 或修改签名信任。交付 ZIP 从已验证的暂存副本制作，应解压到本地非同步目录再核验。

## 签名限制

`codesign --verify --deep --strict` 确认包完整性；签名为 ad-hoc，没有 Team ID。`spctl --assess --type execute` 的真实只读结果为 **rejected**。这不是已经通过 Gatekeeper 的可分发应用，不能承诺双击可用。没有创建证书、使用开发者账号、公证上传或执行安全绕过。实际启动验收须先取得可信签名/公证构建，或另行审查合规的本地开发运行方案；当前不提供绕过命令。

## Windows

本机安装了 `x86_64-pc-windows-msvc` Rust target 和 clang-cl 22.1.8。纯 Rust 共存 crate 的 Windows target 检查通过；完整 app 检查在 `aws-lc-sys` 编译时缺少 Windows SDK/UCRT 的 `stdlib.h`，未完成链接或 NSIS 打包。本机也没有可用的 Windows 执行环境。因此没有 Windows 安装包，也没有 Windows 实测。后续应在获准的 Windows 构建机运行仓库正常 Windows pipeline 及 TEST-MATRIX.md 中的设备验收。

## 当前交付的用途

可审阅实现、编译成功的 macOS 测试包、配置/逻辑测试及静态核心验证。此包禁止 TUN、服务安装和自动 DNS 写入，默认测试代理端口17897。系统代理实际接管、Tailscale安装/注册及测试目标仍需按照 ACCEPTANCE-PLAN.zh-CN.md 确认，不能因包已生成而直接执行。
