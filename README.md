# RS Proxy

Windows 上的进程级 SOCKS5 代理工具。Rust 负责网络和系统集成，Tauri 2 提供桌面窗口，React 界面直接使用 Astryx 官方组件、Neutral 主题和布局模板。

## 使用

1. 以管理员身份启动 RS Proxy。
2. 在「代理组」中填写 SOCKS5 主机和端口，点击「测试连接」验证无认证握手。
3. 在「进程与连接」中搜索进程，打开「详情与代理」，将代理组应用到该进程及其后代。
4. 或在「路由规则」中创建并启用规则。规则从上到下匹配，手动选择优先。

「进程与连接」支持按代理组筛选，并与搜索、已选代理范围一起使用。页面每秒显示全部或所选组的上传、下载速度和累计流量；进程行显示实时速度，详情中可看该进程累计。流量从本次启动代理开始统计，重新启动引擎后清零。组汇总包含已结束进程的流量，搜索只过滤列表。

已经打开的应用需要切换旧连接时，先应用代理设置，再点击页面右上角「重连已选代理」。确认后会处理当前搜索和代理组筛选范围内、全部分页中的已选代理进程，只断开符合当前 TCP 规则的旧 IPv4 直连，不结束进程；已代理的连接、入站连接和本地代理提供者会保留。应用重新建立连接后才会走代理，下载或通话可能需要重试；不会强制应用自动重连，也不处理 UDP 或 IPv6 连接。操作结果显示实际断开、保留和未能处理的数量。

统计的是成功经 TCP/UDP 中继转发的载荷，不包含直连、SOCKS 握手、网络包头或单独的 DNS 转发器。列表分组按进程当前选择筛选，流量归属按连接实际使用的代理组计算；修改规则后，已有连接可能仍在使用先前的组。

默认没有启用的规则，DNS 转发默认关闭。应用不会修改系统代理或路由表。关闭按钮默认收起到托盘；从托盘选择「退出」会停止引擎并恢复本应用设置的 DNS。

规则可同时约束进程名、程序路径前缀、命令行、协议、目标网段和端口。进程名支持 `*` / `?`；命令行的普通关键词需全部命中，含通配符时按完整模式匹配。排除条件优先于包含条件。配置更新作用于新 TCP 连接；已有 TCP 连接继续使用建立时的代理组。

## 开发环境

- Windows 10/11 x64、WebView2 Runtime。
- Rust 1.94 或更新版本，MSVC 工具链；Visual Studio Build Tools 的 C++ 桌面开发组件与 Windows SDK。
- Node.js 22.13 或更新版本，npm。
- 运行桌面程序和手动平台集成测试时需要管理员权限；默认测试不需要。

```powershell
npm ci
pwsh -File scripts/prepare-windivert.ps1
npm run desktop
```

WinDivert 官方 x64 DLL、签名驱动和许可已随工程提供。准备脚本可重新下载并验证固定版本的 SHA256 和驱动签名。

```powershell
npm run build
npm run desktop:build -- --debug
npm run tauri -- build --bundles nsis
```

调试程序位于 `target/debug/rs-proxy.exe`，发布程序位于 `target/release/rs-proxy.exe`，NSIS 安装包位于 `target/release/bundle/nsis/`。分发便携程序时须同时保留 `windivert/` 和许可证资源；建议使用安装包。

仅运行 `npm run dev` 可以查看界面，但浏览器没有 Windows 服务，页面会明确显示未连接。

## 配置与恢复

数据目录是 `%LOCALAPPDATA%\dev.rsproxy.desktop`：

- `config.json`：代理组、规则和设置，原子写入并检查修订号。配置损坏时不会覆盖原文件。
- `dns-recovery.json`：DNS 变更前保存的恢复记录。每个接口恢复成功后才移除相应记录，全部恢复后删除文件；下次启动会先处理未完成恢复。
- `logs/`：按日生成日志，保留最近 7 份。日志级别下次启动生效。

设置页可导入/导出 JSON。删除代理组时必须指定替换组；规则、DNS 和手动选择在配置保存成功后一起更新。

DNS 转发使用本地 IPv4 回环地址的 53 端口，将 UDP/TCP DNS 查询转换为经 SOCKS5 的 TCP 查询。启用时会更改活动 IPv4 接口的 DNS；停止引擎或正常退出时恢复原值。若系统拒绝恢复，应用保留转发器和恢复记录，并显示错误；解决接口问题后重试停止或退出。强制结束应用后，应重新启动以执行恢复。

自动启动使用当前 Windows 用户的登录计划任务，以管理员权限运行；可选择启动后收起到托盘。

## 支持范围

- 透明转发 IPv4 TCP 与未分片 UDP。IPv6、ICMP 和 IPv4 分片保持直连。
- SOCKS5 无认证。UDP 要求服务器支持 UDP ASSOCIATE；不支持 SOCKS UDP 分片。
- 已建立的 TCP 连接不能无缝切换；「重连已选代理」可手动断开符合条件的旧直连，再由应用建立新连接。
- 受保护进程的路径或命令行可能无法读取，对应匹配条件不会凭空补全。
- 首包归属等待默认 20ms；超过等待窗口的 TCP 连接固定直连，避免重传中改变去向。它不是用于阻止所有直连流量的防火墙。
- 验证覆盖当前 Windows x64 环境；其它 Windows 版本、长期持续运行和极端负载尚需进一步验证。

## 验证与维护

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run build
npm test
npm run astryx -- doctor
```

默认 Rust 测试覆盖路由、配置、协议、流量计数和内存中的 DNS 恢复逻辑，不启动抓包驱动，不要求本地代理或管理员权限。依赖真实 Windows 进程、图标、ETW、WinDivert 和桌面服务引擎的集成用例受 `platform-tests` 特性控制，默认不编译、也不运行。

浏览器测试使用模拟桌面 IPC，验证界面交互与窄窗口布局。本地使用已安装的 Microsoft Edge；CI 使用 Playwright 对应版本的 Chromium，先运行 `npx playwright install chromium`。产品构建只检查产品源码；`npm test` 会先运行独立的测试类型检查，也可单独执行 `npm run check:tests`。完整工作区构建和检查应使用 Windows MSVC 环境；项目尚未配置 CI 工作流。

实际代理与原生桌面验收：

```powershell
cargo test --workspace --features rs-proxy/platform-tests
npm run test:proxy -- 127.0.0.1:7891
npm run test:native
npm run test:installer
```

这些命令仅用于本机手动验收，不加入常规 CI。`platform-tests` 需要管理员权限与 WinDivert，透明代理用例启动独立受控进程和本地 SOCKS5 测试服务，目标使用 `198.19.99.0/24`，不依赖外部服务。`test:proxy` 使用生产协议代码执行真实 TCP/UDP 查询，并在随机回环端口验证 DNS 转发。`test:native` 需要管理员权限、可用的 `127.0.0.1:7891` SOCKS5 服务和公网连接；先退出 RS Proxy，避免两套引擎并发。它每次将带本地调试端口的 QA 程序构建到独立的 `target/native-e2e/`，使用真实 Tauri IPC、独立网络子进程和透明字节观察器，验证进程路由、规则、设置、原生文件对话框和桌面生命周期；测试配置与日常配置分开，并恢复自动启动任务。证据写入 `%TEMP%/rs-proxy-e2e/`。公网超时按失败记录，不能用浏览器模拟测试代替。

以上命令均不切换系统 DNS。原生测试始终保持 DNS 关闭，并比较运行前后的网卡 DNS；测试集中不包含系统 DNS 写入用例。随机端口转发通过不代表系统 DNS 接管已验证。

`test:installer` 验证已构建的 NSIS 安装包，要求没有现有 RS Proxy 安装注册，且日常配置已存在并关闭 DNS。它安装到随机临时目录，检查资源、启动与正常退出，再卸载并核对配置与 DNS 基线；测试期间会暂时关闭“关闭窗口时收起到托盘”，结束后恢复原配置字节。

生成前端 Rust 类型：`cargo run -p proxy-core --bin export-types`。前端新增页面先使用 Astryx CLI 的 `build`、`template` 和 `component` 命令，不另建组件样式体系。字体从本地 Figtree 文件加载；doctor 的字体警告来自它对官方远程字体 CSS 的静态检查，Vite 会将其替换为本地字体入口。

目录职责：`crates/proxy-core` 管理领域和配置；`crates/proxy-platform` 封装 Windows；`crates/proxy-engine` 管理代理会话；`src-tauri` 组合桌面生命周期与命令；`src` 实现 Astryx 界面。

代码采用 MIT 许可。第三方资源见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
