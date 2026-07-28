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
        out.push_str(&format!("fn _fennec_validate_{file_id}() {{\n"));
        for cmd in &commands {
            let trampoline = format!("__fennec_cmd_{cmd}");
            out.push_str(&format!("    let _ = {trampoline};\n"));
        }
        out.push_str("}\n\n");
    }

    if has_state {
        generate_stateful(&doc, &mut out);
    } else {
        generate_stateless(&doc, &mut out);
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
        match key.as_str() {
            "size" => {
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
            _ => {}
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
        match key.as_str() {
            "onclick" => {
                let handler = val.as_str();
                let trampoline = format!("__fennec_cmd_{handler}");
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
            _ => {}
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
        if key == "onclick" {
            if let AttrValue::String(name) = val {
                if !cmds.contains(name) {
                    cmds.push(name.clone());
                }
            }
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
