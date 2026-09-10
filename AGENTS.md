# AGENTS.md

## Repository

Agent Config Editor is a Tauri 2 desktop app (macOS first, Linux second) that lists and edits the configuration of Claude Code and OpenAI Codex CLI: skills, MCP servers, plugins, subagents, slash commands, hooks and Codex automations. Every entity can be revealed in Finder or opened in an external editor.

- `src/` — React 19 + TypeScript frontend (Vite, Tailwind 3, Zustand, TanStack Query, CodeMirror 6).
  - `src/lib/types.ts` mirrors `src-tauri/src/model.rs` by hand; `src/lib/rpc.ts` is the typed map of every Tauri command.
  - `src/lib/entities.ts` — selectors, filtering and duplicate collapsing. `src/lib/frontmatter.ts`, `src/lib/rrule.ts`, `src/lib/mask.ts` — pure helpers with colocated `*.test.ts`.
  - `src/components/` — shell (Sidebar, TopBar, EntityList, DetailPane), RawFileEditor, FilesTab, DiffDialog. `src/components/ui/` — shadcn-style primitives.
  - `src/forms/` — McpServerForm, FrontmatterForm, AutomationForm, ScheduleBuilder. `src/screens/` — EntityScreen, ProjectsScreen, SettingsScreen, NewDialogs, `detail/*` per-kind overviews.
- `src-tauri/` — Rust backend. The webview never touches the filesystem; everything goes through commands in `src-tauri/src/commands.rs`.
  - `model.rs` unified data model · `scan.rs` snapshot assembly + dedupe · `claude/` and `codex/` readers · `markdown/frontmatter.rs` lenient frontmatter parser.
  - `writeplan.rs` two-phase writes (preview → apply) with lossless JSON/TOML/frontmatter mutations · `actions.rs` maps UI requests to plans · `fsutil.rs` homes, atomic writes, backups · `watcher.rs` debounced file watching · `settings.rs` app settings.
  - `tests/fixtures/{claude_home,codex_home}` miniature homes reproducing every on-disk trap; `tests/*_scan.rs` integration tests.

## Development

Requires Node 20+, pnpm 9.15, Rust 1.85+ and the Tauri macOS prerequisites (Xcode Command Line Tools).

| Command | Purpose |
| --- | --- |
| `pnpm install` | Install dependencies. |
| `pnpm tauri dev` | Run the app with Vite HMR. |
| `ACE_HOME=$PWD/.sandbox-home pnpm tauri dev` | Run against a copy of the real config (see Sandbox). |
| `pnpm dev:static:prepare` | Build the static debug executable (`tauri build --debug --no-bundle`, frontend embedded); rerun after source changes. |
| `pnpm dev:static` | Launch the prepared executable without Vite or cargo watchers. |
| `pnpm dev:static:fresh` | Prepare, then launch. `ACE_HOME` and the other env vars apply here too. |
| `pnpm typecheck` / `pnpm lint` / `pnpm test` | TypeScript, Biome, Vitest. |
| `cargo test --manifest-path src-tauri/Cargo.toml` | Rust unit + integration tests. |
| `cargo test --manifest-path src-tauri/Cargo.toml -- --ignored --nocapture` | Scan the real machine (or `ACE_HOME`) and print counts, read-only. |
| `APPLE_SIGNING_IDENTITY=- pnpm tauri build` | Ad-hoc signed `.app` + `.dmg` under `src-tauri/target/release/bundle`. |

## Sandbox

`ACE_HOME=<dir>` makes the app treat `<dir>` as the home directory (`<dir>/.claude`, `<dir>/.claude.json`, `<dir>/.codex`, `<dir>/.agents`). Writes outside it are refused unless `ACE_ALLOW_PROJECT_WRITES=1`; `ACE_READ_ONLY=1` blocks every write. `.sandbox-home/` is gitignored; build it with the rsync recipe in `README.md` before testing edits.

## Implementation

- Add a command in `commands.rs`, register it in `lib.rs`, and add its entry to `src/lib/rpc.ts` in the same change. Keep `model.rs` and `src/lib/types.ts` in sync.
- Every write goes through a `WritePlan`: build it in `actions.rs`/`writeplan.rs`, show the diff, then `apply_write_plan`. Never write files from the frontend or from readers.
- TOML is edited only with `toml_edit` (comments, table order, quoted keys and literal strings must survive). JSON keeps key order (`serde_json` `preserve_order`). Frontmatter edits splice only the changed keys.
- Readers push into `ScanOut`, record every file hash, warn on parse errors and never panic on missing files. Unknown keys are passed through as `extra`.
- Values with secret-looking keys are masked in the UI (`src/lib/mask.ts`); never log them.

## Validation

- `pnpm typecheck && pnpm lint && pnpm test`
- `cargo fmt --check --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings && cargo test --manifest-path src-tauri/Cargo.toml`
- Before touching real config after a write-path change: run against `.sandbox-home`, do a no-op save of `.codex/config.toml`, and confirm `cmp` against the original is byte-identical and `stat -f %Lp` still prints `600`.
