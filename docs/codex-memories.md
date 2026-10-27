# Codex memory files

Codex stores local memories under `~/.codex/memories/` by default, or under
`$CODEX_HOME/memories/` when a custom Codex home is configured.

The file roles and loading behavior below were checked on October 26, 2026,
using official OpenAI documentation and the memory instructions bundled with
the installed Codex CLI.

## Which files Codex uses

**When memory use is enabled, Codex automatically includes `memory_summary.md`
in the session's context.** It then uses that overview to find relevant details
in the other memory files as needed.

| File or directory | Purpose | How Codex uses it |
| --- | --- | --- |
| `memory_summary.md` | Compact overview of the user's profile, preferences, general tips, and an index of remembered topics. | Automatically included in the session's context when memory use is enabled. |
| `MEMORY.md` | Detailed, organized memory collection: project context, lessons, procedures, and findings grouped by task, with search keywords and references. | Searched and read as needed for the current task. The full file is not automatically included in every session. |
| `rollout_summaries/` | Summaries of individual past sessions that provide supporting evidence and additional detail. | Relevant summaries can be opened when Codex needs deeper context. |
| `raw_memories.md` | Extracted notes from individual conversations before consolidation. These are extracted memories, not verbatim chat transcripts. | Primarily used by the background memory-generation process; normally not loaded into the working session. |
| `skills/` | Reusable workflows captured during memory consolidation. | Relevant skill instructions can be read as needed. |

For example, a task involving a previously discussed project can follow this
retrieval path:

1. The automatically supplied `memory_summary.md` points to the project topic.
2. Codex searches `MEMORY.md` for the relevant project or task keywords.
3. Codex opens a referenced rollout summary or skill if more detail is useful.

Codex does not need to read every memory file for every task.

## How memories are generated

Memory generation and memory use are separate processes:

- **Generation:** eligible prior conversations produce extracted notes and
  session summaries. A background consolidation pass uses these inputs to
  update `MEMORY.md`, `memory_summary.md`, and any reusable memory skills.
- **Use:** a working session receives the compact summary and retrieves
  relevant details when they help with the current task.

Updates happen in the background and may not appear immediately after a chat
ends. In supported local Codex clients, `/memories` controls whether the current
chat can use existing memories and contribute to future memories.

## Editing and persistent instructions

OpenAI treats these files as generated state. They can be inspected, but manual
edits should not be the primary way to control memory behavior; later
consolidation can rewrite them.

Keep instructions that must consistently apply in `AGENTS.md` or checked-in
documentation. Memories provide useful recall of earlier work.

Local Codex memories use a separate store and controls from ChatGPT web memory.

## Sources

- [OpenAI: Memories](https://learn.chatgpt.com/docs/customization/memories) —
  local storage, background generation, controls, and generated-state guidance.
- Memory instructions bundled with the installed Codex CLI — automatic loading
  of `memory_summary.md` and progressive retrieval through `MEMORY.md`, skills,
  and rollout summaries. These implementation details can vary by version.
