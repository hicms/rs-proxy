# RS Proxy guide

[Back to README](../README.md)

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

Generate frontend types with `cargo run -p proxy-core --bin export-types`. Rust owns routing, configuration, persistence, and Windows integration; the frontend consumes typed Tauri commands and events. Contributor conventions are in [AGENTS.md](../AGENTS.md).
