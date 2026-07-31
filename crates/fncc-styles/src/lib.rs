pub mod css_parser;
pub mod gpui_map;

use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone)]
pub struct GPUIMethodCall {
    pub code: String,
}

#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    pub tokens: HashMap<String, String>,
    pub themes: HashMap<String, HashMap<String, String>>,
    pub rules: HashMap<String, Vec<(String, String)>>,
    /// Custom fonts declared via `@font-face`: font-family name → file path.
    pub fonts: HashMap<String, String>,
}

pub fn resolve(
    classes: &[String],
    inline_style: Option<&str>,
    cascade: &Stylesheet,
    theme: Option<&str>,
) -> Result<Vec<GPUIMethodCall>, String> {
    let mut merged: BTreeMap<String, Vec<String>> = BTreeMap::new();

    let active_tokens = resolve_tokens(cascade, theme);

    for class_name in classes {
        if let Some(props) = cascade.rules.get(class_name) {
            for (name, raw_value) in props {
                let resolved = resolve_value(raw_value, &active_tokens);
                merged.entry(name.clone()).or_default().push(resolved);
            }
        }
    }

    if let Some(inline) = inline_style {
        let props = css_parser::parse_inline(inline)?;
        for (name, value) in props {
            let resolved = resolve_value(&value, &active_tokens);
            merged.entry(name).or_default().push(resolved);
        }
    }

    let mut calls = Vec::new();
    for (name, values) in &merged {
        let last = values.last().expect("non-empty");
        if last.contains('$') {
            return Err(format!("unresolved token in `{name}`: `{last}`"));
        }
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
    // Longest names first so `$accent_dark` is substituted before `$accent`.
    let mut names: Vec<&String> = tokens.keys().collect();
    names.sort_by_key(|b| std::cmp::Reverse(b.len()));
    for token in names {
        result = result.replace(token, &tokens[token]);
    }
    result
}

pub fn merge(stylesheets: Vec<Stylesheet>) -> Stylesheet {
    let mut out = Stylesheet::default();
    for ss in stylesheets {
        out.tokens.extend(ss.tokens);
        for (theme_name, tokens) in ss.themes {
            let entry = out.themes.entry(theme_name).or_default();
            entry.extend(tokens);
        }
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

    #[test]
    fn test_resolve_unresolved_token_errors() {
        let ss = Stylesheet::default();
        let result = resolve(&[], Some("background: $missing;"), &ss, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("$missing"));
    }

    #[test]
    fn test_resolve_longer_token_preferred() {
        let mut ss = Stylesheet::default();
        ss.tokens.insert("$accent".into(), "#ff0000".into());
        ss.tokens.insert("$accent_dark".into(), "#990000".into());
        let calls = resolve(&[], Some("background: $accent_dark;"), &ss, None).unwrap();
        assert_eq!(calls[0].code, "bg(rgba(0x990000ff))");
    }
}
