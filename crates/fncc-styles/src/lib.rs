pub mod css_parser;
pub mod gpui_map;

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct GPUIMethodCall {
    pub code: String,
}

#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    pub tokens: HashMap<String, String>,
    pub themes: HashMap<String, HashMap<String, String>>,
    pub rules: HashMap<String, Vec<(String, String)>>,
    pub inline_props: Vec<(String, String)>,
    /// Custom fonts declared via `@font-face`: font-family name → file path.
    pub fonts: HashMap<String, String>,
}

pub fn resolve(
    classes: &[String],
    inline_style: Option<&str>,
    cascade: &Stylesheet,
    theme: Option<&str>,
) -> Result<Vec<GPUIMethodCall>, String> {
    let mut merged = HashMap::new();

    let active_tokens = resolve_tokens(cascade, theme);

    for class_name in classes {
        if let Some(props) = cascade.rules.get(class_name) {
            for (name, raw_value) in props {
                let resolved = resolve_value(raw_value, &active_tokens);
                merged.entry(name.clone()).or_insert_with(Vec::new).push(resolved);
            }
        }
    }

    if let Some(inline) = inline_style {
        let props = css_parser::parse_inline(inline)?;
        for (name, value) in props {
            let resolved = resolve_value(&value, &active_tokens);
            merged.entry(name).or_insert_with(Vec::new).push(resolved);
        }
    }

    let mut calls = Vec::new();
    for (name, values) in &merged {
        let last = values.last().expect("non-empty");
        let mapped = gpui_map::map(name, last)?;
        calls.extend(mapped);
    }

    Ok(calls)
}

fn resolve_tokens(stylesheet: &Stylesheet, theme: Option<&str>) -> HashMap<String, String> {
    let mut tokens = stylesheet.tokens.clone();
    if let Some(theme_name) = theme
        && let Some(theme_tokens) = stylesheet.themes.get(theme_name)
    {
        for (k, v) in theme_tokens {
            tokens.insert(k.clone(), v.clone());
        }
    }
    tokens
}

fn resolve_value(raw: &str, tokens: &HashMap<String, String>) -> String {
    let mut result = raw.to_string();
    for (token, value) in tokens {
        let t = format!("${}", token.trim_start_matches('$'));
        result = result.replace(&t, value);
    }
    result
}

pub fn merge(stylesheets: Vec<Stylesheet>) -> Stylesheet {
    let mut out = Stylesheet::default();
    for ss in stylesheets {
        out.tokens.extend(ss.tokens);
        out.themes.extend(ss.themes);
        for (class, props) in ss.rules {
            let entry = out.rules.entry(class).or_default();
            for (name, value) in props {
                if let Some(pos) = entry.iter().position(|(n, _)| n == &name) {
                    entry[pos] = (name, value);
                } else {
                    entry.push((name, value));
                }
            }
        }
        out.inline_props.extend(ss.inline_props);
        out.fonts.extend(ss.fonts);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_inline_style_with_token() {
        let mut ss = Stylesheet::default();
        ss.tokens.insert("$accent".into(), "#ff6600".into());
        let calls = resolve(&[], Some("background: $accent; color: white"), &ss, None).unwrap();
        assert!(calls.iter().any(|c| c.code == "bg(rgba(0xff6600ff))"));
        assert!(calls.iter().any(|c| c.code == "text_color(rgba(0xffffffff))"));
    }
}
