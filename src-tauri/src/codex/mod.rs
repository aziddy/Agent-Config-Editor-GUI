//! Codex CLI readers (codex-cli 0.153.x layout).
//!
//! | What | Where |
//! | --- | --- |
//! | Main config | `<codex_home>/config.toml` (mode 0600, read through `toml_edit`) |
//! | Project config | `<project>/.codex/config.toml` for every `[projects."<abs path>"]` entry |
//! | Skills | `<codex_home>/skills/**/SKILL.md` (`.system/` = built-ins), `<agents_dir>/skills/*/SKILL.md`, plugin `skills/`, `<project>/.agents/skills/*/SKILL.md` |
//! | Plugins | `[plugins."name@marketplace"]` resolved against `<codex_home>/plugins/cache/<mkt>/<name>/<version>/` |
//! | Subagents | `<codex_home>/agents/*.toml` |
//! | Hooks | `<codex_home>/hooks.json` + `[hooks.state."<file>:<event>:<i>:<j>"]` trust entries |
//! | Automations | `<codex_home>/automations/<id>/automation.toml` |
//!
//! Every reader is lenient: missing files and directories are silent, parse failures become a
//! [`ScanOut::warn`] and the reader moves on. [`runtime::fetch_mcp_status`] is intentionally not
//! called from [`scan`] because it spawns the Codex binary.

pub mod agents;
pub mod automations;
pub mod config_toml;
pub mod hooks;
pub mod plugins;
pub mod runtime;
pub mod skills;

use crate::scan::{ScanCtx, ScanOut};

/// Read everything Codex-related under `ctx.homes` into `out`.
pub fn scan(ctx: &ScanCtx<'_>, out: &mut ScanOut) {
    let config = config_toml::load(ctx, out);
    config_toml::scan_mcp_servers(&config, out);
    skills::scan(ctx, &config, out);
    plugins::scan(ctx, &config, out);
    agents::scan(ctx, out);
    hooks::scan(ctx, &config, out);
    automations::scan(ctx, out);
}
