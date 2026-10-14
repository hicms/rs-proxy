# RS Proxy

**English** | [简体中文](README.zh-CN.md)

A process-level SOCKS5 proxy for Windows, built with Rust, Tauri 2, React, and Astryx. Select an application and its descendants, or define routing rules, to forward their IPv4 TCP and UDP traffic through a SOCKS5 proxy.

## Features

- Per-process proxy selection with inheritance across the process tree.
- Ordered routing rules for process names, executable paths, command lines, protocols, destination networks, and ports.
- Live connections, upload/download rates, and traffic totals by process and proxy group.
- Optional DNS forwarding for selected processes, without changing the network adapter's DNS settings.
- A separate, optional system DNS forwarder with persistent recovery records.
- Reconnect selected applications' eligible existing direct TCP connections.
- Light/dark themes, tray controls, configuration import/export, and optional elevated startup at sign-in.

## Getting started

Run RS Proxy as administrator, then:

1. Add a SOCKS5 host and port on the proxy groups page and test the connection.
2. Open a process's details and apply a proxy group to that process and its descendants.
3. Alternatively, add routing rules. Enabled rules match from top to bottom; manual process selections take priority.

Configuration changes apply to new connections. Existing TCP connections retain their original route. Use the reconnect action when an application needs to replace existing direct connections; the application must establish its own new connections. This action does not terminate processes or reconnect UDP/IPv6 traffic.

Closing the window normally minimizes the application to the tray. Choose **Exit** from the tray to stop the engine and restore any system DNS settings changed by RS Proxy. System DNS forwarding is disabled by default. RS Proxy does not change the Windows system proxy or routing table.

## Requirements and supported traffic

- Windows 10/11 x64 and WebView2 Runtime. Administrator privileges are required for packet capture.
- WinDivert **2.2.2**: the unmodified official x64 DLL, signed driver, and license are included in this repository and application packages.
- SOCKS5 servers using no authentication. General UDP forwarding requires `UDP ASSOCIATE`; process DNS forwarding uses TCP instead.
- Transparent forwarding covers IPv4 TCP and unfragmented UDP. IPv6, ICMP, and fragmented IPv4 traffic remain direct. SOCKS5 UDP fragmentation is unsupported.
- Process DNS forwarding handles ordinary DNS traffic to non-loopback IPv4 port 53. It does not capture shared Windows DNS Client service queries, cached answers, or encrypted DNS such as DoH/DoT.
- Protected processes may not expose executable paths or command lines. A connection whose owner cannot be resolved within the initial attribution window remains direct. RS Proxy is not a firewall that blocks every direct connection.

## Build from source

Install Rust **1.94+** with the MSVC toolchain, Visual Studio Build Tools with the C++ desktop workload and Windows SDK, Node.js **22.13+**, and npm. Use PowerShell 7 for resource preparation and native verification scripts.

```powershell
npm ci
npm run desktop
```

The included WinDivert resources are ready to use. To verify or restore them from the pinned official release:

```powershell
pwsh -NoProfile -File scripts/prepare-windivert.ps1
# An existing official archive can also be supplied:
pwsh -NoProfile -File scripts/prepare-windivert.ps1 -Archive <path-to-archive.zip>
```

The preparation script verifies the archive's SHA256 and driver signature before replacing resources. Failed preparation preserves the previous resources. If security software blocks the archive, inspect the individual detection before proceeding.

```powershell
# Standalone debug executable with the embedded frontend
npm run desktop:build -- --debug
# Release executable and NSIS installer
npm run tauri -- build --bundles nsis
```

Outputs are `target/debug/rs-proxy.exe`, `target/release/rs-proxy.exe`, and `target/release/bundle/nsis/`. Portable distributions must include the adjacent `windivert/` directory and license resources. The installer includes these dependencies.

The Tauri CLI enables the required `desktop` feature. Ordinary Cargo tests do not rebuild or replace the standalone executable. `npm run desktop` uses a local development server; `npm run dev` alone previews the frontend with no Windows backend.

## Configuration, DNS, and logs

Application data is stored in `%LOCALAPPDATA%\dev.rsproxy.desktop`:

| Path | Purpose |
| --- | --- |
| `config.json` | Proxy groups, rules, and settings. Writes are atomic and reject stale revisions; a corrupt file is preserved. |
| `dns-recovery.json` | Original adapter DNS settings saved before a system DNS change. Failed restores remain pending for the next startup. |
| `logs/` | Daily logs, retaining the latest seven files. Log-level changes take effect on the next startup. |

**Process DNS** uses the selected process's proxy group and configured upstream DNS server. Both TCP and UDP queries are forwarded over SOCKS5 TCP. UDP answers larger than 512 bytes are truncated with the DNS TC flag so clients can retry over TCP. Disable this option when an application needs local or split DNS resolution.

**System DNS forwarding** is independent. It listens on IPv4 loopback port 53 and changes active IPv4 adapters' DNS settings while enabled. Stopping the engine or exiting restores those settings. If restoration fails, the forwarder and recovery record remain active; resolve the interface error and retry. After a forced termination, restart RS Proxy to complete recovery.

Traffic counters measure payload successfully forwarded by the TCP/UDP relays. They exclude direct traffic, protocol overhead, and the separate system DNS forwarder. Counters reset when the engine restarts; existing connections retain the proxy group used when they were established.

## Development and verification

Run these checks in a Windows MSVC environment:

```powershell
npm run build
npm run astryx -- doctor
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features rs-proxy/desktop -- -D warnings
cargo test --workspace
npm test
```

Default Rust tests use controlled local fixtures and require no administrator privileges or external proxy. Browser tests mock desktop IPC and use Microsoft Edge locally; CI environments can install Chromium with `npx playwright install chromium`. They verify interaction, routing displays, retained page state, and small-window layouts.

Optional Windows integration tests exercise real process events, WinDivert, and transparent TCP/UDP/DNS routing with local test servers:

```powershell
# Run in an elevated terminal with no other RS Proxy instance running
cargo test --workspace --features rs-proxy/platform-tests
```

Process DNS integration tests also require Node.js. These tests do not change system DNS. Desktop verification commands are `npm run test:standalone`, `npm run test:titlebar`, and `npm run test:installer`. They require elevation, no running RS Proxy instance, and an existing application configuration with system DNS forwarding disabled. Build the debug executable before standalone/titlebar verification and the NSIS installer before installer verification. The installer check additionally requires no existing RS Proxy installation. Native checks preserve the configuration and adapter DNS baseline.

Generate frontend types with `cargo run -p proxy-core --bin export-types`. Rust owns routing, configuration, persistence, and Windows integration; the frontend consumes typed Tauri commands and events. Contributor conventions are in [AGENTS.md](AGENTS.md).

## License

RS Proxy is licensed under [MIT](LICENSE). Third-party components retain their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
