---
name: create-changeset
description: >
  Create a changeset file to declare version bumps for published crates.
triggers:
  - after making changes to published crates
  - when asked to "add a changeset" or "prepare a release"
---

# Create Changeset Workflow

Declare version changes for the automated release pipeline.

## When Required

A changeset is required when a PR modifies any published crate: `fncc`, `fncc-core`, `fncc-macros`, or `fncc-runtime`.

**Not needed** for: CI workflows, docs-only, `fncc-example`, `xtask`, `.agents/`.

## Steps

### 1. Generate

```bash
cargo run -p xtask -- change
```

Creates `.changes/<crate>-<timestamp>.md`.

### 2. Edit the File

```markdown
---
<crate-name>: <bump-level>
---

- Description of what changed
```

Bump levels: `patch` (bug fix), `minor` (new feature), `major` (breaking change).

Multi-crate example:

```markdown
---
fncc-core: minor
fncc-runtime: patch
---

- Add conditional rendering support (core parser + runtime helpers)
```

### 3. Commit and Validate

```bash
git add .changes/
git commit -m "chore: add changeset for <description>"
cargo run --locked -p xtask -- check
```

## Release Pipeline Flow

1. PR merges to `main` with `.changes/` files → CI runs `xtask release-pr`
2. CI bumps versions, updates changelogs, creates `chore/release-packages` PR
3. When that PR merges (no `.changes/` left), CI publishes to crates.io

Publish order: `fncc-macros` → `fncc-core` → `fncc-runtime` → `fncc`.
