use pest::Parser;

#[derive(pest_derive::Parser)]
#[grammar = "parser/fncc.pest"]
pub struct FnccParser;

use pest::iterators::Pair;

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub frontmatter: Option<String>,
    pub state_type: Option<String>,
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
    let mut pairs = FnccParser::parse(Rule::document, source)
        .map_err(|e| format!("parse error: {e}"))?;

    let pair = pairs.next().expect("document should exist");

    let mut frontmatter = None;
    let mut state_type = None;
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
                    // extract @state directive
                    let mut clean_lines = Vec::new();
                    for line in raw.lines() {
                        let trimmed = line.trim();
                        if let Some(st) = trimmed.strip_prefix("@state ") {
                            state_type = Some(st.trim().to_string());
                        } else {
                            clean_lines.push(line);
                        }
                    }
                    let clean = clean_lines.join("\n");
                    if !clean.trim().is_empty() {
                        frontmatter = Some(clean);
                    }
                }
            }
            Rule::element => {
                root = Some(parse_element(inner));
            }
            _ => {}
        }
    }

    Ok(Document {
        frontmatter,
        state_type,
        root: root.expect("document must have a root element"),
    })
}

fn parse_element(pair: Pair<Rule>) -> Element {
    let mut name = String::new();
    let mut attrs = Vec::new();
    let mut children = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::open_tag => {
                // extract tag_name and attrs from open_tag
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
                // self-closing tag: extract tag_name and attrs
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
                    // children contains nodes; unwrap node.inner
                    let actual = child.into_inner().next()
                        .expect("node should have one child");
                    match actual.as_rule() {
                        Rule::element => {
                            children.push(Node::Element(parse_element(actual)));
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
                // nothing to extract from close_tag for now
            }
            _ => {}
        }
    }

    Element { name, attrs, children }
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
                attr_value = AttrValue::String(val.to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_element() {
        let src = "<Text size=\"xl\">hello</Text>";
        let doc = parse(src).unwrap();
        assert_eq!(doc.root.name, "Text");
        assert_eq!(doc.root.attrs.len(), 1);
        assert_eq!(doc.root.attrs[0].0, "size");
    }

    #[test]
    fn test_frontmatter() {
        let src = "---\nuse crate::lib::State;\n---\n<App></App>";
        let doc = parse(src).unwrap();
        assert_eq!(doc.frontmatter, Some("use crate::lib::State;".to_string()));
        assert_eq!(doc.root.name, "App");
    }

    #[test]
    fn test_interpolation() {
        let src = "<Text>{state.msg}</Text>";
        let doc = parse(src).unwrap();
        assert_eq!(doc.root.children.len(), 1);
        assert_eq!(doc.root.children[0], Node::Interpolation("state.msg".to_string()));
    }

    #[test]
    fn test_nested_elements() {
        let src = "<Stack direction=\"vertical\">\n  <Text>oi</Text>\n</Stack>";
        let doc = parse(src).unwrap();
        assert_eq!(doc.root.name, "Stack");
        assert_eq!(doc.root.children.len(), 1);
        if let Node::Element(child) = &doc.root.children[0] {
            assert_eq!(child.name, "Text");
        } else {
            panic!("expected element node");
        }
    }
}
