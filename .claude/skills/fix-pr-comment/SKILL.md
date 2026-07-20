---
name: fix-pr-comment
description: Resolve GitHub PR review comments for this repository using gh CLI. Use when given a PR URL or PR number, with or without a specific comment, and the task requires checking out the source branch, fixing valid comments, adding targeted tests when needed, running the relevant repository format and check scripts, pushing safely, and replying to each handled comment.
---

# Fix PR Comment

Use this skill when:

- The user asks to address GitHub PR review comments
- The input includes a PR URL, PR number, or enough context to identify the open PR
- The task may target all comments or one specific comment only

## Quick Rules

- Assume `gh` is already authenticated. If a `gh` command fails due to an authentication-related error, stop and ask the user to run `gh auth login`.
- Work on the PR source branch, not the base branch.
- Fetch from GitHub and make sure the local source branch is up to date before making changes.
- Never force push.
- Do not revert unrelated local changes. If unexpected changes appear while you work, stop and ask the user how to proceed.
- Ignore invalid, obsolete, or false-positive comments unless a clarifying code comment or doc update would prevent future confusion.
- If no code or doc changes are needed, skip commit and push, then reply to the relevant comments.
- Before any push, run `REPO_DIR/cicd/format-all.sh` and `REPO_DIR/cicd/check-all.sh`.
- For valid comments, add or update focused tests when they materially reduce regression risk, and make sure those tests pass before push.
- Do not default to `REPO_DIR/cicd/test-all.sh`. It is slow and environment-dependent. Only use it when a smaller test cannot cover the change. If you must run it, use a timeout of at least 1200 seconds and redirect output to a log file.
- If format or check scripts fail because the environment is missing, prepare it with `REPO_DIR/cicd/setup-devenv.sh` before retrying.

## Workflow

1. Determine the repository and PR.
   - Set `REPO_DIR=$(git rev-parse --show-toplevel)`.
   - If the user provides a PR URL or number, inspect it with `gh pr view <pr> --json number,headRefName,baseRefName,url,title`.
   - If the user does not provide a PR identifier, discover the open PR for the current branch with `gh pr view`.
   - If `gh pr view` does not resolve to an open PR, stop and ask the user for the PR URL or PR number.

2. Check out and update the source branch.
   - Run `git fetch origin`.
   - Prefer `gh pr checkout <pr>` when starting from a PR URL or number.
   - If already on the correct branch, update it with a fast-forward or `git pull --rebase origin <headRefName>`.
   - Make sure local code matches the latest remote head before evaluating comments.

3. Gather the comments to handle.
   - Fetch the inline review comments and review threads with `gh`.
   - If the task refers to general PR discussion, also inspect PR-level issue comments or review summaries so you do not miss non-inline feedback.
   - If the user specified one comment, limit the work to that comment only.
   - For each comment or thread, capture enough context to act and reply: comment id, thread id if available, file path, line, body, author, and whether the thread is outdated or already resolved.

4. Triage each comment.
   - `Valid`: the reviewer identified a real problem or a worthwhile improvement. Fix it locally.
   - `Obsolete`: the branch already addresses it. Do not change code again.
   - `False positive` or `misunderstanding`: do not change the implementation unless a clarifying comment or documentation change would help future readers.
   - Keep changes minimal and scoped to the comment being addressed.

5. Apply changes for valid comments.
   - Fix the code or documentation locally.
   - Add or update focused tests when needed to protect the changed behavior from regression.
   - Prefer targeted tests near the changed code over broad suites.
   - If no test is added for a valid fix, be ready to explain why the change does not benefit from an additional automated test.

6. Verify locally.
   - Run the most focused tests that cover the changed behavior.
   - Any test you add or affect must pass before push.
   - Run `REPO_DIR/cicd/format-all.sh`.
   - Run `REPO_DIR/cicd/check-all.sh`.
   - If anything fails, fix the root cause and rerun the relevant tests, then rerun format and check until everything passes.

7. Commit and push if there is a diff.
   - If there is no code or doc diff, skip to replies.
   - Stage only the relevant changes.
   - Create a normal commit with a concise message describing why the comment was addressed.
   - Push to the PR source branch.
   - If the push is rejected because the remote branch moved:
     1. Run `git fetch origin`.
     2. Rebase with `git pull --rebase origin <headRefName>`.
     3. Resolve conflicts carefully without dropping either side's valid changes.
     4. Rerun the relevant tests plus `REPO_DIR/cicd/format-all.sh` and `REPO_DIR/cicd/check-all.sh`.
     5. Push again.
   - Never use `git push --force`.

8. Reply to the comments after the branch is updated.
   - Reply to every handled comment, whether it was valid or not.
   - If no code changed, still reply with the reason.
   - If only one specific comment was requested, reply only to that comment.
   - Prefer comment-specific replies over a generic PR-level comment.
   - Only say a comment is fixed after the final branch state is on remote.

## Useful `gh` Patterns

- Resolve the current repository name:
  - `OWNER_REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)`
- Inspect PR metadata:
  - `gh pr view <pr> --json number,headRefName,baseRefName,url,title`
- List review comments:
  - `gh api repos/$OWNER_REPO/pulls/<pr_number>/comments --paginate`
- Reply to a review comment:
  - `gh api repos/$OWNER_REPO/pulls/comments/<comment_id>/replies -f body='...reply text...'`
- Add a general PR comment only when there is no comment-specific reply target:
  - `gh pr comment <pr> --body '...message...'`

Use GraphQL via `gh api graphql` if you need review thread metadata that is not available in the default REST response, such as thread ids or thread resolution state.
For multi-line reply bodies or text containing quotes, prefer `--input -` or a temporary file over fragile inline shell quoting.

## Reply Guidance

- Valid and fixed:
  - `Fixed on the PR branch. I also added or updated focused verification and confirmed it passes locally.`
- Valid and fixed without a new test:
  - `Fixed on the PR branch. I did not add a new automated test because <brief reason>.`
- Obsolete:
  - `No change was needed because this is already addressed on the current PR branch.`
- False positive or clarification:
  - `I did not change the implementation here because <brief reason>.`
  - If helpful: `I added clarification in <file> so the intent is easier to read.`

Keep replies short, direct, and specific to the comment.
