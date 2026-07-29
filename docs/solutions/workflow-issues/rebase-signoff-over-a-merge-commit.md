---
title: Adding sign-off to a branch that contains a merge commit
date: 2026-07-28
category: workflow-issues
module: version-control
problem_type: workflow_issue
component: development_workflow
severity: medium
applies_when:
  - A DCO or sign-off check fails on a branch whose history already contains a merge
  - Commit messages need rewriting but trees must not change
  - "`git rebase` conflicts on files the branch deliberately deleted"
tags: [git, rebase, dco, sign-off, merge-commit, filter-branch]
---

# Adding sign-off to a branch that contains a merge commit

## Context

A long-lived branch failed its DCO check: 25 commits carried no `Signed-off-by`
trailer. The branch had also merged `main` in earlier to resolve conflicts, so
its history contained a merge commit.

The obvious fix looked like counting the commits and rebasing:

```bash
git rebase HEAD~27 --signoff
```

That put the working tree into a conflicted rebase that could not be finished
without hand-resolving conflicts in files the branch exists to delete.

## Guidance

**To add a trailer to existing commits, rewrite messages -- do not replay
patches.** `git rebase` replays each commit as a patch, so it can conflict.
`git filter-branch --msg-filter` rewrites commit messages and reuses the
existing trees verbatim, so a conflict is structurally impossible.

```bash
git filter-branch -f --msg-filter '
msg=$(cat)
if printf "%s" "$msg" | grep -q "^Signed-off-by: "; then
    printf "%s\n" "$msg"
else
    printf "%s\n\nSigned-off-by: Your Name <you@example.com>\n" "$msg"
fi
' origin/main..HEAD
```

The guard matters: without it, commits that were already signed get a second
identical trailer.

**Verify by tree SHA, not by eyeball.** The whole claim of this approach is that
nothing but messages changed, and that is directly checkable:

```bash
git rev-parse 'HEAD^{tree}'          # before the rewrite
git rev-parse 'HEAD^{tree}'          # after -- must be identical
git diff <pre-rewrite-sha> HEAD      # must be empty
```

Record the pre-rewrite SHA before starting. It stays reachable through the
reflog, but having it written down turns recovery into one command.

## Why This Matters

`git rebase HEAD~N` **linearizes** history. When the range spans a merge commit,
the commits that were merged in stop being "already incorporated" and become
individual patches to replay onto the branch.

On a branch that deletes a subtree, this is maximally bad. The merged-in commits
were dependency bumps to that subtree:

```
pick 2b2953b chore(deps): bump the minor-and-patch group ...
pick 3bacc1e chore(deps): bump clap_complete ...
pick c318420 chore(deps): bump jsonschema ...
```

Each edits files the branch removed, so each conflicts, and each conflict has to
be resolved by hand only for the resolution to be thrown away. The conflict count
scales with how much the branch deleted -- exactly backwards from useful.

The failure is also confusing to diagnose, because the conflict markers name
commits the developer never wrote and did not think were part of their branch.

## When to Apply

- Any trailer-only rewrite: sign-off, `Co-authored-by`, issue references
- Any history rewrite where the trees must be provably unchanged
- Whenever `git rebase` conflicts on a file the branch intentionally deleted --
  that is the signal that a merge is being flattened

Reach for `rebase` when you genuinely want to replay work onto a new base.
Reach for `filter-branch` when the trees are already right and only metadata is
wrong.

## Examples

Recovering from the failed attempt:

```bash
git rebase --abort          # restores the branch exactly
git rev-parse HEAD          # record this before rewriting
```

After the rewrite, the push needs `--force-with-lease` rather than `--force`, so
it refuses if the remote moved underneath you:

```bash
git push --force-with-lease origin <branch>
```

## Related

- `git filter-branch` prints a deprecation warning; `FILTER_BRANCH_SQUELCH_WARNING=1`
  silences it. `git filter-repo` is the modern replacement and offers
  `--message-callback` for the same job, but it is a separate install.
- `git rebase --rebase-merges` preserves merge structure and still replays
  patches, so it does not avoid the conflicts here.
