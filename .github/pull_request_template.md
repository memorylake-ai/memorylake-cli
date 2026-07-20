<!--
PR Title format: `<type>: <Subject>`

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

**IMPORTANT NOTE**: This is only a guide of how PR title should be formatted. 
Do NOT include the title at the beginning of PR body!
-->

## Summary

<!--
One concise paragraph describing WHAT was added/changed and WHY.
For features: describe the new capability and its purpose.
For fixes: describe the root cause and the fix approach.
For refactors: describe what moved/changed and the motivation.
-->

## Changes

<!--
Break down changes by module or file. Use bullet points.
Bold the filename/module, then describe what changed.

Example:
- **yallm/foo.py**: New `FooClass` with sync/async context manager support
- **pyproject.toml**: Added `somelibrary==1.2.3`
- **csrc/bar.cpp**: Comprehensive tests covering X, Y, Z
-->

- **`<file>`**: 

## Test plan

<!--
List the steps to verify this PR is correct. Check off items as they pass.
Include both automated and manual verification steps.

For example:
- [ ] All existing tests pass (`pytest`)
- [ ] New tests pass (if applicable)
- [ ] (... and more)
-->
