<!--
Commit title format: `<type>: <Subject>`

Subject: Use present tense and sentence case—capitalize only the first word and proper nouns/acronyms; do not use "Title Case Like This")
Types:
  feat:     New feature for the user (not a build script feature)
  fix:      Bug fix for the user (not a build script fix)
  docs:     Changes to documentation
  style:    Formatting, missing semi colons, etc; no production code change
  refactor: Refactoring production code (e.g. renaming a variable)
  test:     Adding or refactoring tests; no production code change
  chore:    Maintenance tasks (e.g. updating CI config); no production code change

Example:
  feat: Add this new feature to something
  fix: Fix this bug in something

The title is the first line of the commit message; leave a blank line before the body.
-->

<type>: <Subject>

<!--
One concise paragraph describing WHAT was added/changed and WHY.
For features: describe the new capability and its purpose.
For fixes: describe the root cause and the fix approach.
For refactors: describe what moved/changed and the motivation.
-->

<Body>

<!--
**IMPORTANT NOTE**: Do NOT add `Co-Authored-By` trailers
(e.g. for AI tools such as Cursor, Claude, Codex, etc.).
The commit message should contain only the title and body.
-->

<!--
Full Examples:

<FULL_EXAMPLE_1>
chore: Initialize production Rust CLI workspace

Add a multi-crate Cargo workspace with a thin `memorylake` binary and
`memorylake-core` library, plus rustfmt/clippy configs, GitHub Actions CI,
release profile settings, and MIT licensing so the project is ready for
feature work.
</FULL_EXAMPLE_1>
-->
