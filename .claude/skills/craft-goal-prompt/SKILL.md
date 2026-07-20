---
name: craft-goal-prompt
description: Interactively clarify ambiguous, difficult software-engineering work and turn the confirmed requirements into a standalone, platform-neutral goal prompt for coding agents such as Codex or Claude Code. Use when a user wants to formulate, refine, or audit a prompt for a complex coding, debugging, refactoring, migration, performance, reliability, security, or repository-level task, especially when important context is implicit, completion is hard to define, or premature implementation would be risky.
---

# Coding Goal Prompt

Convert an initially incomplete coding request into a precise task contract. Clarify before composing. Optimize the final prompt for outcome ownership, implementation freedom, verifiable completion, and resistance to plausible-but-incomplete work.

Do not solve or implement the underlying coding task unless the user separately asks for that work. The deliverable of this skill is the prompt.

## Governing Principles

- Treat the user's first description as an intake, not a complete specification.
- Never silently invent requirements, architecture, constraints, priorities, or edge-case behavior.
- Distinguish confirmed facts, evidence-backed observations, user-approved assumptions, proposals, and unknowns.
- Specify the destination and acceptance contract more tightly than the route. Prescribe a method only when safety, policy, reproducibility, or the task itself requires it.
- Make non-completion explicit. Name attractive partial results that must not be mistaken for success.
- Require evidence proportional to risk. Avoid generic demands that cannot be checked.
- Keep the output platform-neutral by default. Add tool, command, agent, CI, or platform details only when the user confirms them.
- Write the final prompt in English unless the user explicitly requests another language. Conduct clarification in the user's language unless asked otherwise.

## Maintain a Requirements Ledger

Maintain a working ledger throughout the conversation with these states:

- **Confirmed**: explicitly stated or confirmed by the user.
- **Observed**: supported by artifacts or read-only inspection; still separate observation from interpretation.
- **Proposed**: a recommendation awaiting the user's decision.
- **Unknown**: missing information that may affect the prompt.
- **Conflict**: statements or constraints that cannot all hold simultaneously.

Never promote an observed inference or a proposed default to Confirmed without user approval. When the user authorizes judgment on a specific issue, record the chosen assumption and its rationale explicitly.

## Stage 1: Clarify Interactively

Start by restating the requested outcome in one or two sentences, including the most consequential uncertainty. Then ask only the smallest useful batch of questions, normally one to three.

Choose questions dynamically. Ask the highest-information questions first; do not dump a universal questionnaire. Build later questions from earlier answers. Prefer concrete contrasts, examples, and boundary scenarios over abstract wording.

Probe only categories that can materially change the task contract:

- business or user outcome and why the change is needed;
- current behavior, desired behavior, and a reproducible example;
- repository, component, runtime, architecture, and relevant artifacts;
- in-scope work, explicit non-goals, and ownership boundaries;
- public interfaces, data contracts, compatibility, and migration expectations;
- correctness invariants and behavior for empty, invalid, partial, duplicate, boundary, or large inputs;
- concurrency, retries, cancellation, timeouts, partial failure, rollback, and idempotency where relevant;
- security, privacy, permissions, compliance, and destructive-operation risk where relevant;
- performance or resource budgets, including how they will be measured;
- observability, deployment, rollout, backward compatibility, and operational recovery where relevant;
- required tests, validation environments, acceptance evidence, and deliverables;
- target agent environment or capability constraints, if platform-neutral instructions are insufficient.

Treat this list as a set of probes, not requirements to inject into every prompt. Mark a category not applicable when that conclusion is supported; otherwise leave it Unknown.

### Use Available Context Carefully

Read artifacts the user supplies. Perform read-only repository inspection only when the repository or files are clearly in scope. Use inspection to replace unnecessary factual questions, but ask the user about intent, priorities, product semantics, and policy.

Do not modify files, run destructive commands, contact external systems, or begin the target implementation while using this skill unless separately authorized.

### Handle Answers and Ambiguity

- If an answer introduces a contradiction, surface the conflict immediately and ask the user to resolve it.
- If an answer is vague, ask for a concrete example, observable behavior, threshold, or decision rule.
- If the user does not know, offer a small number of clearly labeled proposals with tradeoffs; do not select one silently.
- If the user says to use judgment, state the proposed decision and rationale and obtain confirmation with the rest of the ledger.
- If new information invalidates earlier confirmation, revise the ledger and call out the change.
- Do not repeat questions already answered by the conversation or artifacts.

Continue until no unresolved issue can materially change scope, behavior, acceptance, or safety.

## Stage 2: Obtain a Specification Checkpoint

Before writing the final prompt, present a concise checkpoint containing:

1. Objective and motivation
2. Confirmed current and desired behavior
3. Scope and non-goals
4. Constraints and invariants
5. Relevant edge and failure cases
6. Deliverables and acceptance evidence
7. Approved assumptions and their rationale
8. Remaining unknowns or conflicts

Ask the user to confirm or correct this checkpoint explicitly. Do not produce the final prompt while material unknowns or conflicts remain. Do not treat silence as confirmation.

If the user requests an early draft, label it as a draft, preserve unresolved items as explicit questions or placeholders, and do not present it as execution-ready. Still require a confirmed checkpoint before producing the final execution-ready prompt.

## Stage 3: Compose the Goal Prompt

After confirmation, read [references/goal-prompt-contract.md](references/goal-prompt-contract.md) completely and use its contract and template. Include only sections relevant to the confirmed task; do not emit empty headings or placeholders.

Make the prompt self-contained. A capable coding agent should be able to distinguish completion from partial progress without access to the clarification conversation.

Translate user language into precise English by default while preserving identifiers, commands, paths, API names, quoted messages, and domain terms exactly when translation would change meaning.

When the user names a target platform, adapt syntax and capability references to that platform. Otherwise:

- avoid product-specific tool names, directives, modes, and agent counts;
- express parallel exploration or independent review as capability-conditional behavior;
- never claim that subagents, browser access, network access, or a particular CI system exists;
- keep commands illustrative only if the confirmed environment supports them.

## Stage 4: Audit Before Delivery

Audit the generated prompt against the confirmed ledger:

- Every mandatory behavior is represented exactly once or without contradiction.
- No unconfirmed requirement or architecture has become mandatory.
- Each acceptance criterion is observable and has a plausible evidence source.
- The prompt distinguishes implementation quality from mere test passing.
- Scope, non-goals, failure handling, compatibility, and destructive actions are explicit where relevant.
- The prompt identifies realistic partial outcomes that do not count as completion.
- Execution guidance leaves room for discovery while preventing goal substitution.
- Blocking and escalation rules do not encourage guessing or endless work.
- The output is standalone, platform-neutral unless requested otherwise, and in the requested language.
- No unresolved placeholder, editorial note, or clarification transcript remains.

If the audit exposes a material gap, return to clarification rather than repairing it with an assumption.

Deliver only the final prompt in a fenced text block, preceded by a one-sentence label. Do not append implementation advice unless requested.
