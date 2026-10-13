# RS Proxy development

- Rust owns routing, configuration validation, persistence, and Windows integration. The frontend consumes typed Tauri commands/events.
- Use only Astryx components, official themes, tokens, and documented composition patterns. Read the installed CLI docs before using component props. Do not introduce another UI kit or custom control styling.
- Keep tests in `tests/` or each crate's `tests/`; hand-written source files must stay under 1000 lines.
- Keep implementation plans under ignored `.plan/`. Update the execution ledger after each task and validate before a local commit.
- Describe RS Proxy as an independent product. Do not include external application history, repository paths, or source relationships in code, comments, docs, plans, or commit messages. Retain required third-party license notices.
- Run `npm run build`, `npm run astryx -- doctor`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` before delivery.
- The desktop binary requires the `desktop` feature, enabled by the Tauri CLI configuration. Also run `cargo clippy --workspace --all-targets --features rs-proxy/desktop -- -D warnings` to check desktop commands and lifecycle code. Use `npm run desktop:build -- --debug` for a standalone debug executable; ordinary Cargo tests must leave that artifact unchanged. Verify the existing executable with `npm run test:standalone` when changing desktop startup or packaging.

<!-- ASTRYX:START -->
Astryx v0.7.0 · 168 components
CLI: run every command as `npx astryx <cmd>` (shown below as `astryx ...`).

SETUP (once, in app entry e.g. main.tsx) — without these, components render unstyled:
  import "@astryxdesign/core/reset.css";
  import "@astryxdesign/core/astryx.css";

WORKFLOW — start every page from a template, never from scratch:
1. `astryx build "<idea>"` — names the template to scaffold from and the parts it lacks.
2. `astryx template <name> <path>` — scaffold it; keep its frame, gap and padding; replace the content.
3. `astryx template <Block>` for parts it lacks; `astryx component <Name>` to read props before using one.
Changing an existing page? Keep it: skip step 2 and add blocks/components inside its sections.

RULES:
- No <div> — components handle all layout and spacing.
- Read `astryx docs layout` before changing a template's frame.
- Dense data = rows (Table, List/Item), not Card-wrapped lists. Badge = counts only.
- Style with component props first, then tokens (var(--color-*|--spacing-*|--radius-*)). No raw hex/px. No StyleX/Tailwind compiler here — don't use xstyle/utility classes directly. Some templates use StyleX and need a compiler plugin; see `astryx docs styling-overview`.
- Palettes and custom colors go through the theme, never :root overrides. Run `astryx theme add <slug>` to import; `theme eject <slug>` only forks source. See `astryx docs theme`.
- SELF-CHECK: re-read the file; replace any raw <div>/<span>, imported .css/@apply, or hardcoded #hex/px with the component or a token.

`astryx help` lists every command. Key ones beyond the workflow:
  search "<query>"   find any component / hook / doc / template
  discover <words>   integrations you could add
  docs <topic>       getting-started, principles, tokens, theme … (`astryx docs` lists all)
  docs cli           commands, API reference, integration authoring
  upgrade --from <old version> --apply   run after a dependency bump
<!-- ASTRYX:END -->
