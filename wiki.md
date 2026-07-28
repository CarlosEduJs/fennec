# Fennec

> Style Astro to GPUI Native
> Fennec is a compiler and framework. It translates a declarative syntax, based on style Astro, into native Rust applications. Fennec uses GPUI as the rendering engine.

---

## 1. Overview

Fennec lets you write user interfaces with a syntax that is familiar to frontend developers. This syntax includes declarative components, a clear separation between UI and logic, and hot reload. Fennec compiles this syntax into a native binary. The binary does not use Electron or a WebView. The binary renders through the GPU. The binary uses low memory. Fennec uses [GPUI](https://www.gpui.rs/) for this rendering. GPUI is the framework that Zed uses.

### Developer experience goals
- The syntax must use declarative components, in a `.fui`-like format.
- The tool must support hot reload during development.
- The `.fui` files must contain only UI code. The `.rs` files must contain the logic.
- The tool must give component autocomplete, real-time error messages, and safe refactoring.

### Runtime goals
- Fennec must compile to a native binary.
- The binary must render through the GPU (GPUI).
- The binary must use low memory.
- The binary must not use Electron or a WebView.

### Guiding principle
**The compiler must not hide logic from the developer.** Each design decision in this document follows this rule: keep Rust Analyzer fully functional, keep reactivity explicit, keep refactoring safe, and keep the compiler's "magic translation" to a minimum. Where possible, the compiler must pass real Rust code through unchanged.

---

## 2. Example

**Input (`.fui`-like format):**
```fui
<App title="My application">
  <Stack direction="vertical" gap="12">
    <Text size="xl">
      Hello world
    </Text>

    <Button onclick={handleClick}>
      Click here
    </Button>
  </Stack>
</App>
```

**Output (Rust and GPUI):**
```rust
App::new()
    .child(
        stack()
            .gap(12)
            .child(text("Hello world"))
            .child(button("Click here"))
    )
```

---

## 3. Project structure

```
my-app/
├── src/
│   ├── ui/
│   │   ├── App.fui
│   │   └── components/
│   │       └── Button.fui
│   │
│   ├── lib.rs        ← Rust logic
│   └── main.rs
│
├── fncc.config.toml
├── Cargo.toml
└── package.json (?)
```

---

## 4. Design decisions

### 4.1 The frontmatter is real Rust code

The block between the `---` markers in the `.fui` file is not a separate language. This block is real Rust code. The compiler copies this code directly into the generated `.rs` file. This method has three results:

- The compiler does not bind any code at runtime. The `rustc` compiler resolves all code at compile time.
- The compiler performs type-checking at no extra cost.
- Rust Analyzer works natively in the frontmatter.

### 4.2 Commands use a macro (`#[fennec::command]`)

This design comes from the Tauri model (`#[tauri::command]`). A function that handles a UI event must carry this explicit marker:

```rust
#[fennec::command]
pub fn handle_click() {
    println!("clicked!");
}
```

The `fennec` compiler registers each marked function in a command table at compile time. The markup calls the command by name:

```fui
<Button onclick="handle_click">
  Click here
</Button>
```

**Decision about Rust Analyzer and strings:** Rust Analyzer does not track the string `"handle_click"` inside the `.fui` file. Rust Analyzer cannot give autocomplete or automatic rename for this string. **This is an accepted limit.** The developer must enter the correct command name. The compiler validates the name at build time. If the command name does not exist in the table, the compiler must stop the build and show a clear error. The Fennec team will not build a custom LSP for this issue during the POC phase.

### 4.3 Command arguments

The design defines three levels of complexity, from the simplest to the most complex.

**Level 1 — No argument:**
```rust
#[fennec::command]
pub fn handle_click() {
    println!("clicked!");
}
```

**Level 2 — Native GPUI event:**
```rust
#[fennec::command]
pub fn handle_click(event: ClickEvent) {
    println!("clicked at {:?}", event.position);
}
```
The compiler reads the function signature through the macro. The compiler then passes the real GPUI event through, with no change.

**Level 3 — Explicit access to state:**
```rust
#[fennec::command]
pub fn handle_click(state: &mut CounterState, cx: &mut Context<CounterState>) {
    state.count += 1;
    cx.notify(); // The developer decides when to call notify. The compiler adds no hidden logic.
}
```

### 4.4 Reactivity: explicit, not implicit

**Decision:** Fennec uses explicit reactivity. The developer must call `cx.notify()` by hand. Fennec does not use implicit reactivity, where a variable change updates the UI without a direct call.

**Reason:** debugging. Implicit reactivity works well on the first day of a project. In a larger application, implicit reactivity becomes hard to debug. For example, a developer cannot easily find the reason for a component re-render. This decision matches the target user of Fennec: a developer who accepts less ease of use in exchange for predictable, native behavior.

**Positive result:** Fennec does not need its own reactivity system. The GPUI `Model`/`View` model, together with `cx.notify()`, already gives this exact behavior. The compiler's task is only to generate the correct call in the correct place. The compiler does not create a separate reactivity layer.

The compiler resolves markup interpolation, such as `{state.count}`, at **compile time**. The compiler generates code similar to `.child(text(format!("{}", state.count)))`. GPUI re-renders the full tree when the code calls `cx.notify()`.

### 4.5 Explicit state, not implicit state

State lives in normal Rust structs. The developer declares these structs. The developer passes these structs directly to each component. Fennec has no "magic" global state.

```rust
pub struct CounterState {
    pub count: i32,
}
```

```fui
---
use crate::lib::CounterState;
---

<Counter state={CounterState}>
  <Text>{state.count}</Text>
  <Button onclick="handle_click">+1</Button>
</Counter>
```

### 4.6 Imports: Rust style (`use`), not ES Modules style

**Decision:** The frontmatter must use real Rust `use crate::...` statements. The frontmatter must not use JavaScript- or Astro-style statements, such as `import { X } from "../lib"`.

```fui
---
use crate::lib::{CounterState, handle_click};
---
```

**Reason:** this method keeps the rule "the frontmatter is real Rust code." The compiler does not need to resolve relative paths. The compiler has no ambiguity about re-exports. Rust Analyzer reads this line natively. The compiler copies this line into the generated `.rs` file with almost no change.

**Open question:** the team has not yet decided if imports of `.fui` components, such as `Button` or `Stack`, must also use the `use` statement. An alternative is to keep file-path imports in the Astro style, such as `import Button from "./components/Button.fui"`. This is the next design question for the team.

### 4.7 Configuration: `fncc.config.toml` together with `Cargo.toml`

**Decision:** The `fncc.config.toml` file manages only UI and developer-experience metadata. The `Cargo.toml` file continues to manage normal Rust data, such as dependencies. The developer manages this file directly. This split keeps a clear separation of duties. The compiler does not generate one file from the other.

```toml
[package]
name = "my-app"
version = "0.1.0"

[paths]
ui = "src/ui"            # location of the .fui files
lib = "src"              # location of the Rust logic
output = "target/fennec"  # location of the generated intermediate .rs files

[app]
entry = "src/ui/App.fui"

[window]
title = "My application"
width = 800
height = 600
resizable = true

[dev]
hot_reload = true
watch = ["src/ui", "src"]
```

---

## 5. Related projects

- **Dioxus** — Dioxus uses JSX-like Rust macros. Dioxus targets native, web, and mobile platforms. Dioxus is the closest competing project.
- **Slint** — Slint uses its own declarative DSL. Slint compiles to native code with GPU rendering.
- **Tauri** — the Tauri `#[tauri::command]` and `invoke()` model inspired the Fennec command system. This model also has the same weak point: the bridge between UI and logic uses a string, with no type-check.
- **Zed / GPUI** — Zed uses GPUI directly, with pure Rust code and no DSL. Fennec adds the first layer of declarative syntax on top of GPUI.

---

## 6. Suggested roadmap (Phase 0 — POC)

1. Define the minimum syntax subset. This subset must include four or five components: Stack, Text, Button, and App.
2. Translate one `.fui` example into Rust and GPUI code by hand, with no parser. This step checks if the GPUI API can support the required output.
3. Write the parser. Candidate tool: `pest`. This tool is good for fast grammar prototyping.
4. Automate the parser-to-codegen step.
5. Validate the command system (`#[fennec::command]`) with Level 1 and Level 2 arguments.
6. Validate explicit state (Level 3) with a simple counter example.

### Known problems, not yet solved
- **Hot reload for compiled Rust code:** candidate solutions are `hot-lib-reloader`, which reloads a dynamic library during development, or a fast recompile-and-restart method with incremental compilation.
- **Imports for `.fui` components:** the team has not yet decided if these imports must use the `use` statement (Rust style) or a file path (Astro style).