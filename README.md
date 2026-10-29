# Agent Config Editor

A desktop app for seeing and editing everything you have configured for **Claude Code** and **OpenAI Codex CLI** on your machine: skills, MCP servers, plugins, subagents, slash commands, hooks and Codex automations. Filter by agent, kind and scope; toggle what can be toggled; edit with forms or a raw editor; reveal any entity in Finder or open it in your editor.

The **Skills** list also filters by implicit invocation, enabled state, built-in status, and installer-managed status. Invocation uses Codex’s `agents/openai.yaml` policy or Claude’s `disable-model-invocation` frontmatter; missing settings allow automatic use, and unreadable policies appear as **Unknown**. Claude skills have **No enable switch**. Filters combine with agent, location, and search, respect visibility Settings, and remain selected during the current app session. **Clear skill filters** resets just these four controls.

Select a Codex automation and open **Runs** to read its local run history, including archived conversations and expandable tool activity. History refreshes while visible, loads large outputs in chunks, and never changes the automation or marks a run read, accepted, or archived.

Built with Tauri 2 (Rust) + React/TypeScript. macOS first; Linux builds too.

## Prerequisites

- Node 20+, pnpm 9.15
- Rust 1.85+ (Homebrew `rust` or rustup)
- macOS: Xcode Command Line Tools. Linux: the standard Tauri 2 system packages (webkit2gtk, libayatana-appindicator, librsvg).

## Dev

```sh
pnpm install
pnpm tauri dev              # Vite HMR + cargo rebuilds
pnpm dev:static:fresh       # build a static debug binary and run it, no watchers
pnpm dev:static             # relaunch the last static build
```

Run against a **copy** of your real configuration first (recommended after any change to the write path):

```sh
S=$PWD/.sandbox-home; mkdir -p $S/.codex $S/.agents
rsync -a --exclude projects --exclude todos --exclude shell-snapshots --exclude debug --exclude 'history*' \
  --exclude file-history --exclude paste-cache --exclude backups --exclude sessions --exclude telemetry \
  --exclude statsig --exclude tasks --exclude teams --exclude jobs --exclude session-env --exclude plans \
  --exclude 'daemon*' ~/.claude/ $S/.claude/
cp ~/.claude.json $S/.claude.json
rsync -a ~/.codex/config.toml ~/.codex/hooks.json ~/.codex/skills ~/.codex/agents ~/.codex/automations ~/.codex/plugins ~/.codex/rules $S/.codex/
rsync -a ~/.agents/ $S/.agents/
ACE_HOME=$S pnpm tauri dev
```

The sidebar shows a red "Sandbox home" badge while `ACE_HOME` is set. Project-scope files still point at your real repositories; writes there are refused in sandbox mode unless `ACE_ALLOW_PROJECT_WRITES=1`. `ACE_READ_ONLY=1` disables all writes.

## Build (macOS)

```sh
APPLE_SIGNING_IDENTITY=- pnpm tauri build
open src-tauri/target/release/bundle/dmg
```

The bundle is ad-hoc signed: right-click → Open on first launch.

## How edits work

**Codex Settings** is a separate cards/forms view of the same user config, covering model and reasoning, agents, context and compaction, permissions and web search, instructions, project discovery, history and privacy, shell environment filters, memories, and terminal preferences. Its documented feature catalog includes options that are not yet in your file, with default and maturity guidance. Every field starts with **Use Codex default** when unset; loading the page never adds defaults. Save reviews all form changes in one diff. Numeric limits, filter patterns, and list rows are validated before saving. Resetting a field removes its explicit override; advanced and custom TOML values remain available in **Codex Config**. Options follow the [official Codex configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference); availability can depend on your installed Codex version, client, model, and account.

Open **Codex Config** in the sidebar to view and edit the full `~/.codex/config.toml` with TOML syntax highlighting, diff previews, and backup history. The page uses the resolved Codex home, including `CODEX_HOME` and sandbox mode, independently of the agent and location filters.

Every change is previewed as a unified diff before it is written. Applying re-reads the file, refuses if it changed underneath you (structured edits rebase onto the fresh content when the touched key is unchanged), keeps a timestamped backup under the app data dir, writes atomically and preserves the file mode (Codex's `config.toml` stays `0600`). TOML edits are lossless: comments, blank lines, table order and quoting survive. Backups can be listed and restored from the Editor tab. Editing a symlink updates its target and preserves the link; changing the target after preview blocks the save.

The **Memories** sidebar expands into **Core Codex** (`~/.codex/memories`, or the configured Codex home), **Automation memories**, **Claude project memories**, **Claude subagent memories**, **Instructions**, and **Claude rules**, each with a count. Click **Memories** to see all sources; provider and location filters apply across sources. Repository and ancestor `AGENTS.md`, `AGENTS.override.md`, `CLAUDE.md`, `CLAUDE.local.md`, and `.claude/CLAUDE.md` are excluded, including nested copies. Global/home and managed instructions, Claude rules, and other configured fallback files remain available. Preview Markdown or edit the original text; shared files appear once with their agent and project badges. Existing files remain visible when memory is disabled; the inventory does not indicate which instructions an agent currently loads. Dependency/build directories are skipped, and **Rescan** discovers newly added nested rules and fallback files. This version edits existing files; it does not create or delete memories or change memory settings.

What can be toggled:

| Entity | Toggle |
| --- | --- |
| Claude MCP server (user / plugin scope) | per project, via `disabledMcpServers` in `~/.claude.json` |
| Claude project `.mcp.json` server | `enabledMcpjsonServers` / `disabledMcpjsonServers` for that project |
| Claude plugin | `enabledPlugins` in the user or project `settings.json` |
| Codex MCP server / plugin | `enabled` in `~/.codex/config.toml` |
| Codex skill | `[[skills.config]]` deny-list in `~/.codex/config.toml` |
| Codex automation | `status` in `automation.toml` |
| Claude skills, subagents, commands, hooks | no switch exists; edit the file instead |

## Layout

See `AGENTS.md` for the module map, conventions and validation commands.

## Tests

```sh
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
```

Fixtures under `src-tauri/tests/fixtures` reproduce the on-disk shapes both agents use, including the two incompatible `.mcp.json` layouts, array-valued `installed_plugins.json`, name- and path-keyed `[[skills.config]]` entries, frontmatter-less commands, and a `config.toml` with sentinel comments and empty tables that must round-trip byte-for-byte.
