---
fncc-core: minor
---

## Control Flow & Composition Language Features

Introduces `<If>`, `<Else>`, `<ElseIf>`, `<For>`, `<Fragment>`, and `<Slot>` elements to the `.fui` language. These are parsed and translated to Rust expressions in the generated render function.

### Design Decisions

- **Built-in elements, zero runtime**: `<If>`, `<Else>`, `<ElseIf>`, `<For>`, `<Fragment>`, and `<Slot>` are recognized during parsing and translated directly to Rust expressions by the codegen. No trait objects, no vtable dispatch — conditionals become `if/else`, loops become `.iter().map()`, fragments become `div()`. The runtime cost is exactly what Rust gives you.

- **No grammar changes**: All new elements use valid `tag_name` tokens already accepted by the Pest grammar (`fncc.pest`). No parser grammar changes were needed.

- **`<Fragment>` long-form only**: Short syntax (`<>...</>`) was considered but deferred. `<Fragment>` wraps children in `div()`.

- **Slots limited to stateless components**: Stateful components with `<Slot>` produce a compile-time build error via the `parser::has_slot(&pf.ast.root)` guard in `lib.rs`. Stateless components gain a `children: impl IntoElement` parameter in their generated render function.

- **`collect_if_chain()`**: Adjacent `<If>`/`<ElseIf>`/`<Else>` siblings are grouped into a single Rust `if/else if/else` expression. Without this, consecutive siblings would each wrap in their own `div().child(...)`, breaking the logical chain.

### `.fui` Usage Examples

**If/Else chain** — condition expressions use `state.` prefix in templates (stripped to `self.` in codegen):

```fui
<If condition="{state.is_logged_in}">
  <Text>Welcome back!</Text>
</If>
<Else>
  <Button onclick="show_login">Sign in</Button>
</Else>
```

**If/ElseIf/Else**:

```fui
<If condition="{state.score > 90}">
  <Text>Grade: A</Text>
</If>
<ElseIf condition="{state.score > 80}">
  <Text>Grade: B</Text>
</ElseIf>
<Else>
  <Text>Grade: C</Text>
</Else>
```

Generated Rust (simplified):

```rust
if self.score > 90 { div().child("Grade: A") }
else if self.score > 80 { div().child("Grade: B") }
else { div().child("Grade: C") }
```

**For loop** — `each` (interpolation of iterable), `let` (loop variable name), optional `index` (enumerate variable):

```fui
<For each="{state.todos}" let="todo" index="idx">
  <Text>{idx}. {todo}</Text>
</For>
```

Generated Rust:

```rust
div().children(self.todos.iter().enumerate().map(|(idx, todo)| {
    div().child(idx.to_string()).child(". ").child(todo.to_string())
}))
```

Loop variables (`todo`, `idx`) are NOT prefixed with `self.` — they reference the closure parameter, not a struct field. Disambiguated via string replacement in generated code.

**Fragment** — groups children under a single `div()`:

```fui
<Fragment>
  <Text>First</Text>
  <Text>Second</Text>
</Fragment>
```

**Slot** — uses `children` in stateless components (no state type declared):

```fui
<Stack direction="vertical" gap="8">
  <Text size="lg">Card title</Text>
  <Slot />
</Stack>
```

### Technical infrastructure added

- `parser.rs`: `has_slot()`, `get_each_attr()`, `get_let_attr()`, `get_index_attr()`, `get_condition_attr()`; `AttrValue::as_str()` made `pub(crate)`
- `codegen.rs`: `gen_if_expr()`, `gen_for_expr()`, `gen_fragment()`, `generate_children_code()`, `collect_if_chain()`, `clean_inline()`, `replace_self_prefix()`, slot-aware `generate_stateless()` with `children: impl IntoElement` param; `import_has_slots: &[(&str, bool)]` plumbed through gen_* functions; `has_slot` derived internally from `doc.root`; slotted import call-site wiring passes children expression to render function call; string escape fix via `{:?}` at all literal emission sites
- `apps/flow-app`: new workspace member with full stateful If/Else/For/Fragment demo (count, toggle details, iterated items)

### Testing

- 10 new unit tests in `codegen::tests`: `test_if_generates_if_expression`, `test_if_else_chain`, `test_if_elseif_else_chain`, `test_for_generates_iteration`, `test_for_with_index`, `test_fragment_generates_div`, `test_slot_in_stateless_component`, `test_if_in_stack`, `test_for_in_stack`, `test_fragment_nested_in_stack`
- All 146 tests pass across the workspace
- Clippy (`-D warnings`) and formatting checks pass
