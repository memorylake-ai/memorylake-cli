---
name: memorylake-cli
description: Operate MemoryLake from the terminal with the `memorylake` CLI - check the login and workspace, find the right command, and follow its conventions (JSON output, paging, ids, irreversible deletes). Use whenever a task involves MemoryLake workspaces, projects, files, memories, agents, permissions or team settings and no more specific memorylake-cli-* skill fits.
---

# MemoryLake CLI

`memorylake` manages everything in a MemoryLake account: workspaces and
projects, files and databases, memories, agents and skills, the team and who
may reach what. Every command prints the API response as pretty JSON.

If the MemoryLake harness plugin is installed, routine recall during a coding
session goes through it (`ml-recall`). This CLI is for managing MemoryLake
explicitly.

## Before the first command

```bash
memorylake auth status      # logged in? which endpoint and profile
memorylake ws current       # which workspace commands default to, and why
```

- **Not logged in:** ask the user to run `memorylake auth login` themselves.
  Never ask for an API key, type one into a command, or echo one back.
- **No workspace:** run `memorylake ws list` and ask which one to use.
  `ws use <id>` changes the user's default, so only run it when they agree;
  otherwise pass `--workspace <id>` to each command.

## Conventions

- Output is JSON on standard output; pipe it through `jq`. Notes and errors go
  to standard error.
- `memorylake <command> --help` is the authority on flags. Check it rather
  than guessing.
- Lists page manually: pass the response's `continuation_token` back as
  `--continuation-token` until it is absent.
- Most `get` commands accept `--by-custom-id` to address an object by the
  custom id the caller gave it.
- The exit status is meaningful. A batch that partly fails (document import,
  fact delete) prints its full result first, then exits non-zero.
- Aliases: `ws` workspace, `proj` project, `lib` library, `doc` document,
  `conv` conversation, `msg` message, `dbconn` db-connection, `ds` datasource,
  `key` api-key, `invite` invitation, `perm` permission, `ag` access-group.

## Safety rules

- **Deletes are immediate and irreversible.** No prompt, no undo. Before any
  delete, remove, revoke, or overwrite, show the user exactly what will be
  affected and get an explicit yes.
- `--metadata` and `--tags` on an update **replace** the stored value whole;
  they never merge.
- Commands that print a secret (`key create`, `key rotate`, `team create
  --issue-api-key`) print it exactly once. Hand it to the user; do not write
  it to files or logs unless they ask.

## Command map

| Area | Commands |
| --- | --- |
| Workspaces and projects | `ws`, `proj` |
| Files and documents | `lib`, `proj document` |
| Databases | `dbconn`, `ds`, `proj database` |
| Memory | `actor`, `fact`, `conv`, `search`, `boundary` |
| Agents and skills | `agent`, `skill` |
| Team | `team`, `key`, `member`, `invite`, `usage`, `billing`, `audit-log`, `integration` |
| Access control | `role`, `perm`, `ag`, `access` |
