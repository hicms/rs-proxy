# RS Proxy

[English](README.md) | **简体中文**

**为 Windows 应用单独选择 SOCKS5 代理。**

RS Proxy 使用 Rust 和 Tauri 2 构建。选择应用及其后代进程，或通过路由规则，将对应的 IPv4 TCP、UDP 流量经代理转发。

![Windows 10/11 x64](https://img.shields.io/badge/Windows-10%2F11_x64-0078D4)
![Rust + Tauri 2](https://img.shields.io/badge/Built_with-Rust_%2B_Tauri_2-24C8DB)
[![MIT License](https://img.shields.io/badge/License-MIT-green)](LICENSE)

## 界面预览

以下为使用示例数据的界面预览，细节以当前版本为准。

![RS Proxy 进程代理与实时流量](docs/images/processes.jpg)

<details>
<summary>查看网络与 DNS 设置</summary>

![进程 DNS、系统 DNS 转发与直连网段](docs/images/network.png)

</details>

## 功能

- **进程代理**：为应用及其后代选择代理，手动选择优先于自动规则。
- **灵活路由**：按顺序匹配进程名、程序路径、命令行、协议、目标网段和端口。
- **实时监控**：查看连接，以及按进程、代理组统计的上传下载速度和累计流量。
- **DNS 转发**：进程 DNS 无需修改网卡设置；独立的系统 DNS 转发使用恢复记录还原原设置。
- **桌面体验**：明暗主题、托盘控制、配置导入导出，以及登录时以管理员权限自动启动。

## 开始使用

需要 **Windows 10/11 x64**、**WebView2 Runtime**、管理员权限，以及支持无认证连接的 SOCKS5 服务器。WinDivert 2.2.2 运行资源和许可证已随仓库及应用包提供。

1. 以管理员身份运行。在「代理组」填写 SOCKS5 主机和端口，并测试连接。
2. 打开进程详情，将代理组应用到该进程及其后代。
3. 需要自动选择时，添加路由规则；启用的规则从上到下匹配。

设置作用于**新连接**。已有 TCP 连接保留原路由；「重连已选代理」会断开符合条件的旧直连，由应用自行建立新连接，不会结束进程，也不处理 UDP 或 IPv6 连接。

关闭窗口默认收起到托盘。在托盘选择「退出」会停止引擎，并恢复 RS Proxy 修改的系统 DNS。系统 DNS 转发默认关闭；应用不会修改 Windows 系统代理或路由表。

## 支持范围

- IPv4 TCP 与未分片 UDP。普通 UDP 转发要求 SOCKS5 服务器支持 `UDP ASSOCIATE`，进程 DNS 使用 TCP。
- IPv6、ICMP、IPv4 分片及无法确认所属进程的连接保持直连。RS Proxy 不作为阻止全部直连的防火墙。
- 进程 DNS 处理发往非回环 IPv4 地址 53 端口的普通查询，不接管共享 Windows DNS Client 服务、缓存结果或 DoH/DoT。
- 受保护进程可能无法读取程序路径或命令行。

## 构建

安装 Rust **1.94+**（MSVC）、Visual Studio C++ Build Tools 与 Windows SDK、Node.js **22.13+** 和 npm。

```powershell
npm ci
npm run desktop                         # 开发运行
npm run tauri -- build --bundles nsis     # Release 程序与安装包
```

安装包生成在 `target/release/bundle/nsis/`。便携分发需保留程序旁的 `windivert/` 目录和许可证资源。单独运行 `npm run dev` 只能预览前端，无法连接 Windows 后端。

## 更多说明

- [构建、配置、DNS 恢复与开发验证](docs/guide.zh-CN.md)
- [English guide](docs/guide.md)
- [贡献约定](AGENTS.md) · [反馈问题或提出建议](https://github.com/hicms/rs-proxy/issues)

## 许可

[MIT](LICENSE)。第三方组件保留各自许可，详见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
