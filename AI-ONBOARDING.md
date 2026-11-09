# AI onboarding

A prompt for an AI coding agent (Claude Code, Codex, OpenCode or another) that sets up the
local AI tooling for this repository, for example after a fresh clone. Give the text below
to the agent, or tell the agent to read this file and follow it.

---

You are setting up the local AI tooling for voicebro-discord. Work in the repository root.

## Rules

- Everything is local, inside the project folder. Do not change the user's home folders
  (`~/.claude/`, `~/.codex/`, `~/.config/opencode/`, `~/.agents/`) and install nothing
  globally: no `npm -g`, no global skills, MCP servers, plugins or hooks. If a step can only
  be done globally, stop and ask the user.
- Skills come from GitHub. Read each `SKILL.md` before installing it.
- Agent files are local and never committed: `AGENTS.md`, `AI-REFERENCE.md`, `.agents/`,
  `.claude/`, `.codex/`, `.opencode/` and `.tools/` are in `.gitignore`. Tracked are this
  file and `openspec/`, the project specs.
- `AGENTS.md` and `AI-REFERENCE.md` are not in git. If they are missing, ask the user for
  them. Do not create `CLAUDE.md`.
- Ask before running anything that is not listed here.

## 1. Rust skills

From https://github.com/morganlinton/BeautifulRust (MIT) at commit
`b9718436d77ee7bcf17cdcb6aa27628feb95d97a`:

| Skill | Path in that repository | Files to take |
|---|---|---|
| `rust-best-practices` | `skills/rust-best-practices/` | `SKILL.md`, `references/*.md` |
| `rust-review` | `skills/rust-review/` | `SKILL.md` |

Raw files are at
`https://raw.githubusercontent.com/morganlinton/BeautifulRust/<commit>/<path>`.

Copy each skill into both `.agents/skills/<name>/` (read by Codex and OpenCode) and
`.claude/skills/<name>/` (read by Claude Code). Put the repository's `LICENSE` into each
skill folder. Leave out `evals/`.

## 2. OpenSpec

1. Install the CLI locally (MIT, about 2.3 MB):

   ```bash
   npm install --prefix .tools/openspec @fission-ai/openspec@1.16.1
   ```

2. Set it up for the three agents, with telemetry off. In bash:

   ```bash
   OPENSPEC_TELEMETRY=0 DO_NOT_TRACK=1 ./.tools/openspec/node_modules/.bin/openspec init --tools claude,codex,opencode --no-animation --no-copilot-cloud
   ```

   In PowerShell:

   ```powershell
   $env:OPENSPEC_TELEMETRY='0'; $env:DO_NOT_TRACK='1'; .\.tools\openspec\node_modules\.bin\openspec.cmd init --tools claude,codex,opencode --no-animation --no-copilot-cloud
   ```

   It adds the `openspec-*` skills to `.claude/skills/`, `.agents/skills/` and
   `.opencode/skills/`, and keeps the existing `openspec/` folder.

3. The CLI is not on PATH. The OpenSpec skills call `openspec ...`; run those commands as
   `.tools/openspec/node_modules/.bin/openspec ...`, with `OPENSPEC_TELEMETRY=0` and
   `DO_NOT_TRACK=1` set.

## 3. Check

- Codex: `codex debug prompt-input` lists `rust-best-practices`, `rust-review` and the
  `openspec-*` skills.
- OpenCode: `opencode debug skill` lists them.
- Claude Code: a new session started in the repository root lists them.
- Nothing changed in the home folders, and `git status` shows no agent files.

Report what was installed and what was not checked.
