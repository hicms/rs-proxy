# RS Proxy 使用与开发说明

[返回首页](../README.zh-CN.md)

## 从源码构建

安装 Rust **1.94+** 与 MSVC 工具链、Visual Studio Build Tools 的 C++ 桌面开发组件与 Windows SDK、Node.js **22.13+** 和 npm。资源准备及原生验证脚本使用 PowerShell 7。

```powershell
npm ci
npm run desktop
```

仓库中的 WinDivert 资源可直接使用。需要从固定的官方发行包校验或恢复资源时：

```powershell
pwsh -NoProfile -File scripts/prepare-windivert.ps1
# 也可以提供已有的官方归档：
pwsh -NoProfile -File scripts/prepare-windivert.ps1 -Archive <归档路径.zip>
```

准备脚本在替换资源前验证归档 SHA256 和驱动签名，失败时保留原资源。安全防护拦截归档时，应先核验对应的单条检测记录。

```powershell
# 内置前端的独立调试程序
npm run desktop:build -- --debug
# 发布程序与 NSIS 安装包
npm run tauri -- build --bundles nsis
```

输出分别位于 `target/debug/rs-proxy.exe`、`target/release/rs-proxy.exe` 和 `target/release/bundle/nsis/`。便携分发必须保留程序旁的 `windivert/` 目录和许可证资源，安装包会包含这些依赖。

Tauri CLI 已启用必要的 `desktop` 特性。普通 Cargo 测试不会重新构建或覆盖独立程序。`npm run desktop` 使用本地开发服务器；单独执行 `npm run dev` 只能预览前端，无法连接 Windows 后端。

## 配置、DNS 与日志

应用数据位于 `%LOCALAPPDATA%\dev.rsproxy.desktop`：

| 路径 | 用途 |
| --- | --- |
| `config.json` | 代理组、规则和设置。原子写入并拒绝过期修订，配置损坏时保留原文件。 |
| `dns-recovery.json` | 修改系统 DNS 前保存的网卡原设置。恢复失败的记录保留到下次启动。 |
| `logs/` | 按日生成日志，保留最近七份。日志级别下次启动生效。 |

**进程 DNS** 使用该进程选择的代理组和配置的上游 DNS。TCP、UDP 查询均通过 SOCKS5 TCP 转发；超过 512 字节的 UDP 回复设置 DNS TC 标记，让客户端通过 TCP 重试。应用需要内网域名或本地分流解析时，可关闭此功能。

**系统 DNS 转发** 是独立功能，在 IPv4 回环地址的 53 端口监听，启用期间修改活动 IPv4 网卡的 DNS。停止引擎或退出会恢复原设置；恢复失败时保留转发器与恢复记录，解决接口错误后重试。强制结束程序后，应重新启动以完成恢复。

流量计数只包含成功经 TCP/UDP 中继转发的载荷，不包含直连、协议开销或独立的系统 DNS 转发器。重启引擎后计数清零；已有连接仍归属于建立时使用的代理组。

## 开发与验证

在 Windows MSVC 环境运行：

```powershell
npm run build
npm run astryx -- doctor
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features rs-proxy/desktop -- -D warnings
cargo test --workspace
npm test
```

默认 Rust 测试使用受控的本地测试服务，不需要管理员权限或外部代理。浏览器测试模拟桌面 IPC，本地使用 Microsoft Edge；CI 环境可通过 `npx playwright install chromium` 安装 Chromium。测试覆盖交互、路由展示、页面状态保留和窄窗口布局。

可选 Windows 集成测试使用真实进程事件、WinDivert 和本地测试服务，验证透明 TCP、UDP 与 DNS 路由：

```powershell
# 在管理员终端运行，并先退出其他 RS Proxy 实例
cargo test --workspace --features rs-proxy/platform-tests
```

进程 DNS 集成测试还需要 Node.js。这些测试不修改系统 DNS。桌面验证命令为 `npm run test:standalone`、`npm run test:titlebar` 和 `npm run test:installer`，要求管理员权限、没有运行中的 RS Proxy，并已有关闭系统 DNS 转发的应用配置。独立启动和标题栏验证前需构建调试程序，安装验证前需构建 NSIS 安装包，且没有现有 RS Proxy 安装。原生验证保留原配置和网卡 DNS 基线。

通过 `cargo run -p proxy-core --bin export-types` 生成前端类型。Rust 负责路由、配置、持久化和 Windows 集成；前端消费带类型的 Tauri 命令与事件。贡献约定见 [AGENTS.md](../AGENTS.md)。
