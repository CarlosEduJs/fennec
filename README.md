# Fennec 🦊

> Like Astro → GPUI Native

Fennec is a compiler/framework that translates a declarative syntax inspired by Astro into native Rust applications, using [GPUI](https://www.gpui.rs/) — the rendering engine behind Zed — as the runtime.

Write UI with `.fui` files. Compile to a native binary. No Electron, no WebView, GPU-accelerated, low memory.

> **Status:** POC complete. The basic pipeline works end-to-end. Not yet production-ready.

---

## Getting started (The final structure of an application using Fennec may change; this is only for testing the PoC.)

```toml
# Cargo.toml
[dependencies]
fennec = "0.1"

[build-dependencies]
fennec-core = "0.1"
```

```rust
// build.rs
fn main() {
    println!("cargo:rerun-if-changed=src/ui");
    let ui_dir = std::path::Path::new("src/ui");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_file = std::path::Path::new(&out_dir).join("generated.rs");
    fennec_core::generate_all(ui_dir, &out_file);
}
```

```fui
---
@state CounterState
---

<Stack direction="vertical" gap="12">
    <Text size="xl">Count: {state.count}</Text>
    <Button onclick="handle_click">+1</Button>
</Stack>
```

```rust
// main.rs
use fennec::*;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[derive(Default)]
struct CounterState {
    count: i32,
}

#[fennec::command]
fn handle_click(state: &mut CounterState, cx: &mut Context<CounterState>) {
    state.count += 1;
    cx.notify();
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_, cx| {
            cx.new(|_| CounterState::default())
        }).unwrap();
    });
}
```

## Architecture

```
┌─────────────┐     build.rs     ┌──────────────┐    include!()    ┌──────────┐
│  App.fui    │ ───────────────→ │  fennec-core │ ──────────────→ │  main.rs │
│  (declar.)  │   parse + codegen│  (crates/    │   generated.rs   │  (Rust)  │
└─────────────┘                  └──────────────┘                  └──────────┘
                                                                        │
                                                                   fennec  │
                                                                  runtime  │
                                                                 (GPUI)    │
                                                                        ▼
                                                                 ┌──────────┐
                                                                 │  Native   │
                                                                 │  Binary   │
                                                                 └──────────┘
```

## Crates

| Crate | Purpose |
|---|---|
| `fennec-core` | Parser (`pest`) + codegen (AST → Rust+GPUI) |
| `fennec-macros` | `#[fennec::command]` proc-macro (3 levels) |
| `fennec-runtime` | Re-exports `gpui` + convenience wrappers |
| `fennec` | Unified crate — `use fennec::*` + `#[fennec::command]` |

## Command levels

| Level | Signature | Use case |
|---|---|---|
| 1 | `fn()` | Fire-and-forget, no event data |
| 2 | `fn(&ClickEvent)` | Access GPUI click position, modifiers |
| 3 | `fn(&mut State, &mut Context<State>)` | Mutate state + trigger re-render |

The macro generates a **trampoline** (`__fennec_cmd_{name}`) that adapts the user's signature to what the codegen expects. This lets the build-script codegen call commands without knowing their signatures at generation time.

## Design principles

- **No hidden magic** — the compiler passes through real Rust wherever possible. Rust Analyzer works natively in the frontmatter.
- **Explicit reactivity** — you call `cx.notify()` when state changes. No implicit "variable changed → UI updates" magic.
- **Explicit state** — state is a normal Rust struct. No global state, no hidden allocation.
- **Rust-style imports** — `use crate::...` in frontmatter, not JS/Astro `import` paths.
- **build.rs + include!()** — the compiler runs as a build script, not a proc-macro. Generated code is a real `.rs` file you can inspect.

## Roadmap

### Done ✅
- [x] Parser (`pest` grammar — Stack, Text, Button, attributes, interpolation)
- [x] Codegen (AST → GPUI Rust code)
- [x] `build.rs` + `include!()` pipeline
- [x] `#[fennec::command]` Level 1, 2, 3
- [x] Explicit state with `cx.notify()` counter example
- [x] Command trampoline system

### Next
- [ ] `fncc` CLI (`fncc build`, `fncc dev`)
- [ ] Multi-file component imports
- [ ] More components (Input, Image, List, Scroll)
- [ ] Hot reload
- [ ] State type inference (remove `@state` directive)

## Related projects

| Project | Relation |
|---|---|
| [Dioxus](https://dioxuslabs.com/) | JSX-like in Rust, targets native/web/mobile |
| [Slint](https://slint.dev/) | Own DSL, compiles to native with GPU rendering |
| [Tauri](https://tauri.app/) | Inspired the `#[command]` model |
| [Zed](https://zed.dev/) / GPUI | The rendering engine Fennec targets |

## License

Apache 2.0
