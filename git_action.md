# Git actions — cleanup on `feature/workspace`

Commit `9880013` added the repo-context code index (`.repository-context/structure.sqlite3*`,
~10 MB) to git, and it isn't in `.gitignore`. These steps stop tracking it and commit the new
`docs/DONE.md` entry. Run them from the repo root.

```bash
# 1. Ignore the local code index (the files stay on disk, so the index keeps working)
echo '/.repository-context/' >> .gitignore

# 2. Stop tracking it — removes it from git's index only, not from disk
git rm --cached -r .repository-context

# 3. Check: the first command should print nothing; status should list only
#    .gitignore, docs/DONE.md and the removed index files
git ls-files .repository-context
git status

# 4. Commit the cleanup and the DONE.md entry, then push
git add .gitignore docs/DONE.md
git commit -m "chore: untrack local code index; record OAuth renewal work in DONE.md"
git push
```

## Notes
- The 10 MB blob stays in `9880013`'s history on the branch. **Squash-merge** the PR so it never
  reaches `master`. Purging it from the branch would mean rewriting history and force-pushing,
  which isn't worth it here.
- Don't commit this file. Delete it when you're done: `rm git_action.md`.
