---
name: review-changes
description: >
  Review local uncommitted or staged changes before committing. A lighter-weight
  review focused on catching issues before they become commits. Use the review-pr
  workflow for full PR-level reviews.
triggers:
  - when asked to "review changes" or "check my changes"
  - before committing to verify quality
  - after making modifications to review them
skills:
  - coding-guidelines
  - rust-pragmatic-code
---

# Review Local Changes Workflow

Quick review of uncommitted or staged changes to catch issues before they become commits.

## Steps

### 1. Understand What Changed

```bash
# Overview of changed files
git status

# Summary of changes by file
git diff --stat

# Full diff of unstaged changes
git diff

# Full diff of staged changes
git diff --cached

# Both staged and unstaged
git diff HEAD
```

### 2. Quick Validation

Run the fast checks first:

```bash
# Format check (instant)
cargo fmt --all --check

# Type check (fast, no codegen)
cargo check --workspace

# Lints (medium speed)
cargo clippy --workspace --all-targets -- -D warnings
```

### 3. Review Each Changed File

For each modified file, check:

| Check | Question |
|-------|----------|
| **Intent** | Does this change match what was intended? |
| **Completeness** | Are there TODOs or placeholder code left behind? |
| **Consistency** | Does the new code match the style of surrounding code? |
| **Dead code** | Were any imports, functions, or variables left unused? |
| **Tests** | Do existing tests still apply? Are new ones needed? |
| **Comments** | Were existing unrelated comments preserved? |

### 4. Targeted Testing

Run tests only for affected crates:

```bash
# Identify which crates were touched
git diff --name-only | grep "^crates/" | cut -d'/' -f2 | sort -u

# Run tests for those crates
cargo test -p <crate-name>
```

### 5. Report

Provide a concise summary:

```markdown
## Changes Review

**Files changed**: N files (+X, -Y lines)

### ✅ Looks Good
- List of changes that are clean and ready

### ⚠️ Needs Attention
- Issues found with specific file:line references
- Suggested fixes

### 📋 Recommendation
Commit as-is / Fix issues first / Split into multiple commits
```

## When to Escalate to Full PR Review

Use the `review-pr` workflow instead when:
- Changes span more than 5 files
- Changes modify public API surfaces
- Changes touch the parser or codegen pipeline
- Changes affect the proc-macro
