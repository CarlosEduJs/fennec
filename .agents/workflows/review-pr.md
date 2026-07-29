---
name: review-pr
description: >
  Review a GitHub pull request with structured analysis and actionable feedback.
  Use this workflow when reviewing code changes, either on open PRs or local diffs.
  References the coding-guidelines, rust-best-practices, and rust-pragmatic skills.
triggers:
  - when asked to "review a PR" or "review changes"
  - when reviewing code for quality, correctness, or style
  - before approving or requesting changes on a PR
skills:
  - coding-guidelines
  - rust-best-practices
  - rust-pragmatic-code
---

# Review Pull Request Workflow

Perform a structured code review with actionable feedback.

## Prerequisites

Before reviewing, read the relevant skills for this project:

1. `.agents/skills/coding-guidelines/SKILL.md` — Rust naming, formatting, and style rules
2. `.agents/skills/rust-best-practices/SKILL.md` — Idiomatic Rust patterns
3. `.agents/skills/rust-pragmatic/SKILL.md` — Avoid over-engineering

## Steps

### 1. Understand the PR Context

```bash
# List open PRs
gh pr list

# View a specific PR
gh pr view <number>

# Check out the PR locally
gh pr checkout <number>

# View the diff
gh pr diff <number>
```

For local (uncommitted) changes:

```bash
git diff
git diff --stat
```

### 2. Review Checklist

Go through each category in order. For each issue found, note the file, line, severity, and a concrete suggestion.

#### A. Correctness

- [ ] Does the code do what the PR description claims?
- [ ] Are edge cases handled (empty inputs, None values, error paths)?
- [ ] Are new `.fui` template features generating correct Rust code?
- [ ] Do state mutations in codegen correctly use `self.field` pattern?

#### B. Safety & Robustness

- [ ] No `unwrap()` on user-supplied or external data (use `expect()` with message or `?`)
- [ ] Error types are appropriate (see `rust-pragmatic` skill §1)
- [ ] No panics in library code paths (`fncc-core`, `fncc-macros`, `fncc-runtime`)
- [ ] Proc-macro errors use `compile_error!` or `syn::Error`, not `panic!`

#### C. Code Quality

- [ ] Follows naming conventions from `coding-guidelines` skill
- [ ] No unnecessary abstractions (see `rust-pragmatic` skill §2)
- [ ] No unnecessary `.clone()` calls (see `rust-pragmatic` skill §3)
- [ ] Functions are focused — one responsibility each
- [ ] Tests cover the new behavior

#### D. Project-Specific

- [ ] `.fui` codegen changes have corresponding parser tests
- [ ] `#[fncc::command]` macro changes maintain the 0/1/2 arg signature levels
- [ ] State interpolation (`state.field` → `self.field`) works correctly
- [ ] Unknown elements fall back to `div()` with `.attr()` calls
- [ ] Changes to published crates include a `.changes/` changeset file

#### E. CI Readiness

- [ ] `taplo format --check` passes
- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes

### 3. Classify Findings

Use these severity levels:

| Severity | Meaning | Action |
|----------|---------|--------|
| 🔴 **Blocker** | Bug, data loss, or correctness issue | Must fix before merge |
| 🟡 **Suggestion** | Improvement to quality or readability | Should fix, not blocking |
| 🟢 **Nit** | Style preference or minor polish | Optional, nice to have |
| 💬 **Question** | Needs clarification from the author | Discuss before deciding |

### 4. Write the Review

Structure the review as:

```markdown
## Review Summary

**Verdict**: Approve / Request Changes / Comment

Brief overall assessment of the PR.

## Findings

### 🔴 [Blocker] <title>
**File**: `path/to/file.rs:L42`
**Issue**: Description of the problem.
**Suggestion**: Concrete fix or approach.

### 🟡 [Suggestion] <title>
**File**: `path/to/file.rs:L88`
**Issue**: Description.
**Suggestion**: How to improve.

### 🟢 [Nit] <title>
**File**: `path/to/file.rs:L12`
**Note**: Minor observation.
```

### 5. Submit the Review (GitHub)

```bash
# Approve
gh pr review <number> --approve --body "LGTM! <optional comment>"

# Request changes
gh pr review <number> --request-changes --body "<review body>"

# Comment only
gh pr review <number> --comment --body "<review body>"
```

## Anti-Patterns to Watch For

These are common issues in this codebase specifically:

1. **Over-wrapping codegen output** — Generated Rust code should be as clean as hand-written code
2. **Breaking the state interpolation contract** — `state.field` in `.fui` MUST become `self.field` in codegen
3. **Adding elements without fallback** — Unknown elements must fall back to `div()`, not panic
4. **Missing changeset** — Any change to published crates needs a `.changes/` file
5. **Proc-macro panics** — `fncc-macros` must never panic; use `syn::Error` for all error reporting
