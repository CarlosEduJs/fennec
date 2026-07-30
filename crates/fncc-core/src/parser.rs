use pest::Parser;

#[derive(pest_derive::Parser)]
#[grammar = "parser/fncc.pest"]
pub struct FnccParser;

use pest::iterators::Pair;

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentImport {
    pub name: String,
    pub source: ImportSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportSource {
    /// Path to another .fui file, e.g. "ui::components::Button"
    FuiPath(String),
    /// A GPUI native component, e.g. `use gpui::TextInput;`
    Gpui,
    /// A props type, e.g. `use props::HeaderProps;`
    PropsType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub frontmatter: Option<String>,
    pub state_type: Option<String>,
    pub imports: Vec<ComponentImport>,
    pub root: Element,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, AttrValue)>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttrValue {
    String(String),
    Interpolation(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
    Interpolation(String),
}

pub fn parse(source: &str) -> Result<Document, String> {
    let mut pairs = FnccParser::parse(Rule::document, source).map_err(|e| format!("parse error: {e}"))?;

    let pair = pairs.next().expect("document should exist");

    let mut frontmatter = None;
    let mut state_type = None;
    let mut imports = Vec::new();
    let mut root = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::frontmatter => {
                let content = inner.as_str().trim();
                let content = content
                    .strip_prefix("---")
                    .and_then(|s| s.strip_suffix("---"))
                    .map(|s| s.trim().to_string());

                if let Some(ref raw) = content {
                    // extract @state directive and component imports
                    let mut clean_lines = Vec::new();
                    let mut component_imports = Vec::new();
                    for line in raw.lines() {
                        let trimmed = line.trim();
                        if let Some(st) = trimmed.strip_prefix("@state ") {
                            state_type = Some(st.trim().to_string());
                        } else if is_component_import_line(trimmed) {
                            if let Some(imports) = parse_component_imports(trimmed) {
                                component_imports.extend(imports);
                            }
                            // Only strip from emitted Rust if it's a .fui import,
                            // not a gpui import (those are real Rust)
                            if !trimmed.starts_with("use gpui::") {
                                continue;
                            }
                            clean_lines.push(line);
                        } else {
                            clean_lines.push(line);
                        }
                    }
                    imports = component_imports;
                    let clean = clean_lines.join("\n");
                    if !clean.trim().is_empty() {
                        frontmatter = Some(clean);
                    }
                }
            }
            Rule::element => {
                root = Some(parse_element(inner)?);
            }
            _ => {}
        }
    }

    Ok(Document {
        frontmatter,
        state_type,
        imports,
        root: root.expect("document must have a root element"),
    })
}

fn parse_element(pair: Pair<Rule>) -> Result<Element, String> {
    let mut name = String::new();
    let mut attrs = Vec::new();
    let mut children = Vec::new();
    let mut close_name: Option<String> = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::open_tag => {
                for tag_inner in inner.into_inner() {
                    match tag_inner.as_rule() {
                        Rule::tag_name => {
                            name = tag_inner.as_str().to_string();
                        }
                        Rule::attr => {
                            let (aname, avalue) = parse_attr(tag_inner);
                            attrs.push((aname, avalue));
                        }
                        _ => {}
                    }
                }
            }
            Rule::self_closing_tag => {
                for tag_inner in inner.into_inner() {
                    match tag_inner.as_rule() {
                        Rule::tag_name => {
                            name = tag_inner.as_str().to_string();
                        }
                        Rule::attr => {
                            let (aname, avalue) = parse_attr(tag_inner);
                            attrs.push((aname, avalue));
                        }
                        _ => {}
                    }
                }
            }
            Rule::children => {
                for child in inner.into_inner() {
                    let actual = child.into_inner().next().expect("node should have one child");
                    match actual.as_rule() {
                        Rule::element => {
                            children.push(Node::Element(parse_element(actual)?));
                        }
                        Rule::inner_text => {
                            let text = actual.as_str().trim().to_string();
                            if !text.is_empty() {
                                children.push(Node::Text(text));
                            }
                        }
                        Rule::interpolation => {
                            let expr = actual.as_str().trim();
                            let expr = expr.strip_prefix('{').and_then(|s| s.strip_suffix('}')).unwrap_or(expr);
                            children.push(Node::Interpolation(expr.to_string()));
                        }
                        _ => {}
                    }
                }
            }
            Rule::close_tag => {
                for tag_inner in inner.into_inner() {
                    if tag_inner.as_rule() == Rule::tag_name {
                        close_name = Some(tag_inner.as_str().to_string());
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(ref close) = close_name
        && &name != close
    {
        return Err(format!("mismatched close tag: </{close}> does not match <{name}>"));
    }

    Ok(Element { name, attrs, children })
}

fn parse_attr(pair: Pair<Rule>) -> (String, AttrValue) {
    let mut attr_name = String::new();
    let mut attr_value = AttrValue::String(String::new());

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::attr_name => {
                attr_name = inner.as_str().to_string();
            }
            Rule::attr_value => {
                let val = inner.as_str();
                let val = val.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(val);
                let trimmed = val.trim();
                if trimmed.starts_with('{') && trimmed.ends_with('}') {
                    let expr = &trimmed[1..trimmed.len() - 1].trim();
                    attr_value = AttrValue::Interpolation(expr.to_string());
                } else {
                    attr_value = AttrValue::String(val.to_string());
                }
            }
            Rule::interpolation => {
                let expr = inner.as_str().trim();
                let expr = expr.strip_prefix('{').and_then(|s| s.strip_suffix('}')).unwrap_or(expr);
                attr_value = AttrValue::Interpolation(expr.to_string());
            }
            _ => {}
        }
    }

    (attr_name, attr_value)
}

/// Check if a trimmed line is a component import (`use ui::...`, `use gpui::...`, or `use props::...`)
fn is_component_import_line(line: &str) -> bool {
    (line.starts_with("use ui::") || line.starts_with("use gpui::") || line.starts_with("use props::"))
        && line.ends_with(';')
}

/// Parse component imports from a `use` line.
/// Supports:
/// - `use ui::components::Button;`
/// - `use ui::components::{Button, Input};`
/// - `use gpui::TextInput;`
/// - `use props::HeaderProps;`
fn parse_component_imports(line: &str) -> Option<Vec<ComponentImport>> {
    let trimmed = line.strip_prefix("use ")?.strip_suffix(';')?.trim();
    if trimmed.starts_with("ui::") {
        parse_fui_imports(trimmed)
    } else if trimmed.starts_with("gpui::") {
        parse_gpui_imports(trimmed)
    } else if trimmed.starts_with("props::") {
        parse_props_imports(trimmed)
    } else {
        None
    }
}

fn parse_fui_imports(path: &str) -> Option<Vec<ComponentImport>> {
    // Remove leading "ui::"
    let rest = path.strip_prefix("ui::")?;
    if let Some(brace_start) = rest.find('{') {
        // Grouped: "components::{Button, Input}"
        let prefix = rest[..brace_start].trim_end_matches("::");
        let inner = &rest[brace_start + 1..];
        let inner = inner.split('}').next()?.trim();
        let names: Vec<String> = inner
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if names.is_empty() {
            return None;
        }
        Some(
            names
                .into_iter()
                .map(|name| {
                    let source = if prefix.is_empty() {
                        format!("ui::{name}")
                    } else {
                        format!("ui::{prefix}::{name}")
                    };
                    ComponentImport {
                        name: name.clone(),
                        source: ImportSource::FuiPath(source),
                    }
                })
                .collect(),
        )
    } else {
        // Simple: "components::Button"
        let name = rest.split("::").last()?.to_string();
        Some(vec![ComponentImport {
            name: name.clone(),
            source: ImportSource::FuiPath(format!("ui::{}", rest)),
        }])
    }
}

/// Check if an element tree contains a `<Slot>` element.
pub fn has_slot(el: &Element) -> bool {
    if el.name == "Slot" {
        return true;
    }
    for child in &el.children {
        if let Node::Element(child_el) = child
            && has_slot(child_el)
        {
            return true;
        }
    }
    false
}

/// Check whether an element tree contains `{props.field}` interpolation.
/// Used to determine if a component actually receives props (vs merely referencing
/// a PropsType import for a child component).
pub fn uses_props_interpolation(el: &Element) -> bool {
    for (_, val) in &el.attrs {
        if let AttrValue::Interpolation(expr) = val
            && expr.trim().starts_with("props.")
        {
            return true;
        }
    }
    for child in &el.children {
        match child {
            Node::Interpolation(expr) => {
                if expr.trim().starts_with("props.") {
                    return true;
                }
            }
            Node::Element(child_el) => {
                if uses_props_interpolation(child_el) {
                    return true;
                }
            }
            Node::Text(_) => {}
        }
    }
    false
}

/// Collect all command names referenced via `onclick` attributes in an element tree.
pub fn collect_commands(el: &Element) -> Vec<String> {
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

/// Get the `each` attribute value from a `<For>` element.
pub fn get_each_attr(el: &Element) -> Option<&str> {
    el.attrs.iter().find(|(n, _)| n == "each").map(|(_, v)| v.as_str())
}

/// Get the `let` attribute value from a `<For>` element.
pub fn get_let_attr(el: &Element) -> Option<&str> {
    el.attrs.iter().find(|(n, _)| n == "let").map(|(_, v)| v.as_str())
}

/// Get the `index` attribute value from a `<For>` element.
pub fn get_index_attr(el: &Element) -> Option<&str> {
    el.attrs.iter().find(|(n, _)| n == "index").map(|(_, v)| v.as_str())
}

/// Get the `condition` attribute value from an `<If>` or `<ElseIf>` element.
pub fn get_condition_attr(el: &Element) -> Option<&str> {
    el.attrs.iter().find(|(n, _)| n == "condition").map(|(_, v)| v.as_str())
}

impl AttrValue {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            AttrValue::String(s) => s,
            AttrValue::Interpolation(s) => s,
        }
    }
}

fn parse_gpui_imports(path: &str) -> Option<Vec<ComponentImport>> {
    let name = path.strip_prefix("gpui::")?;
    // No grouped imports for gpui (for now)
    if name.contains('{') || name.contains('}') || name.contains("::") {
        return None;
    }
    Some(vec![ComponentImport {
        name: name.to_string(),
        source: ImportSource::Gpui,
    }])
}

fn parse_props_imports(path: &str) -> Option<Vec<ComponentImport>> {
    let rest = path.strip_prefix("props::")?;
    if let Some(brace_start) = rest.find('{') {
        // Grouped: "props::{HeaderProps, FooterProps}"
        let inner = &rest[brace_start + 1..];
        let inner = inner.split('}').next()?.trim();
        let names: Vec<String> = inner
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if names.is_empty() {
            return None;
        }
        Some(
            names
                .into_iter()
                .map(|name| ComponentImport {
                    name,
                    source: ImportSource::PropsType,
                })
                .collect(),
        )
    } else if rest.contains("::") {
        None
    } else {
        Some(vec![ComponentImport {
            name: rest.to_string(),
            source: ImportSource::PropsType,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Happy path ---

    #[test]
    fn test_simple_element_parses_correctly() {
        let doc = parse("<Text size=\"xl\">hello</Text>").unwrap();
        assert_eq!(doc.root.name, "Text");
        assert_eq!(doc.root.attrs.len(), 1);
        assert_eq!(doc.root.attrs[0].0, "size");
        assert_eq!(doc.root.attrs[0].1, AttrValue::String("xl".into()));
        assert_eq!(doc.root.children.len(), 1);
        assert_eq!(doc.root.children[0], Node::Text("hello".into()));
    }

    #[test]
    fn test_frontmatter_with_imports() {
        let doc = parse("---\nuse crate::lib::State;\n---\n<App></App>").unwrap();
        assert_eq!(doc.frontmatter, Some("use crate::lib::State;".to_string()));
        assert_eq!(doc.root.name, "App");
    }

    #[test]
    fn test_interpolation_in_text() {
        let doc = parse("<Text>{state.msg}</Text>").unwrap();
        assert_eq!(doc.root.children.len(), 1);
        assert_eq!(doc.root.children[0], Node::Interpolation("state.msg".to_string()));
    }

    #[test]
    fn test_nested_elements_with_attrs() {
        let src = "<Stack direction=\"vertical\">\n  <Text>oi</Text>\n</Stack>";
        let doc = parse(src).unwrap();
        assert_eq!(doc.root.name, "Stack");
        assert_eq!(doc.root.children.len(), 1);
        let child = match &doc.root.children[0] {
            Node::Element(el) => el,
            _ => panic!("expected element node"),
        };
        assert_eq!(child.name, "Text");
        assert_eq!(child.children.len(), 1);
        assert_eq!(child.children[0], Node::Text("oi".to_string()));
    }

    #[test]
    fn test_frontmatter_with_state_directive() {
        let doc = parse("---\n@state CounterState\n---\n<App></App>").unwrap();
        assert_eq!(doc.state_type, Some("CounterState".to_string()));
        assert_eq!(doc.frontmatter, None);
    }

    #[test]
    fn test_frontmatter_with_state_and_imports() {
        let doc = parse("---\nuse crate::state::MyState;\n@state MyState\n---\n<App></App>").unwrap();
        assert_eq!(doc.state_type, Some("MyState".to_string()));
        assert_eq!(doc.frontmatter, Some("use crate::state::MyState;".to_string()));
    }

    #[test]
    fn test_self_closing_tag() {
        let doc = parse("<Button onclick=\"handle_click\" />").unwrap();
        assert_eq!(doc.root.name, "Button");
        assert_eq!(doc.root.attrs.len(), 1);
        assert_eq!(doc.root.attrs[0].0, "onclick");
        assert!(doc.root.children.is_empty());
    }

    #[test]
    fn text_interpolation_in_attribute() {
        let doc = parse("<Text size=\"{state.size}\">hey</Text>").unwrap();
        assert_eq!(doc.root.attrs[0].1, AttrValue::Interpolation("state.size".into()));
    }

    #[test]
    fn test_multiple_attributes() {
        let src = "<Stack direction=\"horizontal\" gap=\"16\" id=\"main-stack\"></Stack>";
        let doc = parse(src).unwrap();
        assert_eq!(doc.root.attrs.len(), 3);
        assert_eq!(doc.root.attrs[0].0, "direction");
        assert_eq!(doc.root.attrs[1].0, "gap");
        assert_eq!(doc.root.attrs[2].0, "id");
    }

    // --- Edge cases ---

    #[test]
    fn test_element_with_no_children() {
        let doc = parse("<Div></Div>").unwrap();
        assert_eq!(doc.root.name, "Div");
        assert!(doc.root.children.is_empty());
    }

    #[test]
    fn test_element_with_no_attributes() {
        let doc = parse("<View></View>").unwrap();
        assert_eq!(doc.root.name, "View");
        assert!(doc.root.attrs.is_empty());
    }

    #[test]
    fn test_whitespace_only_text_is_ignored() {
        let doc = parse("<Text>   \n  </Text>").unwrap();
        assert!(doc.root.children.is_empty());
    }

    #[test]
    fn test_mixed_children_text_and_interpolation() {
        let src = "<Text>Hello {name} !</Text>";
        let doc = parse(src).unwrap();
        assert_eq!(doc.root.children.len(), 3);
        assert_eq!(doc.root.children[0], Node::Text("Hello".into()));
        assert_eq!(doc.root.children[1], Node::Interpolation("name".into()));
    }

    #[test]
    fn test_deeply_nested_elements() {
        let src = "<A><B><C><D><E></E></D></C></B></A>";
        let doc = parse(src).unwrap();
        assert_eq!(doc.root.name, "A");
        match &doc.root.children[0] {
            Node::Element(b) => {
                assert_eq!(b.name, "B");
                match &b.children[0] {
                    Node::Element(c) => {
                        assert_eq!(c.name, "C");
                        match &c.children[0] {
                            Node::Element(d) => {
                                assert_eq!(d.name, "D");
                                match &d.children[0] {
                                    Node::Element(e) => assert_eq!(e.name, "E"),
                                    _ => panic!("expected element E"),
                                }
                            }
                            _ => panic!("expected element D"),
                        }
                    }
                    _ => panic!("expected element C"),
                }
            }
            _ => panic!("expected element B"),
        }
    }

    #[test]
    fn test_tag_name_with_hyphen() {
        let doc = parse("<my-component></my-component>").unwrap();
        assert_eq!(doc.root.name, "my-component");
    }

    #[test]
    fn test_tag_name_starting_with_underscore() {
        let doc = parse("<_custom></_custom>").unwrap();
        assert_eq!(doc.root.name, "_custom");
    }

    #[test]
    fn test_frontmatter_only_with_state_and_other_lines() {
        let src = "---\n@state MyState\nconst X: i32 = 42;\n---\n<Root></Root>";
        let doc = parse(src).unwrap();
        assert_eq!(doc.state_type, Some("MyState".into()));
        assert_eq!(doc.frontmatter, Some("const X: i32 = 42;".into()));
    }

    #[test]
    fn test_multiline_interpolation_expression() {
        let src = "<Text>{ state . count }</Text>";
        let doc = parse(src).unwrap();
        match &doc.root.children[0] {
            Node::Interpolation(expr) => {
                assert!(expr.contains("state"));
                assert!(expr.contains("count"));
            }
            _ => panic!("expected interpolation"),
        }
    }

    // --- Invalid inputs / Error handling ---

    #[test]
    fn test_empty_string_returns_error() {
        let result = parse("");
        assert!(result.is_err());
    }

    #[test]
    fn test_only_whitespace_returns_error() {
        let result = parse("   \n  \t  ");
        assert!(result.is_err());
    }

    #[test]
    fn test_unclosed_tag_returns_error() {
        let result = parse("<Text>unclosed");
        assert!(result.is_err());
    }

    #[test]
    fn test_unopened_close_tag_returns_error() {
        let result = parse("</Text>");
        assert!(result.is_err());
    }

    #[test]
    fn test_mismatched_close_tag_returns_error() {
        let result = parse("<Div></Text>");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("mismatched close tag"), "error: {err}");
    }

    #[test]
    fn test_invalid_tag_name_returns_error() {
        let result = parse("<123invalid></123invalid>");
        assert!(result.is_err());
    }

    #[test]
    fn test_unclosed_frontmatter_returns_error() {
        let result = parse("---\n@state Foo\n<Root></Root>");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_attribute_syntax_returns_error() {
        let result = parse("<Text size=>hello</Text>");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_attribute_value_no_quotes_returns_error() {
        let result = parse("<Text size=xl>hello</Text>");
        assert!(result.is_err());
    }

    #[test]
    fn test_self_closing_tag_with_content_before_close_errors() {
        // This should parse as: <Text> with children "hello" and then <Button /> after
        // Actually, it's valid because <Text>hello<Button/ ></Text> is well-formed
        let result = parse("<Text>hello<Button/></Text>");
        assert!(result.is_ok());
    }

    // --- Contract / Structural tests ---

    #[test]
    fn test_document_structure_contract() {
        let doc = parse("<Root></Root>").unwrap();
        assert_eq!(doc.root.name, "Root");
        assert!(doc.frontmatter.is_none());
        assert!(doc.state_type.is_none());
        assert!(doc.root.attrs.is_empty());
        assert!(doc.root.children.is_empty());
    }

    #[test]
    fn test_node_variants_contract() {
        let src = r#"<Container>
            text content
            <Inner />
            {interp}
        </Container>"#;
        let doc = parse(src).unwrap();
        let types: Vec<&str> = doc
            .root
            .children
            .iter()
            .map(|n| match n {
                Node::Text(_) => "text",
                Node::Element(_) => "element",
                Node::Interpolation(_) => "interpolation",
            })
            .collect();
        assert_eq!(types, ["text", "element", "interpolation"]);
    }

    #[test]
    fn test_attr_value_types_contract() {
        let src = "<Text size=\"lg\" data-value=\"{expr}\" />";
        let doc = parse(src).unwrap();
        assert_eq!(doc.root.attrs[0].1, AttrValue::String("lg".into()));
        assert_eq!(doc.root.attrs[1].1, AttrValue::Interpolation("expr".into()));
    }

    // --- Regression tests ---

    #[test]
    fn test_regression_trailing_whitespace_after_tag() {
        let doc = parse("<Text>hello</Text>  ").unwrap();
        assert_eq!(doc.root.name, "Text");
    }

    #[test]
    fn test_regression_leading_whitespace_before_tag() {
        let doc = parse("  <Text>hello</Text>").unwrap();
        assert_eq!(doc.root.name, "Text");
    }

    #[test]
    fn test_regression_nested_same_component_name() {
        let src = "<Item><Item><Item></Item></Item></Item>";
        let doc = parse(src).unwrap();
        assert_eq!(doc.root.name, "Item");
        if let Node::Element(child) = &doc.root.children[0] {
            assert_eq!(child.name, "Item");
            if let Node::Element(grandchild) = &child.children[0] {
                assert_eq!(grandchild.name, "Item");
            } else {
                panic!("expected inner Item");
            }
        } else {
            panic!("expected child Item");
        }
    }

    #[test]
    fn test_regression_interpolation_with_adjacent_text() {
        let doc = parse("<Text>Count: {count} items</Text>").unwrap();
        assert_eq!(doc.root.children.len(), 3);
        assert_eq!(doc.root.children[0], Node::Text("Count:".into()));
        assert_eq!(doc.root.children[1], Node::Interpolation("count".into()));
        assert_eq!(doc.root.children[2], Node::Text("items".into()));
    }

    // --- Component imports ---

    #[test]
    fn test_simple_use_ui_import() {
        let doc = parse("---\nuse ui::components::Button;\n---\n<Stack></Stack>").unwrap();
        assert_eq!(doc.imports.len(), 1);
        assert_eq!(doc.imports[0].name, "Button");
        assert_eq!(
            doc.imports[0].source,
            ImportSource::FuiPath("ui::components::Button".into())
        );
    }

    #[test]
    fn test_use_ui_import_stripped_from_frontmatter() {
        let doc = parse("---\nuse ui::components::Button;\n---\n<Stack></Stack>").unwrap();
        assert!(doc.frontmatter.is_none());
    }

    #[test]
    fn test_use_crate_import_preserved() {
        let doc = parse("---\nuse crate::lib::State;\n---\n<App></App>").unwrap();
        assert_eq!(doc.frontmatter, Some("use crate::lib::State;".to_string()));
        assert!(doc.imports.is_empty());
    }

    #[test]
    fn test_use_ui_and_crate_mixed() {
        let src = "---\nuse crate::prelude::*;\nuse ui::components::Header;\n---\n<App></App>";
        let doc = parse(src).unwrap();
        assert_eq!(doc.imports.len(), 1);
        assert_eq!(doc.imports[0].name, "Header");
        assert!(doc.frontmatter.as_ref().unwrap().contains("use crate::prelude::*;"));
        assert!(!doc.frontmatter.as_ref().unwrap().contains("use ui::"));
    }

    #[test]
    fn test_grouped_use_ui_import() {
        let src = "---\nuse ui::components::{Button, Input, Card};\n---\n<Stack></Stack>";
        let doc = parse(src).unwrap();
        assert_eq!(doc.imports.len(), 3);
        assert_eq!(doc.imports[0].name, "Button");
        assert_eq!(doc.imports[1].name, "Input");
        assert_eq!(doc.imports[2].name, "Card");
        for imp in &doc.imports {
            match &imp.source {
                ImportSource::FuiPath(p) => assert!(p.starts_with("ui::components::")),
                _ => panic!("expected FuiPath"),
            }
        }
    }

    #[test]
    fn test_use_gpui_import_kept_in_frontmatter() {
        let doc = parse("---\nuse gpui::TextInput;\n---\n<Stack></Stack>").unwrap();
        assert_eq!(doc.imports.len(), 1);
        assert_eq!(doc.imports[0].name, "TextInput");
        assert_eq!(doc.imports[0].source, ImportSource::Gpui);
        // gpui imports are real Rust — stay in frontmatter
        assert!(doc.frontmatter.as_ref().unwrap().contains("use gpui::TextInput;"));
    }

    #[test]
    fn test_use_ui_with_state_and_imports() {
        let src = "---\n@state AppState\nuse ui::components::Header;\n---\n<Stack></Stack>";
        let doc = parse(src).unwrap();
        assert_eq!(doc.state_type, Some("AppState".into()));
        assert_eq!(doc.imports.len(), 1);
        assert_eq!(doc.imports[0].name, "Header");
    }

    #[test]
    fn test_imports_default_empty() {
        let doc = parse("<Text>hi</Text>").unwrap();
        assert!(doc.imports.is_empty());
    }

    #[test]
    fn test_use_ui_with_deep_path() {
        let doc = parse("---\nuse ui::layout::sidebar::SidePanel;\n---\n<Root></Root>").unwrap();
        assert_eq!(doc.imports.len(), 1);
        assert_eq!(doc.imports[0].name, "SidePanel");
        assert_eq!(
            doc.imports[0].source,
            ImportSource::FuiPath("ui::layout::sidebar::SidePanel".into())
        );
    }
}
