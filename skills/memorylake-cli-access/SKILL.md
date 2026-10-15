---
name: memorylake-cli-access
description: Diagnose and change who may do what in a MemoryLake team with the `memorylake` CLI - roles, permission rules, access scopes, access groups and per-instance permission levels. Use when someone cannot see or change a workspace, project, actor, agent, boundary, database or skill, when the user wants to give or take away access, set up a least-privilege identity for automation, or understand why a member has a permission.
---

# MemoryLake access control

Read the `memorylake-cli` skill first if the login is not yet confirmed.

## Members, not actors

Access control is about **members**: the people in the team, and virtual
members that act only through API keys. Every command here takes a member's
**principal id** from `memorylake member list`. That is not the actor id
memories are attributed to; never pass one where the other is asked for.

## Four layers decide what a member can do

| Layer | Decides | Effect | Commands |
| --- | --- | --- | --- |
| **Role** | which actions | grants | `role`, `perm rule` |
| **Access scope** | which instances, per resource type | **only narrows**; never more than the role | `member scope`, `role scope` |
| **Access group** | extra actions and extra instances | **only adds** | `ag` |
| **Permission level** | `read` / `write` / `manage` on one instance, per member | sets the level there | `access set-level` |

A member can do something when a role or group grants the action and the
instance is within reach. Details:
[references/model.md](references/model.md).

## What does the user want?

| The user says | Do this | Reference |
| --- | --- | --- |
| "Why can't Alice open this project?" | the diagnosis below | diagnose.md |
| "What can I do here?" | `perm me` | diagnose.md |
| "Who can reach this workspace?" | `access show workspace <id>` | diagnose.md |
| "Give Bob access to project X" | `access grant`, or `access set-level` | change.md |
| "Bob should only see project X" | `member scope set` | change.md |
| "Make a role for reviewers" | `role create`, then `perm rule create` | change.md |
| "A bot for CI with minimal rights" | virtual member + custom role + key | recipes.md |
| "Take away everything Carol has" | recipe "Revoke a member's access" | recipes.md |

Resource types and their ids:
[references/resource-types.md](references/resource-types.md).
Whole workflows: [references/recipes.md](references/recipes.md).

## Diagnose before you change

Reading is safe and needs no confirmation. Answer "why can't X do Y" from the
output, naming the layer that blocks:

```bash
memorylake member list --name alice
memorylake access show project proj-123
memorylake member explain usr-123
memorylake member scope get usr-123
memorylake access levels project proj-123 --member usr-123
```

Finish with a verdict such as "Alice's role grants `project:read`, but her
access scope limits projects to proj-456, so proj-123 is out of reach."
Then propose the smallest change that fixes it.

## Rules for changes

1. **Confirm every write.** Say what will change, for whom, and what it
   unlocks, then wait for an explicit yes. Access changes take effect at once.
2. **Least privilege.** Grant the narrowest action on the fewest instances
   that does the job. Prefer read over write, one project over all projects.
3. **Prefer groups for several people.** A grant to an access group is one
   change to review and one to undo; per-member grants drift.
4. **Every write needs `--reason`.** It goes into the audit log. Write what
   was asked and by whom, for example `"Alice needs to review Atlas docs, per
   Bob 2026-10-15"`.
5. **Read, edit, write back with `--if-match`.** For scopes and group
   permissions, pass the ETag the read printed, so a concurrent change is not
   overwritten.
6. **Verify afterwards** with `member explain` or `access show`, and report
   the result rather than assuming it.
7. **Some changes cannot be made, by design.** Built-in roles never change;
   only the team owner manages roles; nobody can change their own role; the
   last member who can manage an instance cannot be lowered. Explain the
   refusal instead of looking for a way around it.
