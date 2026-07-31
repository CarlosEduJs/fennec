use crate::Stylesheet;

pub fn parse(css: &str) -> Result<Stylesheet, String> {
    let mut ss = Stylesheet::default();

    let mut expecting: Option<BlockKind> = None;
    let mut block_name: Option<String> = None;
    let mut block_props: Vec<(String, String)> = Vec::new();

    let lines: Vec<&str> = css.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let raw_line = lines[i];
        let trimmed = raw_line.trim().to_string();
        i += 1;

        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        if trimmed.starts_with("/*") {
            while i < lines.len() && !trimmed.contains("*/") {}
            continue;
        }

        if trimmed.starts_with(":root") && trimmed.contains('{') {
            finalize(&mut expecting, &mut block_name, &mut block_props, &mut ss);
            expecting = Some(BlockKind::Tokens);
            block_name = None;
            block_props = Vec::new();
            let after_brace = trimmed.split('{').nth(1).unwrap_or("");
            if let Some(close_pos) = after_brace.rfind('}') {
                let content = &after_brace[..close_pos];
                parse_properties_into(content, &mut block_props);
                finalize(&mut expecting, &mut block_name, &mut block_props, &mut ss);
            } else if !after_brace.trim().is_empty() {
                parse_properties_into(after_brace, &mut block_props);
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("theme ")
            && let Some(name) = rest.split('{').next().map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            finalize(&mut expecting, &mut block_name, &mut block_props, &mut ss);
            expecting = Some(BlockKind::Theme);
            block_name = Some(name.to_string());
            block_props = Vec::new();
            let after_brace = rest.split_once('{').map(|x| x.1).unwrap_or("");
            if let Some(close_pos) = after_brace.rfind('}') {
                let content = &after_brace[..close_pos];
                parse_properties_into(content, &mut block_props);
                finalize(&mut expecting, &mut block_name, &mut block_props, &mut ss);
            } else if !after_brace.trim().is_empty() {
                parse_properties_into(after_brace, &mut block_props);
            }
            continue;
        }

        if trimmed.starts_with("@font-face") && trimmed.contains('{') {
            finalize(&mut expecting, &mut block_name, &mut block_props, &mut ss);
            expecting = Some(BlockKind::FontFace);
            block_name = None;
            block_props = Vec::new();
            let after_brace = trimmed.split_once('{').map(|x| x.1).unwrap_or("");
            if let Some(close_pos) = after_brace.rfind('}') {
                let content = &after_brace[..close_pos];
                parse_properties_into(content, &mut block_props);
                finalize(&mut expecting, &mut block_name, &mut block_props, &mut ss);
            } else if !after_brace.trim().is_empty() {
                parse_properties_into(after_brace, &mut block_props);
            }
            continue;
        }

        if let Some(dot_rest) = trimmed.strip_prefix('.')
            && let Some(name) = dot_rest.split('{').next().map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            finalize(&mut expecting, &mut block_name, &mut block_props, &mut ss);
            expecting = Some(BlockKind::Rules);
            block_name = Some(name.to_string());
            block_props = Vec::new();
            let after_brace = dot_rest.split_once('{').map(|x| x.1).unwrap_or("");
            if let Some(close_pos) = after_brace.rfind('}') {
                let content = &after_brace[..close_pos];
                parse_properties_into(content, &mut block_props);
                finalize(&mut expecting, &mut block_name, &mut block_props, &mut ss);
            } else if !after_brace.trim().is_empty() {
                parse_properties_into(after_brace, &mut block_props);
            }
            continue;
        }

        if trimmed == "}" {
            finalize(&mut expecting, &mut block_name, &mut block_props, &mut ss);
            continue;
        }

        if expecting.is_some()
            && let Some((prop_name, rest)) = trimmed.split_once(':')
        {
            let prop_value = rest.trim().trim_end_matches(';').trim();
            if !prop_name.trim().is_empty() {
                block_props.push((prop_name.trim().to_string(), prop_value.to_string()));
            }
        }
    }

    finalize(&mut expecting, &mut block_name, &mut block_props, &mut ss);

    Ok(ss)
}

pub fn parse_inline(style: &str) -> Result<Vec<(String, String)>, String> {
    let mut props = Vec::new();
    for segment in style.split(';') {
        let seg = segment.trim();
        if seg.is_empty() {
            continue;
        }
        if let Some((name, value)) = seg.split_once(':') {
            props.push((name.trim().to_string(), value.trim().to_string()));
        } else {
            return Err(format!("invalid inline style: `{seg}`"));
        }
    }
    Ok(props)
}

fn parse_properties_into(content: &str, props: &mut Vec<(String, String)>) {
    for part in content.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((name, value)) = part.split_once(':') {
            props.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum BlockKind {
    Tokens,
    Theme,
    Rules,
    FontFace,
}

fn finalize(
    expecting: &mut Option<BlockKind>,
    block_name: &mut Option<String>,
    block_props: &mut Vec<(String, String)>,
    ss: &mut Stylesheet,
) {
    let kind = match expecting.take() {
        Some(k) => k,
        None => return,
    };
    let props = std::mem::take(block_props);

    match kind {
        BlockKind::Tokens => {
            for (name, value) in props {
                let key = name.trim_start_matches('$');
                ss.tokens.insert(format!("${key}"), value);
            }
        }
        BlockKind::Theme => {
            let name = block_name.take();
            if let Some(theme_name) = name {
                let entry = ss.themes.entry(theme_name).or_default();
                for (n, v) in props {
                    let key = n.trim_start_matches('$');
                    entry.insert(format!("${key}"), v);
                }
            }
        }
        BlockKind::Rules => {
            let name = block_name.take();
            if let Some(class_name) = name {
                ss.rules.insert(class_name, props);
            }
        }
        BlockKind::FontFace => {
            let mut family = None;
            let mut src = None;
            for (k, v) in props {
                match k.as_str() {
                    "font-family" => family = Some(v.trim_matches(['"', '\'']).to_string()),
                    "src" => src = Some(extract_url(&v)),
                    _ => {}
                }
            }
            if let (Some(fam), Some(path)) = (family, src)
                && !fam.is_empty()
                && !path.is_empty()
            {
                ss.fonts.insert(fam, path);
            }
        }
    }
}

fn extract_url(value: &str) -> String {
    let v = value.trim();
    let v = v.strip_prefix("url(").unwrap_or(v).strip_suffix(')').unwrap_or(v);
    v.trim().trim_matches(['"', '\'']).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tokens() {
        let css = ":root {\n    $primary: #0066cc;\n    $spacing: 16px;\n}";
        let ss = parse(css).unwrap();
        assert_eq!(ss.tokens.get("$primary").unwrap(), "#0066cc");
        assert_eq!(ss.tokens.get("$spacing").unwrap(), "16px");
    }

    #[test]
    fn test_parse_theme() {
        let css = ":root { $primary: blue; }\ntheme dark {\n    $primary: navy;\n}";
        let ss = parse(css).unwrap();
        assert_eq!(ss.tokens.get("$primary").unwrap(), "blue");
        assert_eq!(ss.themes.get("dark").unwrap().get("$primary").unwrap(), "navy");
    }

    #[test]
    fn test_parse_rules() {
        let css = ".btn {\n    color: red;\n    padding: 16px;\n}";
        let ss = parse(css).unwrap();
        let props = ss.rules.get("btn").unwrap();
        assert_eq!(props[0], ("color".into(), "red".into()));
        assert_eq!(props[1], ("padding".into(), "16px".into()));
    }

    #[test]
    fn test_parse_inline() {
        let props = parse_inline("color: red; padding: 16px").unwrap();
        assert_eq!(props[0], ("color".into(), "red".into()));
        assert_eq!(props[1], ("padding".into(), "16px".into()));
    }

    #[test]
    fn test_parse_inline_trailing_semicolon() {
        let props = parse_inline("color: red;").unwrap();
        assert_eq!(props.len(), 1);
    }

    #[test]
    fn test_parse_full_stylesheet() {
        let css = r#"
:root {
    $primary: #0066cc;
    $radius: 4px;
}
theme dark {
    $primary: #003366;
}
.btn {
    color: $primary;
    padding: 16px;
    border-radius: $radius;
}
.heading {
    font-size: 24px;
    font-weight: bold;
}
"#;
        let ss = parse(css).unwrap();
        assert_eq!(ss.tokens.get("$primary").unwrap(), "#0066cc");
        assert_eq!(ss.themes.get("dark").unwrap().get("$primary").unwrap(), "#003366");
        assert_eq!(ss.rules.get("btn").unwrap().len(), 3);
        assert_eq!(ss.rules.get("heading").unwrap().len(), 2);
    }

    #[test]
    fn test_parse_font_face() {
        let css = r#"
@font-face {
    font-family: "Inter";
    src: url("./fonts/Inter.ttf");
}
"#;
        let ss = parse(css).unwrap();
        assert_eq!(ss.fonts.get("Inter").unwrap(), "./fonts/Inter.ttf");
    }

    #[test]
    fn test_parse_font_face_no_quotes() {
        let css = r#"@font-face { font-family: Verdana; src: url("fonts/Verdana.ttf"); }"#;
        let ss = parse(css).unwrap();
        assert_eq!(ss.fonts.get("Verdana").unwrap(), "fonts/Verdana.ttf");
    }

    #[test]
    fn test_parse_font_face_skips_invalid() {
        let css = r#"@font-face { src: url("./missing.ttf"); }"#;
        let ss = parse(css).unwrap();
        assert!(ss.fonts.is_empty());
    }
}
