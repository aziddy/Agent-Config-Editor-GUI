model = "gpt-5.6-terra"
model_reasoning_effort = "high"
approval_policy = "on-request"

[projects."__ROOT__/projects/repo a"]
trust_level = "trusted"

[projects."__ROOT__/projects/repo-b"]
trust_level = "trusted"

[projects."__ROOT__"]
trust_level = "trusted"

[mcp_servers.alpha]
command = "npx"
args = ["alpha-mcp@latest", "--stdio"]
startup_timeout_sec = 120
tool_timeout_sec = 30.5

[mcp_servers.alpha.env]
ALPHA_JSON = '{"a":1}'
ALPHA_TOKEN = "REDACTED"

[mcp_servers.beta]
args = []
command = "./bin/beta"
cwd = "."
enabled = false

[mcp_servers.docs]
url = "https://example.com/mcp"
bearer_token_env_var = "DOCS_TOKEN"
oauth_resource = "https://example.com/mcp"

[plugins."demo@test-mkt"]
enabled = true

[plugins."demo@test-mkt".mcp_servers.demo-api.tools.search]
approval_mode = "approve"

[plugins."ghost@nowhere"]
enabled = false

[tui]
status_line = ["model-with-reasoning", "context-remaining"]
status_line_use_colors = true

[tui.keymap.editor]
insert_newline = ["alt-enter", "ctrl-j"]

[tui.keymap.chat]

[tui.model_availability_nux]
"gpt-5.5" = 4
"gpt-5.6-sol" = 4
gpt-6-astra = 4

[marketplaces.test-mkt]
source_type = "local"
source = "__ROOT__/marketplaces/test-mkt"

[[skills.config]]
name = "two"
enabled = false

[[skills.config]]
path = "__ROOT__/.codex/skills/one/SKILL.md"
enabled = false

[[skills.config]]
name = "locked"
enabled = true

[features]
js_repl = false
# cmux-codex-hooks-feature-1234 begin
hooks = true

[desktop]
followUpQueueMode = "queue"
dock-icon-preference = "codex-system"
# cmux-codex-hooks-feature-1234 end

[desktop.open-in-target-preferences.perPath]
"__ROOT__/projects/repo a" = "cursor"

[shell_environment_policy.set]
SOME_SECRET = "REDACTED"

# cmux-codex-hook-trust-5678 begin
[hooks.state."__ROOT__/.codex/hooks.json:pre_tool_use:0:0"]
trusted_hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"

[notice.model_migrations]
"gpt-5.4-mini" = "gpt-5.6-luna"
# cmux-codex-hook-trust-5678 end
