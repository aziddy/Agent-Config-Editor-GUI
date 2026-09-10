# Agent Config Editor

A desktop app for seeing and editing everything you have configured for **Claude Code** and **OpenAI Codex CLI** on your machine: skills, MCP servers, plugins, subagents, slash commands, hooks and Codex automations. Filter by agent, kind and scope; toggle what can be toggled; edit with forms or a raw editor; reveal any entity in Finder or open it in your editor.

Built with Tauri 2 (Rust) + React/TypeScript. macOS first; Linux builds too.

## Prerequisites

- Node 20+, pnpm 9.15
- Rust 1.85+ (Homebrew `rust` or rustup)
- macOS: Xcode Command Line Tools. Linux: the standard Tauri 2 system packages (webkit2gtk, libayatana-appindicator, librsvg).

## Dev

```sh
pnpm install
pnpm tauri dev
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

Every change is previewed as a unified diff before it is written. Applying re-reads the file, refuses if it changed underneath you (structured edits rebase onto the fresh content when the touched key is unchanged), keeps a timestamped backup under the app data dir, writes atomically and preserves the file mode (Codex's `config.toml` stays `0600`). TOML edits are lossless: comments, blank lines, table order and quoting survive. Backups can be listed and restored from the Raw tab.

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
