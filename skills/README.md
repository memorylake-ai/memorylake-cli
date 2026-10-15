# Skills for coding agents

Each directory here is one [Agent Skill](https://agentskills.io): a `SKILL.md`
with `name` and `description` frontmatter, plus any `references/` it links to.
The CLI embeds this directory at build time, and `memorylake agent-skills
install` copies each skill into `~/.claude/skills/` and `~/.agents/skills/`.
Files directly in this directory, like this one, are not installed.

When writing or changing a skill:

- **`name` matches the directory**: lowercase letters, digits and single
  hyphens, prefixed `memorylake-cli`. The description is one line of 1-1024
  characters and says when to use the skill, not only what it covers.
- **Keep `SKILL.md` short.** Its description sits in every session's context
  and its body loads whenever the skill triggers; detail belongs in
  `references/`, which the agent reads only when it needs to.
- **Commands in ` ```bash ` blocks must parse.** A unit test runs every line
  there that starts with `memorylake` through the CLI's own argument parser,
  so renaming a command or flag fails the build until the skills follow.
  Write them as concrete examples with stand-in ids (`act-123`, `proj-123`).
  Syntax summaries with `[optional]` or `<placeholder>` parts go in ` ```text `
  blocks, which are not checked.
- Write for an agent acting for a user: say which commands change data and
  that the user confirms those first.
