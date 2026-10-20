# Issue tracker: GitHub

Issues, specs, and the SYS-2.0 build-out tickets live as GitHub issues in `stijnklomp/boxgen`. Use the `gh` CLI for all operations.

## Conventions

- **Create an issue**: `gh issue create --title "..." --body "..."`. Use a heredoc for multi-line bodies.
- **Read an issue**: `gh issue view <number> --repo stijnklomp/boxgen --json number,title,body,labels,comments`.
- **List issues**: `gh issue list --repo stijnklomp/boxgen --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'` with appropriate `--label` and `--state` filters.
- **Make an issue a sub-issue of a parent**: `gh issue create --parent <parent> ...`, or `gh issue edit <parent> --add-sub-issue <child>` afterwards (`gh` 2.94+). Older `gh`: `gh api --method POST repos/stijnklomp/boxgen/issues/<parent>/sub_issues -F sub_issue_id=<child-db-id>` (database id, not the issue number). Without sub-issues, put `Part of #<parent>` at the top of the child body.
- **Comment on an issue**: `gh issue comment <number> --repo stijnklomp/boxgen --body "..."`.
- **Apply / remove labels**: `gh issue edit <number> --repo stijnklomp/boxgen --add-label "..."` / `--remove-label "..."`.
- **Close**: `gh issue close <number> --repo stijnklomp/boxgen --comment "..."`.

`gh` infers the repository from the git remote when run inside a clone. Pass `--repo` when running from a worktree or script where inference is ambiguous.

## Pull requests as a request surface

**PRs as a request surface: no.** Feature requests and bugs arrive as issues. Pull requests are reviewed on their merits; they do not enter the issue triage flow.

## When a skill says "publish to the issue tracker"

Create a GitHub issue in `stijnklomp/boxgen`.

## When a skill says "fetch the relevant ticket"

Read it with `gh issue view`, including labels and comments.

## Blocking and wayfinding

- **Blocking**: use GitHub's native issue dependencies: `gh api --method POST repos/stijnklomp/boxgen/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`, where `<blocker-db-id>` is `gh api repos/stijnklomp/boxgen/issues/<n> --jq .id` (not the `#number`). If dependencies are unavailable, put `Blocked by: #<n>, #<n>` at the top of the child body.
- **Frontier**: list the open children of the build-out milestone, drop any with an open blocker or an assignee; first in map order wins.
- **Claim**: `gh issue edit <n> --repo stijnklomp/boxgen --add-assignee @me` before starting work.
- **Resolve**: comment with the result, close the issue, and update the parent or milestone.
