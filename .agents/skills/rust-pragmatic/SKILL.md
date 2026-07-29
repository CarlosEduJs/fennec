---
name: rust-pragmatic-code
description: >
  Guide for writing idiomatic, pragmatic Rust code that avoids the two most common
  failure modes of AI coding agents: excessive defensive programming and unnecessary
  abstraction. Use this skill when:
  (1) writing new Rust code or functions,
  (2) reviewing, refactoring, or "optimizing" existing Rust code,
  (3) deciding between borrowing vs cloning or ownership patterns,
  (4) implementing error handling with Result/Option types,
  (5) deciding whether to introduce a trait, generic, or builder,
  (6) writing tests or documentation for Rust projects.
compatibility: Rust 1.70+, Cargo
---

# Rust Pragmatic Code — Write Less, Not More

## Mindset

Rust is already a defensive language by design: the compiler guarantees memory
safety and forces you to handle error cases at the type level. This means you
**do not need to manually reinforce that defense in every function**. Good Rust
code isn't about adding more layers of protection — it's about trusting what the
compiler already guarantees and writing the minimum code that solves the real
problem, in the most direct way possible.

Golden rule before writing anything: **"Does this code solve a problem that
exists right now, or a problem I imagine might exist someday?"** Only the
former justifies the abstraction or the extra check.

You can use skill codign-guidelines and rust-best-practices in parallel, but this skill focuses on the pragmatic side of Rust coding, emphasizing simplicity, directness, and avoiding over-engineering.

---

## 1. Error handling: stop wrapping everything in `Result`

**Signs of overkill:**
- `Result<Result<T, E>, E>` or `Option<Result<T, E>>` when one of the two already
  covers the case.
- A `enum MyError` with 15 variants for a function that can only fail one way.
- Mixing `anyhow::Error` with a custom `thiserror` type in the same module for no
  reason.
- Returning `Result<()>` from functions that never actually fail (e.g. they only
  do `println!` or in-memory operations with no I/O).
- `.unwrap_or_default()` used "just to be safe" in places where a missing value
  is actually a bug that should panic or propagate, not be silently swallowed.

**Practical rules:**
- Library crate (consumed by others) → `thiserror`, typed, domain-specific errors.
- Application binary (CLI, service) → `anyhow` (or `eyre`) is enough. Don't build
  a custom error hierarchy for a `main.rs`.
- `unwrap()`/`expect()` are **not forbidden**. They're fine when the invariant is
  guaranteed by the code itself (e.g. `vec![1,2,3][0]`, a regex compiled in a
  `const`/`static`, data you just inserted in the same function). Use
  `expect("clear message explaining why this is safe")` in those cases instead of
  a silent `.unwrap()` — that documents the invariant instead of hiding it behind
  a giant `match`.
- Don't handle errors the caller has no way to act on. If the only sensible
  response to an error is "propagate it up", use `?` and move on — no need for
  `match` + log + wrap + propagate.
- Avoid `if let Err(e) = result { /* nothing relevant */ }` just to "not leave the
  error unhandled". If you're not going to do anything with it, use `?` or `.ok()`.

---

## 2. Abstraction: generics, traits, and builders only when ≥2 real cases already exist

**Signs of overkill:**
- `trait Repository<T>` with a single implementation (`SqliteRepository`) and no
  real prospect of a second one.
- Builder pattern (`.with_x().with_y().build()`) for a 3-field struct that could
  just be a struct literal or a function with named arguments.
- Generics (`fn foo<T: Trait>`) when only one concrete type is ever passed — and
  ever will be.
- `Box<dyn Trait>` for "future flexibility" when a closed `enum` already models
  the known variants perfectly.
- Separate modules (`domain/`, `application/`, `infrastructure/`, `ports/`)
  copying Clean Architecture / DDD in a 500-line binary.
- Dependency injection via trait objects for code that has — and will have — no
  tests with mocks.

**Practical rules:**
- **Rule of two implementations**: only extract a trait/generic once a second
  real (not hypothetical) implementation actually needs it. Until then, write
  concrete code.
- Prefer `enum` over `Box<dyn Trait>` whenever the set of variants is known and
  closed. `enum` is faster (no vtable), easier to `match` exhaustively, and the
  compiler warns you if you forget a case.
- Simple structs don't need a builder. `Config { host, port, timeout }` with
  `Config { host: "x".into(), ..Default::default() }` covers 90% of cases.
- Don't create a new module/file for every tiny responsibility. Start with one
  file; split it up once it gets hard to navigate, not before.
- Avoid reinventing abstractions the stdlib or well-established crates already
  solve (don't write your own `Result`-like type, your own `Option`-like type,
  your own error runtime).

---

## 3. Ownership and clones: stop cloning "just so the borrow checker stops complaining"

**Signs of overkill:**
- `.clone()` scattered around just to make the borrow checker stop complaining,
  without understanding why it complained in the first place.
- `Arc<Mutex<T>>` in single-threaded code.
- `Rc<RefCell<T>>` when a simple `&mut` would work with minimal restructuring of
  the flow.
- Passing `String`/`Vec<T>` by value in every function when `&str`/`&[T]` would
  do.

**Practical rules:**
- Before `.clone()`, ask: "can I pass a reference here instead?" Only clone when
  the data genuinely needs to live in two places at once.
- `Arc`/`Mutex`/`RwLock` only belong where there's real concurrency (threads,
  async with `Send`). In single-threaded code, `Rc`/`RefCell` (or nothing at
  all) is enough.
- Prefer `&str` over `String` and `&[T]` over `Vec<T>` in function signatures,
  unless the function actually needs to take ownership of the data.

---

## 4. Checklist before calling the code "optimized"

Run through this list before considering the code done:

1. Does every abstraction (trait, generic, error enum) have at least 2 real
   uses today — not hypothetical ones?
2. Does every `Result`/`Option` in a signature represent a failure that can
   actually happen and that the caller can act on?
3. Did I remove `.clone()` calls that exist only to silence the borrow checker,
   without understanding the root cause?
4. Does `cargo clippy --all-targets --all-features -- -D warnings` pass clean?
   (clippy catches unnecessary abstraction, redundant `.clone()`, etc.)
5. Can someone reading this function for the first time follow the flow
   without jumping across 4 files/traits?
6. If I deleted this layer/trait/wrapper, would the program lose any real
   functionality today?

If the answer to (6) is "no", delete it.

---

## 5. Quick example (before → after)

**Before (defensive + over-abstracted):**
```rust
trait UserValidator {
    fn validate(&self, user: &User) -> Result<(), ValidationError>;
}

struct DefaultUserValidator;
impl UserValidator for DefaultUserValidator {
    fn validate(&self, user: &User) -> Result<(), ValidationError> {
        if user.name.is_empty() {
            return Err(ValidationError::EmptyName);
        }
        Ok(())
    }
}

fn create_user(name: String, validator: &dyn UserValidator) -> Result<Result<User, ValidationError>, CreateUserError> {
    let user = User { name: name.clone() };
    match validator.validate(&user) {
        Ok(_) => Ok(Ok(user)),
        Err(e) => Ok(Err(e)),
    }
}
```

**After (direct):**
```rust
fn create_user(name: String) -> Result<User, ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::EmptyName);
    }
    Ok(User { name })
}
```

No trait for a single validation rule, no `Result<Result<>>`, no unnecessary
clone. If a second real validation strategy ever shows up, extracting the
trait becomes worth it.

---

## When defense/abstraction IS justified

To avoid swinging to the opposite extreme: keep handling errors from I/O,
network calls, and parsing of external input — those ARE real failure modes.
Keep using traits when the crate is a public library with multiple consumers,
or when 2+ concrete implementations already exist for real. The goal isn't
"less code always" — it's code proportional to the actual problem, no more
and no less.