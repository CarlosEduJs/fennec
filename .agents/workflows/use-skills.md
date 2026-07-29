---
name: use-skills
description: >
  Guide for discovering and applying the project's skills during development.
  Skills are specialized instruction sets that extend agent capabilities.
triggers:
  - when starting any Rust coding task
  - when reviewing or writing code
  - when unsure which skill applies to a task
---

# Use Skills Workflow

Skills are specialized instruction sets in `.agents/skills/`. Read the relevant
skill's `SKILL.md` before performing the associated task.

## Available Skills

| Skill | Path | When to Use |
|-------|------|-------------|
| **coding-guidelines** | `.agents/skills/coding-guidelines/SKILL.md` | Naming, formatting, style, lints |
| **rust-best-practices** | `.agents/skills/rust-best-practices/SKILL.md` | Idiomatic patterns, error handling, testing |
| **rust-pragmatic-code** | `.agents/skills/rust-pragmatic/SKILL.md` | Avoiding over-engineering, unnecessary abstraction |
| **rust-review** | `.agents/skills/rust-review/SKILL.md` | Security audit of unsafe, FFI, concurrency code |

## Skill Selection Matrix

| Task | Primary Skill | Secondary |
|------|--------------|-----------|
| Writing new code | rust-pragmatic-code | coding-guidelines |
| Code review | rust-best-practices | rust-pragmatic-code |
| Naming things | coding-guidelines | — |
| Error handling design | rust-pragmatic-code | rust-best-practices |
| Security audit | rust-review | rust-best-practices |
| Refactoring | rust-pragmatic-code | coding-guidelines |
| Performance work | rust-best-practices | — |
| Writing tests | rust-best-practices | — |

## How to Use

1. **Identify** the task type from the matrix above
2. **Read** the primary skill's `SKILL.md` file
3. **Read** secondary skills if applicable (can be done in parallel)
4. **Apply** the skill's guidelines while performing the task
5. **Reference** specific rules from the skill when explaining decisions

## Key Principle

Skills are **read once per conversation**, not per task. If you've already read
a skill in this conversation, you don't need to read it again. But always read
before first use.
