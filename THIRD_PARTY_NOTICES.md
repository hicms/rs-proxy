# Third-party notices

RS Proxy uses the following independent open-source components. Package lockfiles record the complete dependency versions.

- **Astryx** — Copyright (c) Meta Platforms, Inc. and affiliates. MIT. Components, official Neutral and Butter themes, and documented layout composition patterns. https://github.com/facebook/astryx
- **React** — Copyright (c) Meta Platforms, Inc. and affiliates. MIT. https://github.com/facebook/react
- **Lucide** — ISC and MIT (derived Feather icons). SVG icons rendered through Astryx Icon; complete notices: `licenses/Lucide-LICENSE.txt`. https://lucide.dev
- **Tauri** — MIT OR Apache-2.0. https://github.com/tauri-apps/tauri
- **WinDivert 2.2.2** — Copyright (c) Basil, LGPL v3 (the author also offers GPL v2). RS Proxy dynamically loads the unmodified official x64 DLL and signed driver. The distributed license is included at `src-tauri/resources/windivert/LICENSE`. Source and release archives: https://github.com/basil00/WinDivert
- **Figtree** — Copyright 2022 The Figtree Project Authors. SIL Open Font License 1.1. Fonts are bundled locally via Fontsource; license: `licenses/Figtree-OFL.txt`. https://github.com/erikdkennedy/figtree
- **Outfit** — Copyright 2021 The Outfit Project Authors. SIL Open Font License 1.1. Bundled locally via Fontsource; license: `licenses/Outfit-OFL.txt`. https://github.com/Outfitio/Outfit-Fonts
- **Tokio** — MIT. https://github.com/tokio-rs/tokio
- **Hickory DNS** — MIT OR Apache-2.0. DNS wire validation and truncated response encoding; MIT notice: `licenses/Hickory-MIT.txt`. https://github.com/hickory-dns/hickory-dns
- **Rust for Windows** — MIT OR Apache-2.0. https://github.com/microsoft/windows-rs

WinDivert resources are replaceable alongside the executable under `windivert/`. `scripts/prepare-windivert.ps1` downloads the official release and verifies the archive checksum and driver signature.

Full license texts for Astryx, React, Tauri, Figtree, Outfit, Lucide and Hickory DNS are in `licenses/` and included in the application package. Astryx page templates retain their copyright notices.
