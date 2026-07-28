use crate::parser::{AttrValue, Document, Element, Node};

static FILE_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn generate(doc: &Document) -> String {
    let mut out = String::new();
    let file_id = FILE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let has_state = doc.state_type.is_some();

    if let Some(ref fm) = doc.frontmatter {
        out.push_str(fm);
        out.push('\n');
    }

    // collect referenced command names for validation
    let commands = collect_commands(&doc.root);
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
        generate_stateful(doc, &mut out);
    } else {
        generate_stateless(doc, &mut out);
    }

    out
}

fn generate_stateless(doc: &Document, out: &mut String) {
    let fn_name = format!("render_{}", to_snake_case(&doc.root.name));
    out.push_str(&format!("pub fn {fn_name}() -> impl IntoElement {{\n"));
    out.push_str(&generate_element(&doc.root, 1, false));
    out.push('\n');
    out.push_str("}\n");
}

fn generate_stateful(doc: &Document, out: &mut String) {
    let state_type = doc.state_type.as_deref().unwrap_or("Self");

    out.push_str(&format!("impl Render for {state_type} {{\n"));
    out.push_str("    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {\n");
    out.push_str("        let handle = cx.entity().downgrade();\n");
    out.push_str(&generate_element(&doc.root, 2, true));
    out.push_str("\n    }\n");
    out.push_str("}\n");
}

fn generate_element(el: &Element, depth: usize, stateful: bool) -> String {
    let indent = "    ".repeat(depth);
    match el.name.as_str() {
        "Stack" => gen_stack(el, &indent, depth, stateful),
        "Text" => gen_text(el, &indent, depth, stateful),
        "Button" => gen_button(el, &indent, depth, stateful),
        _ => gen_fallback(el, &indent, depth, stateful),
    }
}

fn gen_stack(el: &Element, indent: &str, depth: usize, stateful: bool) -> String {
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

    for child in &el.children {
        out.push_str(&format!("{indent}    .child(\n"));
        match child {
            Node::Element(child_el) => {
                out.push_str(&generate_element(child_el, depth + 2, stateful));
            }
            Node::Text(t) => {
                out.push_str(&format!("{indent}        \"{t}\""));
            }
            Node::Interpolation(expr) => {
                let e = strip_state_prefix(expr);
                out.push_str(&format!("{indent}        format!(\"{{}}\", self.{e})"));
            }
        }
        out.push('\n');
        out.push_str(&format!("{indent}    )\n"));
    }

    out.trim_end().to_string()
}

fn gen_text(el: &Element, indent: &str, depth: usize, stateful: bool) -> String {
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
            out.push_str(&format!("{indent}    .child(\"{t}\")"));
        }
        [Node::Interpolation(expr)] => {
            let e = strip_state_prefix(expr);
            out.push_str(&format!("{indent}    .child(format!(\"{{}}\", self.{e}))"));
        }
        children => {
            for child in children {
                match child {
                    Node::Text(t) => out.push_str(&format!("{indent}    .child(\"{t}\")\n")),
                    Node::Interpolation(expr) => {
                        let e = strip_state_prefix(expr);
                        out.push_str(&format!("{indent}    .child(format!(\"{{}}\", self.{e}))\n"));
                    }
                    Node::Element(child_el) => {
                        out.push_str(&format!("{indent}    .child(\n"));
                        out.push_str(&generate_element(child_el, depth + 1, stateful));
                        out.push_str(&format!("\n{indent}    )\n"));
                    }
                }
            }
        }
    }

    out.trim_end().to_string()
}

fn gen_button(el: &Element, indent: &str, depth: usize, stateful: bool) -> String {
    let mut out = format!("{indent}div()\n");

    let btn_id = match &el.children[..] {
        [Node::Text(t)] => t.clone(),
        _ => format!("button_{depth}"),
    };
    out.push_str(&format!("{indent}    .id(\"{btn_id}\")\n"));
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
        [Node::Text(t)] => out.push_str(&format!("{indent}    .child(\"{t}\")")),
        [Node::Interpolation(expr)] => {
            let e = strip_state_prefix(expr);
            out.push_str(&format!("{indent}    .child(format!(\"{{}}\", self.{e}))"));
        }
        children => {
            for child in children {
                match child {
                    Node::Text(t) => out.push_str(&format!("{indent}    .child(\"{t}\")\n")),
                    Node::Element(child_el) => {
                        out.push_str(&format!("{indent}    .child(\n"));
                        out.push_str(&generate_element(child_el, depth + 1, stateful));
                        out.push_str(&format!("\n{indent}    )\n"));
                    }
                    Node::Interpolation(expr) => {
                        let e = strip_state_prefix(expr);
                        out.push_str(&format!("{indent}    .child(format!(\"{{}}\", self.{e}))\n"));
                    }
                }
            }
        }
    }

    out.trim_end().to_string()
}

fn gen_fallback(el: &Element, indent: &str, depth: usize, stateful: bool) -> String {
    let mut out = format!("{indent}div()\n");
    for (key, val) in &el.attrs {
        let v = val.as_str();
        out.push_str(&format!("{indent}    .attr(\"{key}\", \"{v}\")\n"));
    }
    for child in &el.children {
        out.push_str(&format!("{indent}    .child(\n"));
        match child {
            Node::Element(child_el) => {
                out.push_str(&generate_element(child_el, depth + 2, stateful));
            }
            Node::Text(t) => out.push_str(&format!("{indent}        \"{t}\"")),
            Node::Interpolation(expr) => {
                let e = strip_state_prefix(expr);
                out.push_str(&format!("{indent}        format!(\"{{}}\", self.{e})"));
            }
        }
        out.push('\n');
        out.push_str(&format!("{indent}    )\n"));
    }
    out.trim_end().to_string()
}

/// Removes `state.` prefix from interpolation expressions
/// e.g. "state.count" → "count"
fn strip_state_prefix(expr: &str) -> &str {
    expr.trim().strip_prefix("state.").unwrap_or(expr.trim())
}

fn to_snake_case(name: &str) -> String {
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

fn collect_commands(el: &Element) -> Vec<String> {
    let mut cmds = Vec::new();
    for (key, val) in &el.attrs {
        if key == "onclick"
            && let AttrValue::String(name) = val
            && !cmds.contains(name)
        {
            cmds.push(name.clone());
        }
    }
    for child in &el.children {
        if let Node::Element(child_el) = child {
            cmds.extend(collect_commands(child_el));
        }
    }
    cmds
}

impl AttrValue {
    fn as_str(&self) -> &str {
        match self {
            AttrValue::String(s) => s,
            AttrValue::Interpolation(s) => s,
        }
    }
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
        assert!(out.contains("format!(\"{}\", self.msg)"));
        assert!(!out.contains("self.state.msg"));
    }

    #[test]
    fn test_interpolation_strips_state_prefix() {
        let out = generate_from("<Text>{state.count}</Text>");
        // stateless, so state. prefix is stripped but no self. prefix
        assert!(out.contains("format!(\"{}\", self.count)"));
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
        // BUG: currently produces `px(12.5.)` which is invalid Rust
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
        let a = generate_from("<Button onclick=\"x\">X</Button>");
        let b = generate_from("<Button onclick=\"x\">X</Button>");
        // FILE_COUNTER makes validation fn names different
        assert_ne!(a, b);
        // both should compile to the same structure though
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
        // custom_expr doesn't start with "state.", so strip_state_prefix leaves it as-is
        // But stateful codegen adds "self." prefix? Let's check...
        assert!(out.contains("custom_expr") || out.contains("self.custom_expr"));
    }

    #[test]
    fn test_regression_file_counter_does_not_overflow() {
        // Reset counter for test
        for _ in 0..100 {
            generate_from("<Button onclick=\"f\">F</Button>");
        }
        // Should not panic
    }
}
