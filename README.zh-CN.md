# RS Proxy

[English](README.md) | **简体中文**

Windows 上的进程级 SOCKS5 代理工具，使用 Rust、Tauri 2、React 和 Astryx 构建。选择应用及其后代进程，或创建路由规则，将对应的 IPv4 TCP、UDP 流量经 SOCKS5 代理转发。

## 功能

- 按进程选择代理，并在进程树中继承。
- 按顺序匹配进程名、程序路径、命令行、协议、目标网段和端口。
- 查看实时连接，以及按进程、代理组统计的上传下载速度和累计流量。
- 可选的进程 DNS 转发，无需修改网卡 DNS 设置。
- 独立的系统 DNS 转发，使用持久化记录恢复原设置。
- 断开已选应用中符合条件的旧 TCP 直连，供应用重新建立连接。
- 明暗主题、托盘控制、配置导入导出，以及登录时以管理员权限自动启动。

## 开始使用

以管理员身份启动 RS Proxy，然后：

1. 在「代理组」填写 SOCKS5 主机和端口，并测试连接。
2. 打开进程详情，将代理组应用到该进程及其后代。
3. 或创建路由规则。启用的规则从上到下匹配，手动选择优先。

配置更新作用于新连接；已有 TCP 连接保留建立时的路由。需要切换旧直连时，可使用「重连已选代理」，由应用自行建立新连接。该操作不会结束进程，也不处理 UDP 或 IPv6 连接。

关闭窗口默认收起到托盘。在托盘选择「退出」会停止引擎，并恢复 RS Proxy 修改的系统 DNS。系统 DNS 转发默认关闭。应用不会修改 Windows 系统代理或路由表。

## 运行要求与支持范围

- Windows 10/11 x64 和 WebView2 Runtime；抓包需要管理员权限。
- WinDivert **2.2.2**：未修改的官方 x64 DLL、签名驱动和许可证已随仓库及应用包提供。
- SOCKS5 无认证。普通 UDP 转发要求服务器支持 `UDP ASSOCIATE`；进程 DNS 使用 TCP。
- 透明转发 IPv4 TCP 与未分片 UDP。IPv6、ICMP 和 IPv4 分片保持直连，不支持 SOCKS5 UDP 分片。
- 进程 DNS 接管发往非回环 IPv4 地址 53 端口的普通查询，不接管共享 Windows DNS Client 服务、缓存结果或 DoH/DoT 等加密 DNS。
- 受保护进程可能无法读取路径或命令行。首包归属等待窗口内无法确认所属进程的连接会保持直连。RS Proxy 不作为阻止全部直连的防火墙。

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

通过 `cargo run -p proxy-core --bin export-types` 生成前端类型。Rust 负责路由、配置、持久化和 Windows 集成；前端消费带类型的 Tauri 命令与事件。贡献约定见 [AGENTS.md](AGENTS.md)。

## 许可

RS Proxy 使用 [MIT 许可](LICENSE)。第三方组件保留各自许可，详见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
