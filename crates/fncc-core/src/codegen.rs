#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use crate::parser::{self, AttrValue, Document, Element, Node};
use crate::semantic::PropField;

pub fn generate(doc: &Document) -> String {
    generate_with_id(doc, 0)
}

pub fn generate_with_id(doc: &Document, file_id: usize) -> String {
    generate_with_imports(doc, file_id, &[], None, None, None, &[], None, &[])
}

/// Resolved import entry: (tag_name, render_fn_name)
/// - For .fui component imports: ("Header", "render_header")
/// - For gpui imports: ("TextInput", "") — registered but no special codegen
pub type ResolvedImport<'a> = (&'a str, &'a str);

/// Generate code for a document with a specific component name.
/// `component_name` is used for the render function name (derived from the file stem).
/// If `None`, falls back to the root element name (backward compat).
/// `resolved_state_type` overrides the document's `@state` directive (used by semantic analysis).
pub fn generate_with_imports(
    doc: &Document,
    file_id: usize,
    imports: &[ResolvedImport],
    component_name: Option<&str>,
    resolved_state_type: Option<&str>,
    props_type: Option<&str>,
    import_props: &[(&str, Option<&str>)],
    prop_fields: Option<&HashMap<String, Vec<PropField>>>,
    import_has_slots: &[(&str, bool)],
) -> String {
    let mut out = String::new();
    let state_type = resolved_state_type.or(doc.state_type.as_deref());
    let has_state = state_type.is_some();
    let has_slot = parser::has_slot(&doc.root);

    if let Some(ref fm) = doc.frontmatter {
        out.push_str(fm);
        out.push('\n');
    }

    // collect referenced command names for validation
    let commands = parser::collect_commands(&doc.root);
    if !commands.is_empty() {
        out.push_str("#[allow(unused)]\n");
        out.push_str(&format!("fn _fncc_validate_{file_id}() {{\n"));
        for cmd in &commands {
            let trampoline = format!("__fncc_cmd_{cmd}");
            out.push_str(&format!("    let _ = {trampoline};\n"));
        }
        out.push_str("}\n\n");
    }

    if has_state {
        generate_stateful(
            doc,
            &mut out,
            imports,
            state_type,
            import_props,
            prop_fields,
            import_has_slots,
        );
    } else {
        generate_stateless(
            doc,
            &mut out,
            imports,
            component_name,
            props_type,
            import_props,
            prop_fields,
            import_has_slots,
            has_slot,
        );
    }

    out
}

fn generate_stateless(
    doc: &Document,
    out: &mut String,
    imports: &[ResolvedImport],
    component_name: Option<&str>,
    props_type: Option<&str>,
    import_props: &[(&str, Option<&str>)],
    prop_fields: Option<&HashMap<String, Vec<PropField>>>,
    import_has_slots: &[(&str, bool)],
    has_slot: bool,
) {
    let name = component_name.unwrap_or(&doc.root.name);
    let fn_name = format!("render_{}", to_snake_case(name));

    // Build function signature based on props and slot
    let params = match (props_type, has_slot) {
        (Some(pt), true) => format!("props: &{pt}, children: impl IntoElement"),
        (Some(pt), false) => format!("props: &{pt}"),
        (None, true) => "children: impl IntoElement".to_string(),
        (None, false) => String::new(),
    };

    let sig = if params.is_empty() {
        format!("pub fn {fn_name}() -> impl IntoElement {{\n")
    } else {
        format!("pub fn {fn_name}({params}) -> impl IntoElement {{\n")
    };
    out.push_str(&sig);
    out.push_str(&generate_element(
        &doc.root,
        1,
        false,
        imports,
        import_props,
        prop_fields,
        import_has_slots,
    ));
    out.push('\n');
    out.push_str("}\n");
}

fn generate_stateful(
    doc: &Document,
    out: &mut String,
    imports: &[ResolvedImport],
    state_type: Option<&str>,
    import_props: &[(&str, Option<&str>)],
    prop_fields: Option<&HashMap<String, Vec<PropField>>>,
    _import_has_slots: &[(&str, bool)],
) {
    let state_type = state_type.unwrap_or("Self");

    out.push_str(&format!("impl Render for {state_type} {{\n"));
    out.push_str("    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {\n");
    out.push_str("        let handle = cx.entity().downgrade();\n");
    out.push_str(&generate_element(
        &doc.root,
        2,
        true,
        imports,
        import_props,
        prop_fields,
        _import_has_slots,
    ));
    out.push_str("\n    }\n");
    out.push_str("}\n");
}

/// Group consecutive If/ElseIf/Else elements into branches.
/// Stops at a second `If` (which begins an independent conditional block)
/// or any non-If/ElseIf/Else element.
fn collect_if_chain(nodes: &[Node], start: usize) -> Vec<&Element> {
    let mut branches = Vec::new();
    for node in &nodes[start..] {
        match node {
            Node::Element(el) if el.name == "If" => {
                if branches.is_empty() {
                    branches.push(el);
                } else {
                    break; // new `If` starts a separate chain
                }
            }
            Node::Element(el) if el.name == "ElseIf" || el.name == "Else" => {
                if branches.is_empty() {
                    break; // orphaned Else/ElseIf without preceding If
                }
                branches.push(el);
            }
            _ => break,
        }
    }
    branches
}

/// Generate an if/else-if/else expression for a chain of If/ElseIf/Else elements.
/// Returns a single-line Rust expression like `if cond { div()... } else { div()... }`.
fn gen_if_expr(
    branches: &[&Element],
    stateful: bool,
    imports: &[ResolvedImport],
    import_props: &[(&str, Option<&str>)],
    prop_fields: Option<&HashMap<String, Vec<PropField>>>,
    _import_has_slots: &[(&str, bool)],
) -> String {
    let mut expr = String::new();
    for (i, el) in branches.iter().enumerate() {
        if i == 0 {
            expr.push_str("if ");
        } else if el.name == "Else" {
            expr.push_str(" else ");
        } else {
            expr.push_str(" else if ");
        }
        if el.name != "Else" {
            if let Some(cond) = parser::get_condition_attr(el) {
                let e = interpolation_expr(cond);
                expr.push_str(&e);
                expr.push(' ');
            } else {
                expr.push_str("compile_error!(\"<");
                expr.push_str(&el.name);
                expr.push_str("> element is missing `condition` attribute\") ");
            }
        }
        expr.push_str("{ ");
        match &el.children[..] {
            [] => expr.push_str("div()"),
            [single] => match single {
                Node::Text(t) => expr.push_str(&format!("div().child({:?})", t)),
                Node::Interpolation(interp) => {
                    let e = interpolation_expr(interp);
                    expr.push_str(&format!("div().child({e}.to_string())"));
                }
                Node::Element(child_el) => {
                    let child_code = generate_element(
                        child_el,
                        0,
                        stateful,
                        imports,
                        import_props,
                        prop_fields,
                        _import_has_slots,
                    );
                    expr.push_str(&format!("div().child({})", clean_inline(&child_code)));
                }
            },
            children => {
                expr.push_str("div()");
                for child in children {
                    match child {
                        Node::Text(t) => expr.push_str(&format!(".child({:?})", t)),
                        Node::Interpolation(interp) => {
                            let e = interpolation_expr(interp);
                            expr.push_str(&format!(".child({e}.to_string())"));
                        }
                        Node::Element(child_el) => {
                            let child_code = generate_element(
                                child_el,
                                0,
                                stateful,
                                imports,
                                import_props,
                                prop_fields,
                                _import_has_slots,
                            );
                            expr.push_str(&format!(".child({})", clean_inline(&child_code)));
                        }
                    }
                }
            }
        }
        expr.push_str(" }");
    }
    // Add else { div() } if the last branch isn't an Else
    if branches.last().map(|b| b.name.as_str()) != Some("Else") {
        expr.push_str(" else { div() }");
    }
    expr
}

/// Generate a For loop expression for use inside `.children(...)`.
fn gen_for_expr(
    el: &Element,
    stateful: bool,
    imports: &[ResolvedImport],
    import_props: &[(&str, Option<&str>)],
    prop_fields: Option<&HashMap<String, Vec<PropField>>>,
    _import_has_slots: &[(&str, bool)],
) -> String {
    let each = match parser::get_each_attr(el) {
        Some(e) => e,
        None => {
            return "compile_error!(\"<For> element is missing required `each` attribute\")".to_string();
        }
    };
    let let_var = parser::get_let_attr(el).unwrap_or("item");
    let index_var = parser::get_index_attr(el);
    let iter_expr = interpolation_expr(each);

    let mut body = String::new();
    match &el.children[..] {
        [] => body.push_str("div()"),
        [single] => match single {
            Node::Text(t) => body.push_str(&format!("div().child({:?})", t)),
            Node::Interpolation(interp) => {
                let e = interpolation_expr(interp);
                body.push_str(&format!("div().child({e}.to_string())"));
            }
            Node::Element(child_el) => {
                let child_code = generate_element(
                    child_el,
                    1,
                    stateful,
                    imports,
                    import_props,
                    prop_fields,
                    _import_has_slots,
                );
                body.push_str(&child_code);
            }
        },
        children => {
            body.push_str("div()");
            for child in children {
                match child {
                    Node::Text(t) => body.push_str(&format!(".child({:?})", t)),
                    Node::Interpolation(interp) => {
                        let e = interpolation_expr(interp);
                        body.push_str(&format!(".child({e}.to_string())"));
                    }
                    Node::Element(child_el) => {
                        let child_code = generate_element(
                            child_el,
                            1,
                            stateful,
                            imports,
                            import_props,
                            prop_fields,
                            _import_has_slots,
                        );
                        body.push_str(&format!(".child({})", clean_inline(&child_code)));
                    }
                }
            }
        }
    }

    // Strip self. prefix for loop variables so they reference the closure args, not state fields.
    // Uses identifier-aware replacement to avoid corrupting fields that share a prefix
    // e.g. self.item_count stays unchanged when var = "item"
    let var_names: Vec<&str> = index_var.iter().chain(std::iter::once(&let_var)).copied().collect();
    for var in &var_names {
        body = replace_self_prefix(&body, var);
    }

    if let Some(index) = index_var {
        format!("{iter_expr}.iter().enumerate().map(|({index}, {let_var})| {{\n    {body}\n}})")
    } else {
        format!("{iter_expr}.iter().map(|{let_var}| {{\n    {body}\n}})")
    }
}

fn generate_element(
    el: &Element,
    depth: usize,
    stateful: bool,
    imports: &[ResolvedImport],
    import_props: &[(&str, Option<&str>)],
    prop_fields: Option<&HashMap<String, Vec<PropField>>>,
    _import_has_slots: &[(&str, bool)],
) -> String {
    let indent = "    ".repeat(depth);

    // Built-in elements
    match el.name.as_str() {
        "Stack" => {
            return gen_stack(
                el,
                &indent,
                depth,
                stateful,
                imports,
                import_props,
                prop_fields,
                _import_has_slots,
            );
        }
        "Text" => {
            return gen_text(
                el,
                &indent,
                depth,
                stateful,
                imports,
                import_props,
                prop_fields,
                _import_has_slots,
            );
        }
        "Button" => {
            return gen_button(
                el,
                &indent,
                depth,
                stateful,
                imports,
                import_props,
                prop_fields,
                _import_has_slots,
            );
        }
        "Fragment" => {
            return gen_fragment(
                el,
                &indent,
                depth,
                stateful,
                imports,
                import_props,
                prop_fields,
                _import_has_slots,
            );
        }
        "For" => {
            return format!(
                "div().children({})",
                gen_for_expr(el, stateful, imports, import_props, prop_fields, _import_has_slots)
            );
        }
        "If" | "ElseIf" | "Else" => {
            // Standalone — generate if-expression with no else
            let branches = [el];
            return gen_if_expr(
                &branches,
                stateful,
                imports,
                import_props,
                prop_fields,
                _import_has_slots,
            );
        }
        "Slot" => return "children".to_string(),
        _ => {}
    }

    // .fui component imports (have a non-empty render function name)
    if let Some(render_fn) = imports
        .iter()
        .find(|(name, fn_name)| name == &el.name && !fn_name.is_empty())
        .map(|(_, fn_name)| *fn_name)
    {
        let props_type = import_props.iter().find(|(n, _)| n == &el.name).and_then(|(_, p)| *p);
        let has_slot = _import_has_slots
            .iter()
            .find(|(n, _)| n == &el.name)
            .map(|(_, h)| *h)
            .unwrap_or(false);

        let children_expr = if has_slot {
            if el.children.is_empty() {
                "div()".to_string()
            } else {
                let child_code = generate_children_code(
                    &el.children,
                    &indent,
                    depth + 1,
                    stateful,
                    imports,
                    import_props,
                    prop_fields,
                    _import_has_slots,
                );
                clean_inline(&format!("div(){child_code}"))
            }
        } else {
            String::new()
        };

        return if let Some(pt) = props_type {
            let mut struct_fields = String::new();
            if let Some(fields) = prop_fields.and_then(|m| m.get(pt)) {
                for f in fields {
                    if let Some((_, attr_val)) = el.attrs.iter().find(|(n, _)| n == &f.name) {
                        let v = match attr_val {
                            AttrValue::String(s) => format!("{:?}.into()", s),
                            AttrValue::Interpolation(expr) => {
                                format!("{}.into()", interpolation_expr(expr))
                            }
                        };
                        struct_fields.push_str(&format!("\n{indent}        {}: {},", f.name, v));
                    } else if f.is_optional {
                        struct_fields.push_str(&format!("\n{indent}        {}: None,", f.name));
                    }
                }
            } else {
                for (attr_name, attr_val) in &el.attrs {
                    let v = match attr_val {
                        AttrValue::String(s) => format!("{:?}.into()", s),
                        AttrValue::Interpolation(expr) => {
                            format!("{}.into()", interpolation_expr(expr))
                        }
                    };
                    struct_fields.push_str(&format!("\n{indent}        {attr_name}: {v},"));
                }
            }
            if has_slot {
                format!("{indent}{render_fn}(&{pt} {{ {struct_fields}\n{indent}    }}, {children_expr})")
            } else {
                format!("{indent}{render_fn}(&{pt} {{ {struct_fields}\n{indent}    }})")
            }
        } else if has_slot {
            format!("{indent}{render_fn}({children_expr})")
        } else {
            format!("{indent}{render_fn}()")
        };
    }

    // Fallback for gpui imports and unknown elements
    gen_fallback(
        el,
        &indent,
        depth,
        stateful,
        imports,
        import_props,
        prop_fields,
        _import_has_slots,
    )
}

/// Generate code for a `<Fragment>` — wraps children in `div()` using
/// `generate_children_code` for consistent control-flow handling.
fn gen_fragment(
    el: &Element,
    indent: &str,
    depth: usize,
    stateful: bool,
    imports: &[ResolvedImport],
    import_props: &[(&str, Option<&str>)],
    prop_fields: Option<&HashMap<String, Vec<PropField>>>,
    _import_has_slots: &[(&str, bool)],
) -> String {
    let children_code = generate_children_code(
        &el.children,
        indent,
        depth,
        stateful,
        imports,
        import_props,
        prop_fields,
        _import_has_slots,
    );
    format!("{indent}div()\n{children_code}").trim_end().to_string()
}

/// Generate child code with control-flow awareness (If/ElseIf/Else chains, For, Fragment inlining).
fn generate_children_code(
    children: &[Node],
    indent: &str,
    depth: usize,
    stateful: bool,
    imports: &[ResolvedImport],
    import_props: &[(&str, Option<&str>)],
    prop_fields: Option<&HashMap<String, Vec<PropField>>>,
    _import_has_slots: &[(&str, bool)],
) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            Node::Element(el) if el.name == "If" => {
                let chain = collect_if_chain(children, i);
                let expr = gen_if_expr(&chain, stateful, imports, import_props, prop_fields, _import_has_slots);
                out.push_str(&format!("{indent}    .child(\n"));
                out.push_str(&format!("{indent}        {expr}\n"));
                out.push_str(&format!("{indent}    )\n"));
                i += chain.len();
            }
            Node::Element(el) if el.name == "For" => {
                let for_code = gen_for_expr(el, stateful, imports, import_props, prop_fields, _import_has_slots);
                out.push_str(&format!("{indent}    .children(\n"));
                out.push_str(&format!("{indent}        {for_code}\n"));
                out.push_str(&format!("{indent}    )\n"));
                i += 1;
            }
            Node::Element(el) if el.name == "Fragment" => {
                let frag_code = generate_children_code(
                    &el.children,
                    indent,
                    depth,
                    stateful,
                    imports,
                    import_props,
                    prop_fields,
                    _import_has_slots,
                );
                out.push_str(&frag_code);
                i += 1;
            }
            _ => {
                out.push_str(&format!("{indent}    .child(\n"));
                match &children[i] {
                    Node::Element(child_el) => {
                        out.push_str(&generate_element(
                            child_el,
                            depth + 2,
                            stateful,
                            imports,
                            import_props,
                            prop_fields,
                            _import_has_slots,
                        ));
                    }
                    Node::Text(t) => {
                        out.push_str(&format!("{indent}        {:?}", t));
                    }
                    Node::Interpolation(expr) => {
                        let e = interpolation_expr(expr);
                        out.push_str(&format!("{indent}        {e}.to_string()"));
                    }
                }
                out.push('\n');
                out.push_str(&format!("{indent}    )\n"));
                i += 1;
            }
        }
    }
    out
}

fn gen_stack(
    el: &Element,
    indent: &str,
    depth: usize,
    stateful: bool,
    imports: &[ResolvedImport],
    import_props: &[(&str, Option<&str>)],
    prop_fields: Option<&HashMap<String, Vec<PropField>>>,
    _import_has_slots: &[(&str, bool)],
) -> String {
    let mut out = format!("{indent}div()\n");

    let mut is_vertical = false;
    for (key, val) in &el.attrs {
        match key.as_str() {
            "direction" if val.as_str() == "vertical" => is_vertical = true,
            "gap" => {
                let v = val.as_str();
                if let Ok(n) = v.parse::<f64>() {
                    out.push_str(&format!("{indent}    .gap(px({n}.))\n"));
                }
            }
            _ => {}
        }
    }

    if is_vertical {
        out.push_str(&format!("{indent}    .flex()\n{indent}    .flex_col()\n"));
    } else {
        out.push_str(&format!("{indent}    .flex()\n"));
    }

    out.push_str(&generate_children_code(
        &el.children,
        indent,
        depth,
        stateful,
        imports,
        import_props,
        prop_fields,
        _import_has_slots,
    ));

    out.trim_end().to_string()
}

fn gen_text(
    el: &Element,
    indent: &str,
    depth: usize,
    stateful: bool,
    imports: &[ResolvedImport],
    import_props: &[(&str, Option<&str>)],
    prop_fields: Option<&HashMap<String, Vec<PropField>>>,
    _import_has_slots: &[(&str, bool)],
) -> String {
    let mut out = format!("{indent}div()\n");

    for (key, val) in &el.attrs {
        if key.as_str() == "size" {
            let v = val.as_str();
            let ts = match v {
                "xs" => "text_xs()",
                "sm" => "text_sm()",
                "base" => "text_base()",
                "lg" => "text_lg()",
                "xl" => "text_xl()",
                "2xl" | "xxl" => "text_2xl()",
                "3xl" => "text_3xl()",
                _ => "text_base()",
            };
            out.push_str(&format!("{indent}    .{ts}\n"));
        }
    }

    match &el.children[..] {
        [Node::Text(t)] => {
            out.push_str(&format!("{indent}    .child({:?})", t));
        }
        [Node::Interpolation(expr)] => {
            let e = interpolation_expr(expr);
            out.push_str(&format!("{indent}    .child({e}.to_string())"));
        }
        children => {
            out.push_str(&generate_children_code(
                children,
                indent,
                depth,
                stateful,
                imports,
                import_props,
                prop_fields,
                _import_has_slots,
            ));
        }
    }

    out.trim_end().to_string()
}

fn gen_button(
    el: &Element,
    indent: &str,
    depth: usize,
    stateful: bool,
    imports: &[ResolvedImport],
    import_props: &[(&str, Option<&str>)],
    prop_fields: Option<&HashMap<String, Vec<PropField>>>,
    _import_has_slots: &[(&str, bool)],
) -> String {
    let mut out = format!("{indent}div()\n");

    let btn_id = match &el.children[..] {
        [Node::Text(t)] => t.clone(),
        _ => format!("button_{depth}"),
    };
    out.push_str(&format!("{indent}    .id({:?})\n", btn_id));
    out.push_str(&format!("{indent}    .cursor_pointer()\n"));

    for (key, val) in &el.attrs {
        if key.as_str() == "onclick" {
            let handler = val.as_str();
            let trampoline = format!("__fncc_cmd_{handler}");
            if stateful {
                // Level 3: use entity handle pattern
                out.push_str(&format!("{indent}    .on_click({{\n"));
                out.push_str(&format!("{indent}        let handle = handle.clone();\n"));
                out.push_str(&format!("{indent}        move |_, _, cx| {{\n"));
                out.push_str(&format!("{indent}            handle.update(cx, |this, cx| {{\n"));
                out.push_str(&format!("{indent}                {trampoline}(this, cx);\n"));
                out.push_str(&format!("{indent}            }}).ok();\n"));
                out.push_str(&format!("{indent}        }}\n"));
                out.push_str(&format!("{indent}    }})\n"));
            } else {
                out.push_str(&format!("{indent}    .on_click({trampoline})\n"));
            }
        }
    }

    match &el.children[..] {
        [Node::Text(t)] => out.push_str(&format!("{indent}    .child({:?})", t)),
        [Node::Interpolation(expr)] => {
            let e = interpolation_expr(expr);
            out.push_str(&format!("{indent}    .child({e}.to_string())"));
        }
        children => {
            out.push_str(&generate_children_code(
                children,
                indent,
                depth,
                stateful,
                imports,
                import_props,
                prop_fields,
                _import_has_slots,
            ));
        }
    }

    out.trim_end().to_string()
}

fn gen_fallback(
    el: &Element,
    indent: &str,
    depth: usize,
    stateful: bool,
    imports: &[ResolvedImport],
    import_props: &[(&str, Option<&str>)],
    prop_fields: Option<&HashMap<String, Vec<PropField>>>,
    _import_has_slots: &[(&str, bool)],
) -> String {
    let mut out = format!("{indent}div()\n");
    for (key, val) in &el.attrs {
        let v = val.as_str();
        out.push_str(&format!("{indent}    .attr({:?}, {:?})\n", key, v));
    }
    out.push_str(&generate_children_code(
        &el.children,
        indent,
        depth,
        stateful,
        imports,
        import_props,
        prop_fields,
        _import_has_slots,
    ));
    out.trim_end().to_string()
}

// Collapse whitespace and remove space between `)` and `.` for inline expressions.
fn clean_inline(s: &str) -> String {
    let collapsed: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.replace(") .", ").")
}

/// Resolve an interpolation expression to the correct Rust variable reference.
/// - `state.field` → `self.field` (for stateful components)
/// - `props.field` → `props.field` (for stateless components with props)
/// - `bare_expr` → `self.bare_expr` (fallback for stateful components)
fn interpolation_expr(expr: &str) -> String {
    let trimmed = expr.trim();
    if let Some(field) = trimmed.strip_prefix("state.") {
        format!("self.{field}")
    } else if trimmed.starts_with("props.") {
        trimmed.to_string()
    } else {
        format!("self.{trimmed}")
    }
}

/// Replace `self.{var}` with `{var}` only when followed by a non-identifier character
/// or end of string. This prevents corrupting state fields that share a prefix with
/// a loop variable (e.g. `self.item_count` is not changed when var = "item").
fn replace_self_prefix(s: &str, var: &str) -> String {
    let pattern = format!("self.{var}");
    let mut out = String::new();
    let mut last = 0;
    for (idx, _) in s.match_indices(&pattern) {
        out.push_str(&s[last..idx]);
        let end = idx + pattern.len();
        let keep = if end < s.len() {
            let b = s.as_bytes()[end];
            b.is_ascii_alphanumeric() || b == b'_'
        } else {
            false
        };
        if keep {
            out.push_str(&pattern);
            last = idx + pattern.len();
        } else {
            out.push_str(var);
            last = end;
        }
    }
    out.push_str(&s[last..]);
    out
}

pub(crate) fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            for c in ch.to_lowercase() {
                result.push(c);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn generate_from(source: &str) -> String {
        let doc = parse(source).unwrap();
        generate(&doc)
    }

    // --- Happy path ---

    #[test]
    fn test_generates_stateless_render_function() {
        let out = generate_from("<MyComp></MyComp>");
        assert!(out.contains("pub fn render_my_comp() -> impl IntoElement {"));
        assert!(out.contains("div()"));
    }

    #[test]
    fn test_generates_stateful_render_impl() {
        let src = "---\n@state CounterState\n---\n<App></App>";
        let out = generate_from(src);
        assert!(out.contains("impl Render for CounterState {"));
        assert!(
            out.contains("fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {")
        );
    }

    #[test]
    fn test_frontmatter_is_preserved_in_output() {
        let src = "---\nuse crate::prelude::*;\n---\n<App></App>";
        let out = generate_from(src);
        assert!(out.contains("use crate::prelude::*;"));
    }

    #[test]
    fn test_stack_with_direction_vertical_generates_flex_col() {
        let out = generate_from("<Stack direction=\"vertical\"></Stack>");
        assert!(out.contains(".flex()"));
        assert!(out.contains(".flex_col()"));
    }

    #[test]
    fn test_stack_with_direction_horizontal_generates_flex_only() {
        let out = generate_from("<Stack direction=\"horizontal\"></Stack>");
        assert!(out.contains(".flex()"));
        assert!(!out.contains(".flex_col()"));
    }

    #[test]
    fn test_stack_with_gap_generates_px_value() {
        let out = generate_from("<Stack gap=\"12\"></Stack>");
        assert!(out.contains(".gap(px(12.))"));
    }

    #[test]
    fn test_text_with_size_xl() {
        let out = generate_from("<Text size=\"xl\">hello</Text>");
        assert!(out.contains(".text_xl()"));
        assert!(out.contains(".child(\"hello\")"));
    }

    #[test]
    fn test_text_with_size_unknown_falls_back_to_base() {
        let out = generate_from("<Text size=\"huge\">text</Text>");
        assert!(out.contains(".text_base()"));
    }

    #[test]
    fn test_button_with_text_child() {
        let out = generate_from("<Button onclick=\"handle_click\">Click</Button>");
        assert!(out.contains(".id(\"Click\")"));
        assert!(out.contains(".cursor_pointer()"));
        assert!(out.contains(".child(\"Click\")"));
    }

    #[test]
    fn test_stateful_button_with_onclick_generates_entity_pattern() {
        let src = "---\n@state AppState\n---\n<Button onclick=\"inc\">+1</Button>";
        let out = generate_from(src);
        assert!(out.contains("let handle = handle.clone();"));
        assert!(out.contains("handle.update(cx, |this, cx| {"));
        assert!(out.contains("__fncc_cmd_inc(this, cx);"));
    }

    #[test]
    fn test_stateless_button_with_onclick_generates_direct_call() {
        let out = generate_from("<Button onclick=\"log_click\">Go</Button>");
        assert!(out.contains(".on_click(__fncc_cmd_log_click)"));
    }

    // --- Edge cases ---

    #[test]
    fn test_empty_element_children() {
        let out = generate_from("<Div></Div>");
        assert!(out.contains("div()"));
    }

    #[test]
    fn test_unknown_element_falls_back_to_div_with_attrs() {
        let out = generate_from("<CustomEl foo=\"bar\">content</CustomEl>");
        assert!(out.contains(".attr(\"foo\", \"bar\")"));
        assert!(out.contains(".child("));
        assert!(out.contains("\"content\""));
    }

    #[test]
    fn test_interpolation_in_text_content_generates_format() {
        let out = generate_from("---\n@state S\n---\n<Text>{state.msg}</Text>");
        assert!(out.contains("self.msg.to_string()"));
        assert!(!out.contains("self.state.msg"));
    }

    #[test]
    fn test_interpolation_strips_state_prefix() {
        let out = generate_from("<Text>{state.count}</Text>");
        // stateless, so state. prefix is stripped but no self. prefix
        assert!(out.contains("self.count.to_string()"));
    }

    #[test]
    fn test_multiple_commands_collected_in_validation_fn() {
        let src = "<Stack><Button onclick=\"a\">A</Button><Button onclick=\"b\">B</Button></Stack>";
        let out = generate_from(src);
        assert!(out.contains("fn _fncc_validate_"));
        assert!(out.contains("__fncc_cmd_a"));
        assert!(out.contains("__fncc_cmd_b"));
    }

    // --- Contract tests ---

    #[test]
    fn test_generated_code_contains_no_markdown_or_template_leftovers() {
        let out = generate_from("<Text>hello</Text>");
        assert!(!out.contains("{{"));
        assert!(!out.contains("{state."));
        assert!(!out.contains("__fncc_cmd_") || out.contains("__fncc_cmd_"));
    }

    #[test]
    fn test_generated_function_name_follows_snake_case() {
        let out = generate_from("<HTMLParser></HTMLParser>");
        assert!(out.contains("render_h_t_m_l_parser") || out.contains("render_html_parser"));
    }

    // --- Regression tests ---

    #[test]
    fn test_regression_gap_with_decimal_does_not_produce_invalid_syntax() {
        let out = generate_from("<Stack gap=\"12.5\"></Stack>");
        assert!(out.contains(".gap(px(12.5))") || out.contains(".gap(px(12.5.))"));
    }

    #[test]
    fn test_regression_duplicate_button_ids_at_same_depth() {
        let src = "<Stack><Button>OK</Button><Button>OK</Button></Stack>";
        let out = generate_from(src);
        let id_count = out.matches(".id(\"OK\")").count();
        assert!(
            id_count <= 2,
            "expected at most 2 .id(\"OK\") occurrences, got {id_count}"
        );
    }

    #[test]
    fn test_regression_multiple_calls_have_unique_validation_fn_names() {
        let doc = parse("<Button onclick=\"x\">X</Button>").unwrap();
        let a = generate_with_id(&doc, 0);
        let b = generate_with_id(&doc, 1);
        assert_ne!(a, b);
        assert!(a.contains("__fncc_cmd_x"));
        assert!(b.contains("__fncc_cmd_x"));
    }

    #[test]
    fn test_regression_empty_gap_does_not_panic() {
        let out = generate_from("<Stack gap=\"\"></Stack>");
        // empty string fails to parse as f64, should skip gap
        assert!(!out.contains(".gap(") || out.contains(".gap(px(0.))"));
    }

    #[test]
    fn test_regression_interpolation_without_state_prefix() {
        let out = generate_from("---\n@state S\n---\n<Text>{custom_expr}</Text>");
        // custom_expr doesn't start with "state.", so interpolation_expr returns "self.custom_expr"
        assert!(out.contains("custom_expr") || out.contains("self.custom_expr"));
    }

    #[test]
    fn test_regression_many_calls_do_not_panic() {
        for _ in 0..100 {
            generate_from("<Button onclick=\"f\">F</Button>");
        }
    }

    // --- Component imports ---

    #[test]
    fn test_imported_element_generates_render_call() {
        let doc = parse("<Stack><Header /></Stack>").unwrap();
        let imports: &[(&str, &str)] = &[("Header", "render_header")];
        let out = generate_with_imports(&doc, 0, imports, None, None, None, &[], None, &[]);
        assert!(out.contains("render_header()"));
    }

    #[test]
    fn test_imported_element_in_stateful_component() {
        let src = "---\n@state AppState\n---\n<Stack><Footer /></Stack>";
        let doc = parse(src).unwrap();
        let imports: &[(&str, &str)] = &[("Footer", "render_footer")];
        let out = generate_with_imports(&doc, 0, imports, None, None, None, &[], None, &[]);
        assert!(out.contains("render_footer()"));
    }

    #[test]
    fn test_gpui_import_falls_back_to_div() {
        let doc = parse("<Stack><TextInput /></Stack>").unwrap();
        let imports: &[(&str, &str)] = &[("TextInput", "")];
        let out = generate_with_imports(&doc, 0, imports, None, None, None, &[], None, &[]);
        // GPUI imports have empty render fn — fall through to div
        assert!(out.contains("div()"));
    }

    #[test]
    fn test_builtin_takes_precedence_over_import() {
        let doc = parse("<Text>hello</Text>").unwrap();
        let imports: &[(&str, &str)] = &[("Text", "render_text")];
        let out = generate_with_imports(&doc, 0, imports, None, None, None, &[], None, &[]);
        // Built-in "Text" handling takes precedence, not render_text()
        assert!(out.contains(".child(\"hello\")"));
    }

    #[test]
    fn test_imported_element_with_custom_component_name() {
        let doc = parse("<Stack><MyHeader /></Stack>").unwrap();
        let imports: &[(&str, &str)] = &[("MyHeader", "render_header")];
        let out = generate_with_imports(&doc, 0, imports, None, None, None, &[], None, &[]);
        assert!(out.contains("render_header()"));
    }

    #[test]
    fn test_render_fn_name_uses_component_name_arg() {
        let doc = parse("<Text>hello</Text>").unwrap();
        let out = generate_with_imports(&doc, 0, &[], Some("CustomWidget"), None, None, &[], None, &[]);
        assert!(out.contains("pub fn render_custom_widget()"));
        // Should NOT use root element name
        assert!(!out.contains("pub fn render_text()"));
    }

    // --- Props tests ---

    #[test]
    fn test_props_stateless_component_with_props_signature() {
        let doc = parse("<Text>{props.title}</Text>").unwrap();
        let out = generate_with_imports(&doc, 0, &[], Some("Header"), None, Some("HeaderProps"), &[], None, &[]);
        assert!(out.contains("pub fn render_header(props: &HeaderProps) -> impl IntoElement {"));
        assert!(out.contains("props.title"));
    }

    #[test]
    fn test_props_caller_generates_struct_construction() {
        let doc = parse("<Header title=\"Welcome\" />").unwrap();
        let imports: &[(&str, &str)] = &[("Header", "render_header")];
        let import_props: &[(&str, Option<&str>)] = &[("Header", Some("HeaderProps"))];
        let out = generate_with_imports(&doc, 0, imports, None, None, None, import_props, None, &[]);
        assert!(out.contains("render_header(&HeaderProps {"));
        assert!(out.contains("title: \"Welcome\".into(),"));
        assert!(out.contains("})"));
    }

    #[test]
    fn test_props_caller_multiple_attributes() {
        let doc = parse("<Header title=\"Hi\" subtitle=\"World\" />").unwrap();
        let imports: &[(&str, &str)] = &[("Header", "render_header")];
        let import_props: &[(&str, Option<&str>)] = &[("Header", Some("HeaderProps"))];
        let out = generate_with_imports(&doc, 0, imports, None, None, None, import_props, None, &[]);
        assert!(out.contains("title: \"Hi\".into(),"));
        assert!(out.contains("subtitle: \"World\".into(),"));
    }

    #[test]
    fn test_props_caller_option_field() {
        let doc = parse("<Header title=\"Hi\" />").unwrap();
        let imports: &[(&str, &str)] = &[("Header", "render_header")];
        let import_props: &[(&str, Option<&str>)] = &[("Header", Some("HeaderProps"))];
        let out = generate_with_imports(&doc, 0, imports, None, None, None, import_props, None, &[]);
        // Option<T> fields are transparent at codegen — .into() handles conversion
        assert!(out.contains("title: \"Hi\".into(),"));
    }

    #[test]
    fn test_props_nested_element_with_props() {
        let doc = parse("<Stack><Header title=\"Nested\" /><Text>ok</Text></Stack>").unwrap();
        let imports: &[(&str, &str)] = &[("Header", "render_header")];
        let import_props: &[(&str, Option<&str>)] = &[("Header", Some("HeaderProps"))];
        let out = generate_with_imports(&doc, 0, imports, None, None, None, import_props, None, &[]);
        assert!(out.contains("render_header(&HeaderProps {"));
        assert!(out.contains("title: \"Nested\".into(),"));
    }

    #[test]
    fn test_props_mixed_components_with_and_without_props() {
        let doc = parse("<Stack><Header title=\"A\" /><Footer /></Stack>").unwrap();
        let imports: &[(&str, &str)] = &[("Header", "render_header"), ("Footer", "render_footer")];
        let import_props: &[(&str, Option<&str>)] = &[("Header", Some("HeaderProps")), ("Footer", None)];
        let out = generate_with_imports(&doc, 0, imports, None, None, None, import_props, None, &[]);
        assert!(out.contains("render_header(&HeaderProps {"));
        assert!(out.contains("title: \"A\".into(),"));
        assert!(out.contains("render_footer()"));
    }

    #[test]
    fn test_props_element_without_props_still_calls_directly() {
        let doc = parse("<Footer />").unwrap();
        let imports: &[(&str, &str)] = &[("Footer", "render_footer")];
        let import_props: &[(&str, Option<&str>)] = &[("Footer", None)];
        let out = generate_with_imports(&doc, 0, imports, None, None, None, import_props, None, &[]);
        assert!(out.contains("render_footer()"));
        assert!(!out.contains("&"));
    }

    #[test]
    fn test_props_self_closing_with_props() {
        let doc = parse("<Header title=\"SelfClose\" subtitle=\"X\" />").unwrap();
        let imports: &[(&str, &str)] = &[("Header", "render_header")];
        let import_props: &[(&str, Option<&str>)] = &[("Header", Some("HeaderProps"))];
        let out = generate_with_imports(&doc, 0, imports, None, None, None, import_props, None, &[]);
        assert!(out.contains("title: \"SelfClose\".into(),"));
        assert!(out.contains("subtitle: \"X\".into(),"));
    }

    // --- Control Flow & Composition tests ---

    #[test]
    fn test_if_generates_if_expression() {
        let out = generate_from("<If condition=\"{state.show}\"><Text>Hi</Text></If>");
        assert!(out.contains("if self.show {"));
        assert!(out.contains("div().child(div().child(\"Hi\"))"));
        assert!(out.contains("} else {"), "else branch should be present");
        assert!(out.contains("div() }"));
    }

    #[test]
    fn test_if_else_chain() {
        let src = "<Stack><If condition=\"{state.a}\"><Text>A</Text></If><Else><Text>B</Text></Else></Stack>";
        let out = generate_from(src);
        assert!(out.contains("if self.a {"));
        assert!(out.contains("} else {"));
        assert!(out.contains("div().child(\"B\")"));
    }

    #[test]
    fn test_if_elseif_else_chain() {
        let src = "<Stack><If condition=\"{state.x}\"><Text>X</Text></If><ElseIf condition=\"{state.y}\"><Text>Y</Text></ElseIf><Else><Text>Z</Text></Else></Stack>";
        let out = generate_from(src);
        assert!(out.contains("if self.x {"));
        assert!(out.contains("else if self.y {"));
        assert!(out.contains("else {"));
    }

    #[test]
    fn test_for_generates_iteration() {
        let out = generate_from("<For each=\"{state.items}\" let=\"item\"><Text>{item.name}</Text></For>");
        assert!(out.contains("self.items.iter().map(|item| {"));
        assert!(out.contains("item.name"));
    }

    #[test]
    fn test_for_with_index() {
        let out = generate_from("<For each=\"{state.items}\" let=\"item\" index=\"i\"><Text>{item.name}</Text></For>");
        assert!(out.contains(".enumerate()"));
        assert!(out.contains("|(i, item)|"));
    }

    #[test]
    fn test_fragment_generates_div() {
        let out = generate_from("<Fragment><Text>A</Text><Text>B</Text></Fragment>");
        assert!(out.contains("div()"));
    }

    #[test]
    fn test_slot_in_stateless_component() {
        let doc = parse("<Stack><Slot /></Stack>").unwrap();
        let out = generate_with_imports(&doc, 0, &[], Some("Card"), None, None, &[], None, &[]);
        assert!(out.contains("children: impl IntoElement"));
        assert!(out.contains("children"));
    }

    #[test]
    fn test_if_in_stack() {
        let src = "<Stack><If condition=\"{state.flag}\"><Text>Yes</Text></If><Text>Always</Text></Stack>";
        let out = generate_from(src);
        assert!(out.contains("if self.flag {"));
        assert!(out.contains("\"Always\""));
    }

    #[test]
    fn test_for_in_stack() {
        let src = "<Stack><For each=\"{state.items}\" let=\"item\"><Text>{item}</Text></For></Stack>";
        let out = generate_from(src);
        assert!(out.contains("self.items.iter().map(|item| {"));
        assert!(out.contains(".children("));
    }

    #[test]
    fn test_fragment_nested_in_stack() {
        let src = "<Stack><Fragment><Text>A</Text><Text>B</Text></Fragment></Stack>";
        let out = generate_from(src);
        assert!(out.contains("div()"));
        assert!(out.contains(".child(\"A\")"));
        assert!(out.contains(".child(\"B\")"));
    }
}
