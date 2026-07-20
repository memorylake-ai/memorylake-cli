# Goal Prompt Contract for Difficult Coding Tasks

Use this reference only after the user confirms the specification checkpoint. It defines the content model for the final execution-ready prompt. It is a template, not a fixed form: include a section only when it carries confirmed, task-relevant information.

## Contents

1. [Contract properties](#contract-properties)
2. [Recommended prompt shape](#recommended-prompt-shape)
3. [Writing acceptance criteria](#writing-acceptance-criteria)
4. [Defining non-completion](#defining-non-completion)
5. [Execution and review policy](#execution-and-review-policy)
6. [Blockers and return conditions](#blockers-and-return-conditions)
7. [Final quality checks](#final-quality-checks)

## Contract Properties

An execution-ready goal prompt must be:

- **Faithful**: contain no requirement that the user did not confirm or authorize as an assumption.
- **Self-contained**: define enough context, terms, paths, interfaces, and constraints to execute without the prior interview.
- **Outcome-led**: define observable end state without imposing an arbitrary implementation sequence.
- **Bounded**: say what is in scope, what is not, and which existing behavior must remain unchanged.
- **Auditable**: map mandatory outcomes to concrete validation evidence.
- **Adversarially robust**: anticipate plausible shortcuts, regressions, and false-positive completion claims.
- **Operationally responsible**: define how to handle new uncertainty, destructive actions, external dependencies, and genuine blockers.

Do not add theatrical persistence requirements, arbitrary time quotas, unsupported scale claims, or generic production language. Convert importance into task-specific invariants and evidence.

## Recommended Prompt Shape

Use direct imperative language. Replace bracketed guidance with confirmed content and remove all unused sections.

```text
# Current Task

[State the coding task and relevant system context in concrete terms. Include the reason for the change when it affects decisions.]

## Goal

[Define the required end state in observable terms. State what must be true when the task is complete.]

## Current and Desired Behavior

[Describe verified current behavior, desired behavior, and representative examples or reproductions. Clearly label observations versus requirements.]

## Scope

[List components, interfaces, data, environments, or workflows included in the work.]

## Non-Goals and Preservation Requirements

[List excluded work and behavior, APIs, data, compatibility, or user changes that must not be disturbed.]

## Definitions and Contracts

[Define domain terms, public interfaces, schemas, invariants, compatibility rules, and exact semantics that could otherwise be interpreted differently.]

## Required Outcomes

[State mandatory functional and non-functional outcomes. Express each as an invariant or externally observable behavior where possible.]

## Edge Cases and Failure Behavior

[Specify only relevant confirmed cases: empty or invalid input, boundaries, duplicates, large inputs, concurrency, cancellation, timeouts, retries, partial failure, rollback, idempotency, permission failures, dependency outages, or corrupted state.]

## Constraints

[State confirmed technology, architecture, dependency, security, privacy, performance, compatibility, rollout, operational, and change-size constraints.]

## Acceptance Criteria and Evidence

[Provide testable completion criteria. For each important criterion, name acceptable evidence such as focused tests, integration tests, static checks, benchmarks, migration validation, logs, or a manual scenario.]

## What Does Not Count as Completion

[List likely partial or misleading outcomes: symptom masking, special-case-only fixes, disabled validation, tests that cannot fail, happy-path-only handling, unapproved API changes, data loss, unexplained snapshots, skipped required checks, or a reduction to an unresolved task. Include only credible task-specific failure modes.]

## Execution Policy

- Inspect and understand the relevant implementation and existing conventions before editing.
- Preserve unrelated user changes and avoid expanding the task beyond the confirmed scope.
- Choose the implementation approach based on evidence from the codebase; do not substitute a simpler goal.
- Validate assumptions against code, tests, documentation, or runtime evidence. Ask only when an unresolved decision would materially change behavior, scope, safety, or compatibility.
- Handle errors explicitly. Do not hide failures, weaken safeguards, or leave TODOs in required paths.
- Keep changes maintainable and proportionate to the task. Document non-obvious invariants and decisions where future maintainers need them.
- Run the strongest relevant validation available in the confirmed environment. Distinguish failures caused by the change from pre-existing or environmental failures.

[Add capability-conditional exploration or independent review rules here only for genuinely difficult tasks.]

## Deliverables

[List required code, tests, migrations, documentation, generated artifacts, benchmark results, or concise handoff information.]

## Blockers and Escalation

[Define what the agent may decide autonomously, what requires approval, and what evidence it must provide when blocked.]

## Completion and Return Conditions

[Require the agent to return only after the acceptance criteria are met and audited, or after a genuine blocker requires user or external action. Require an exact account of validation performed, any limitations, and any unmet criterion.]
```

The `Execution Policy` bullets are defaults, not sacred text. Retain, alter, or remove them to match the confirmed task. Do not let this section become a hidden rigid SOP.

## Writing Acceptance Criteria

Prefer criteria with this structure:

> Given [precondition or input], when [observable action or event], then [required result], evidenced by [specific validation].

Not every criterion needs Given/When/Then prose, but every mandatory claim needs a way to tell whether it is true.

Use the evidence appropriate to the risk:

- behavior change: focused regression test plus relevant integration coverage;
- concurrency or ordering: deterministic stress or synchronization test, not a timing-only sleep;
- performance: named workload, environment assumptions, metric, baseline, and threshold;
- migration: forward compatibility, existing-data validation, rollback or recovery behavior where confirmed;
- security or permissions: positive and negative authorization cases without exposing secrets;
- user interface: behavior at relevant states and viewport/input modes, with automated or manual evidence as confirmed;
- operational behavior: logs, metrics, alerts, health behavior, or failure injection when required.

Avoid criteria such as "works correctly," "is production-ready," "handles all edge cases," or "has good performance" unless converted into observable definitions.

Do not require 100% coverage, zero warnings, broad refactors, load testing, new telemetry, or documentation by default. Include them only when justified and confirmed.

## Defining Non-Completion

Infer likely false finishes from the task, then include only those the confirmed contract rules out. Common patterns include:

- fixing a visible example while the underlying invariant still fails;
- supporting one implementation, environment, input class, or code path when the scope is broader;
- catching or suppressing an error without restoring required state or behavior;
- changing tests, fixtures, thresholds, or mocks so the defect is no longer observed;
- introducing retries that duplicate side effects or create unbounded work;
- passing unit tests while violating an external contract or migration path;
- relying on an unproved compatibility assumption or undocumented dependency behavior;
- returning a design, reduction, or partial scaffold when working code is required;
- declaring success without running required validation or explaining why it could not run.

Write these as task-specific exclusions. Do not paste the whole catalog.

## Execution and Review Policy

For difficult tasks, encourage a diverse search without hard-coding unavailable orchestration features:

- Explore materially different hypotheses or designs when the path is uncertain.
- Track why a route is promising, falsified, or blocked; do not keep investing in a route that merely restates the original difficulty.
- Require concrete evidence from experiments, traces, tests, type checks, benchmarks, or minimal reproductions.
- Re-evaluate the plan when evidence contradicts the current theory.
- Separate construction from audit for high-risk changes. Review exact semantics, error paths, compatibility, concurrency, data integrity, security boundaries, and rollback only where relevant.

If the confirmed environment supports independent agents or parallel workers and the task benefits from them:

- Begin with a genuinely diverse portfolio of approach families rather than a fixed allocation by headcount.
- Preserve enough early independence that workers do not all inherit the same favored diagnosis or design.
- Maintain a lightweight registry grouped by underlying idea, evidence, and unresolved gap; redirect duplicate effort toward underexplored families.
- Require each worker to return a concrete patch, reproduction, invariant, experiment, trace, test, benchmark, falsification, or exact blocker rather than a status report.
- Mark a route blocked when its missing step is as difficult or uncertain as the original task. Reopen it only for a materially new mechanism or new evidence.
- Cross-pollinate only after independent routes expose their real strengths and gaps.
- Assign adversarial review independently from construction for any candidate that could satisfy the goal.

Do not prescribe worker counts, orchestration syntax, or tool names unless the user confirmed them. When independent workers are unavailable, apply the same reasoning principles sequentially instead of pretending parallelism exists.

Do not force multiple approaches when the solution is already direct and well-supported. Exploration serves uncertainty; it is not ceremony.

## Blockers and Return Conditions

Balance autonomy with safety:

- Let the coding agent make reversible, local implementation choices inside the confirmed contract.
- Require clarification before choosing among interpretations that materially alter externally visible behavior, data, security, scope, compatibility, or irreversible operations.
- For a genuine blocker, require the agent to report the exact unmet criterion, evidence gathered, alternatives attempted, and the smallest user or external action needed.
- Do not let the agent stop merely because the first approach failed.
- Do not require endless retries or conceal incomplete work. Completion and blocked status must remain distinguishable.

A strong return condition requires a compact evidence report: changed behavior, validation run and outcomes, remaining limitations, and any deviation from the contract. It must prohibit declaring completion when a mandatory criterion remains unmet.

## Final Quality Checks

Before delivering the prompt, verify:

1. Could two competent agents interpret any mandatory behavior differently? If so, clarify it.
2. Does every "must" correspond to confirmed intent and observable evidence?
3. Is any preferred implementation accidentally written as the goal?
4. Are relevant boundaries and failures specified without importing irrelevant boilerplate?
5. Could an agent pass the listed checks while still violating the user's actual outcome? Add the missing invariant.
6. Does the non-completion section target realistic shortcuts for this task?
7. Are new discoveries handled without either reckless guessing or unnecessary user interruption?
8. Are destructive or external actions properly bounded?
9. Are platform-specific assumptions absent unless confirmed?
10. Is the final prompt concise enough that the true objective remains prominent?
