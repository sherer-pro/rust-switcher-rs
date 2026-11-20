# Branch protection

Open pull requests against `main`. The release and Dependabot controllers require
successful `ci-gate` and `workflow-security-gate` checks on the exact PR head.
Configure both as required status checks when enabling branch protection in GitHub.

To inspect the live classic protection and applicable rulesets from this checkout:

```powershell
gh api 'repos/{owner}/{repo}/branches/main/protection'
gh api 'repos/{owner}/{repo}/rules/branches/main' --paginate
```

## PR flow

```powershell
# From the feature branch
git push -u origin HEAD
gh pr create --base main --fill
```
