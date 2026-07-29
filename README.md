# fncc 🦊

> Like Astro → GPUI Native

fncc is a compiler/framework that translates a declarative syntax inspired by Astro into native Rust applications, using [GPUI](https://www.gpui.rs/) — the rendering engine behind Zed — as the runtime.

Write UI with `.fui` files. Compile to a native binary. No Electron, no WebView, GPU-accelerated, low memory.

> **Status:** POC complete — parser, codegen, commands, state, and multi-file component imports all work end-to-end.

---

## Getting started (The final structure of an application using fncc may change; this is only for testing the PoC.)

```toml
# Cargo.toml
[dependencies]
fncc = "0.1"

[build-dependencies]
fncc-core = "0.1"
```

```rust
// build.rs
fn main() {
    println!("cargo:rerun-if-changed=src/ui");
    let ui_dir = std::path::Path::new("src/ui");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_file = std::path::Path::new(&out_dir).join("generated.rs");
    fncc_core::generate_all(ui_dir, &out_file);
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
use fncc::*;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[derive(Default)]
struct CounterState {
    count: i32,
}

#[fncc::command]
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

## Component imports

Organize your UI across multiple `.fui` files. Use Rust-style `use` paths in the frontmatter to import components from other files.

```
src/ui/
├── App.fui              # imports components/Header.fui, components/Footer.fui
└── components/
    ├── Header.fui
    └── Footer.fui
```

**`src/ui/components/Header.fui`** — a stateless component:
```fui
<Text size="xl">Welcome</Text>
```

**`src/ui/App.fui`** — imports and uses it:
```fui
---
@state AppState
use ui::components::Header;
use ui::components::Footer;
---

<Stack direction="vertical" gap="16">
    <Header />
    <Text>Count: {state.count}</Text>
    <Button onclick="handle_click">+1</Button>
    <Footer />
</Stack>
```

**Rules:**
- The `ui::` prefix signals a `.fui` component import: `use ui::path::Name;` → `<ui_dir>/path/Name.fui`
- Grouped imports work: `use ui::components::{Button, Card};`
- Imported components must be stateless (no `@state` directive)
- Render function names derive from the file stem, not the root element — `Header.fui` always produces `render_header()`
- `use gpui::TextInput;` keeps the real Rust import in emitted code for use in handlers and state types
- Regular Rust `use crate::...` statements pass through unchanged

## Architecture

```
┌─────────────┐     build.rs     ┌──────────────┐    include!()    ┌──────────┐
│  App.fui    │ ───────────────→ │  fncc-core   │ ──────────────→ │  main.rs │
│  (declar.)  │   parse + codegen│  (crates/    │   generated.rs   │  (Rust)  │
└─────────────┘                  └──────────────┘                  └──────────┘
                                                                        │
                                                                    fncc    │
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
| `fncc-core` | Parser (`pest`) + codegen (AST → Rust+GPUI) |
| `fncc-macros` | `#[fncc::command]` proc-macro (3 levels) |
| `fncc-runtime` | Re-exports `gpui` + convenience wrappers |
| `fncc` | Unified crate — `use fncc::*` + `#[fncc::command]` |

## Command levels

| Level | Signature | Use case |
|---|---|---|
| 1 | `fn()` | Fire-and-forget, no event data |
| 2 | `fn(&ClickEvent)` | Access GPUI click position, modifiers |
| 3 | `fn(&mut State, &mut Context<State>)` | Mutate state + trigger re-render |

The macro generates a **trampoline** (`__fncc_cmd_{name}`) that adapts the user's signature to what the codegen expects. This lets the build-script codegen call commands without knowing their signatures at generation time.

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
- [x] `#[fncc::command]` Level 1, 2, 3
- [x] Explicit state with `cx.notify()` counter example
- [x] Command trampoline system
- [x] Multi-file component imports (`use ui::path::Name;` syntax, recursive scan, import resolution)

### Next
- [ ] `fncc` CLI (`fncc build`, `fncc dev`)
- [ ] More components (Input, Image, List, Scroll)
- [ ] Hot reload
- [x] Semantic analysis — foundation (`semantic::SemanticDb`, `GenerateOptions`)
  - [x] Command extraction from `.rs` files via `syn`
  - [x] Automatic state inference (remove `@state`)
  - [x] Hard-error command resolution (`onclick` → `#[fncc::command]` validation)
  - [ ] Typed component props
  - [ ] LSP metadata generation

## Related projects

| Project | Relation |
|---|---|
| [Dioxus](https://dioxuslabs.com/) | JSX-like in Rust, targets native/web/mobile |
| [Slint](https://slint.dev/) | Own DSL, compiles to native with GPU rendering |
| [Tauri](https://tauri.app/) | Inspired the `#[command]` model |
| [Zed](https://zed.dev/) / GPUI | The rendering engine fncc targets |

## License

Apache 2.0
