---
name: memorylake-cli-memory
description: Store, find, correct, trace and curate long-term memory in MemoryLake with the `memorylake` CLI - facts owned by an actor, project or agent, conversations the server turns into facts, search, conflicts between facts, and the instructions that steer what gets remembered. Use when the user asks to remember or forget something in MemoryLake, asks what MemoryLake knows, wants a memory corrected or explained, imports a conversation into memory, or reviews contradictions.
---

# MemoryLake memory

Read the `memorylake-cli` skill first if login or the workspace is not yet
confirmed; every command here needs both.

## The model in five lines

- A **fact** is one remembered statement. It belongs to exactly one **owner**:
  an actor, a project, or an agent. Owners are addressed with `--actor`,
  `--project` or `--agent`, exactly one per command.
- An **actor** is who memories are about: a person or an identity. Your own is
  `memorylake actor me`. An actor must be **bound** to a workspace to have
  memory there.
- A **project** holds shared knowledge: its facts and its documents.
- A **conversation** is a message log the server reads in the background and
  turns into facts, deciding on its own which actor or project each one
  belongs to.
- **Search** ranks documents, facts and database results together.

Details: [references/model.md](references/model.md).

## What does the user want?

| The user says | Do this | Reference |
| --- | --- | --- |
| "Remember that I prefer X" | `fact add --actor <your actor>` | write.md |
| "Remember for this project that Y" | `fact add --project <id>` | write.md |
| "Save this chat / transcript to memory" | `conv create`, `conv msg append --wait` | write.md |
| "What do you know about Z?" | `search`, then judge each hit | read.md |
| "Show everything stored for X" | `fact list --actors` / `--projects` | read.md |
| "That memory is wrong" | find it, `fact trace`, then `fact update` or `fact delete` | maintain.md |
| "Why does memory say that?" | `fact trace`, `conv fact-actions` | maintain.md |
| "Memory contradicts itself" | `fact conflict list`, then `resolve` | maintain.md |
| "Only remember things about Q here" | `fact instruction draft`, then `set` | maintain.md |

Whole workflows, start to finish: [references/recipes.md](references/recipes.md).

## Choosing the owner

Decide the owner before writing, because a fact cannot be moved later:

- About a person (preferences, background, decisions they made): that
  person's **actor**. For the user themselves, `actor me`.
- About a body of work shared by several people: the **project**.
- What an agent should remember for its own work: the **agent**.

If it is not obvious, ask the user. Never guess between a personal and a
shared owner.

## Rules

1. **Write facts that stand alone.** One statement each, with names instead
   of pronouns and absolute dates instead of "yesterday". Write
   `Alice Chen prefers pnpm over npm (2026-10-15)`, not `she likes pnpm`.
2. **Show before you delete or overwrite.** `fact delete`, `fact update`,
   `conflict resolve` with `keep_fact` or `edit_fact`, `instruction set` and
   `conv delete` take effect at once and cannot be undone. List exactly what
   will change and get a yes first.
3. **`--metadata` replaces, never merges.** This applies to facts and actors
   alike; `--tags` on `actor update` too.
4. **Search without `--actors` covers only your own actor.** To include other
   people's memories, name them.
5. **Conversations lag.** Appended messages become facts only after the server
   processes them. Use `--wait`, or check `conv cook-status`, before telling
   the user the memory is there.
6. **Judge every search hit.** Results come back for any query, ranked. A
   high rank is not relevance; read the text.
7. **Never invent memory.** If nothing relevant comes back, retry once with
   different wording, then say so plainly.
8. **A failed command is not an empty result.** A non-zero exit means the
   operation did not happen; report the error rather than "nothing found".

## Quick reference

```bash
memorylake actor me
memorylake fact add --actor act-123 "Alice Chen prefers pnpm over npm"
memorylake fact list --actors act-123 --query pnpm
memorylake search "Alice Chen package manager preference" --actors act-123
memorylake fact trace --actor act-123 fct-123
memorylake fact update --actor act-123 fct-123 --text "Alice Chen prefers bun over pnpm (2026-10-15)"
memorylake fact delete --actor act-123 fct-123
memorylake fact conflict list --project proj-123 --resolved false
memorylake conv cook-status conv-123
```
