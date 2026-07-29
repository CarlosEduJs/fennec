---
name: develop
description: >
  End-to-end development workflow for implementing features, fixes, or refactors.
  Orchestrates the other workflows (validate, commit, create-pr, create-changeset)
  into a complete development cycle.
triggers:
  - when starting work on a new feature or fix
  - when asked to "implement", "build", or "develop" something
  - as the default workflow for any coding task
skills:
  - coding-guidelines
  - rust-pragmatic-code
---

# Development Workflow

Complete end-to-end workflow for implementing changes in the fncc workspace.

## Phase 1: Prepare

### Understand the Task

1. Read the AGENTS.md for project context
2. Identify which crates are affected
3. Read relevant skills (see `use-skills` workflow)

### Create a Branch

```bash
git checkout main
git pull origin main
git checkout -b <type>/<short-description>
```

## Phase 2: Implement

### Development Loop

1. **Write code** — follow skills guidelines
2. **Check quickly** — `cargo check --workspace`
3. **Test incrementally** — `cargo test -p <affected-crate>`
4. **Iterate** until the feature works

### Project-Specific Reminders

- `.fui` changes → modify parser in `fncc-core` + update codegen
- Proc-macro changes → modify `fncc-macros`, test with `fncc-example`
- Runtime additions → add to `fncc-runtime`, re-export from `fncc`
- All commands run from workspace root (`/home/carlos/dev/fncc`)

## Phase 3: Validate

Follow the `validate` workflow:

```bash
taplo format --check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Fix any issues before proceeding.

## Phase 4: Commit

Follow the `commit` workflow:

```bash
git add <files>
git commit -m "<type>(<scope>): <description>"
```

## Phase 5: Changeset (if applicable)

If changes affect published crates, follow `create-changeset` workflow:

```bash
cargo run -p xtask -- change
# Edit the generated file
git add .changes/
git commit -m "chore: add changeset"
```

## Phase 6: Push & PR

Follow the `create-pr` workflow:

```bash
git push -u origin <branch>
gh pr create --base main --title "<type>(<scope>): <desc>" --body "..."
```

## Quick Reference: Crate Map

| What to change | Crate | Key files |
|----------------|-------|-----------|
| `.fui` parsing | `fncc-core` | `crates/fncc-core/src/parser/` |
| Rust codegen | `fncc-core` | `crates/fncc-core/src/codegen/` |
| `#[fncc::command]` | `fncc-macros` | `crates/fncc-macros/src/` |
| GPUI wrappers | `fncc-runtime` | `crates/fncc-runtime/src/` |
| Umbrella re-exports | `fncc` | `crates/fncc/src/` |
| Release tooling | `xtask` | `xtask/src/` |
| Example app | `fncc-example` | `apps/fncc-example/` |
