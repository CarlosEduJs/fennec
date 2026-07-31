use crate::GPUIMethodCall;

pub fn map(name: &str, value: &str) -> Result<Vec<GPUIMethodCall>, String> {
    match name {
        "display" => match value {
            "flex" => Ok(vec![GPUIMethodCall { code: "flex()".into() }]),
            "grid" => Ok(vec![GPUIMethodCall { code: "grid()".into() }]),
            other => Err(format!("unsupported display value: `{other}`")),
        },
        "flex-direction" => match value {
            "column" | "vertical" => Ok(vec![GPUIMethodCall {
                code: "flex_col()".into(),
            }]),
            "row" | "horizontal" => Ok(vec![]),
            other => Err(format!("unsupported flex-direction: `{other}`")),
        },
        "flex" => Ok(vec![GPUIMethodCall {
            code: format!("flex_{value}"),
        }]),
        "align-items" | "align_items" => match value {
            "flex-start" | "start" => Ok(vec![GPUIMethodCall { code: "items_start()".into() }]),
            "flex-end" | "end" => Ok(vec![GPUIMethodCall { code: "items_end()".into() }]),
            "center" => Ok(vec![GPUIMethodCall { code: "items_center()".into() }]),
            "baseline" => Ok(vec![GPUIMethodCall { code: "items_baseline()".into() }]),
            "stretch" => Ok(vec![]),
            other => Err(format!("unsupported align-items: `{other}`")),
        },
        "justify-content" | "justify_content" => match value {
            "flex-start" | "start" => Ok(vec![GPUIMethodCall { code: "justify_start()".into() }]),
            "flex-end" | "end" => Ok(vec![GPUIMethodCall { code: "justify_end()".into() }]),
            "center" => Ok(vec![GPUIMethodCall { code: "justify_center()".into() }]),
            "space-between" => Ok(vec![GPUIMethodCall { code: "justify_between()".into() }]),
            "space-around" => Ok(vec![GPUIMethodCall { code: "justify_around()".into() }]),
            other => Err(format!("unsupported justify-content: `{other}`")),
        },
        "gap" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("gap({c})") }])
        }
        "padding" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("p({c})") }])
        }
        "padding-left" | "pl" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("pl({c})") }])
        }
        "padding-right" | "pr" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("pr({c})") }])
        }
        "padding-top" | "pt" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("pt({c})") }])
        }
        "padding-bottom" | "pb" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("pb({c})") }])
        }
        "margin" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("m({c})") }])
        }
        "width" | "w" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("w({c})") }])
        }
        "height" | "h" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("h({c})") }])
        }
        "min-width" | "min_w" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("min_w({c})") }])
        }
        "max-width" | "max_w" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("max_w({c})") }])
        }
        "min-height" | "min_h" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("min_h({c})") }])
        }
        "max-height" | "max_h" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("max_h({c})") }])
        }
        "background" | "bg" => {
            let c = parse_color(value)?;
            Ok(vec![GPUIMethodCall {
                code: format!("bg({c})"),
            }])
        }
        "color" => {
            let c = parse_color(value)?;
            Ok(vec![GPUIMethodCall {
                code: format!("text_color({c})"),
            }])
        }
        "border-radius" | "rounded" => {
            let c = format_dim(value)?;
            Ok(vec![GPUIMethodCall { code: format!("rounded({c})") }])
        }
        "border-width" | "border_width" => {
            let n = parse_dim(value)?;
            Ok(vec![GPUIMethodCall {
                code: format!("border(px({n}.))"),
            }])
        }
        "border-color" | "border_color" => {
            let c = parse_color(value)?;
            Ok(vec![GPUIMethodCall {
                code: format!("border_color({c})"),
            }])
        }
        "border" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            let mut calls = Vec::new();
            let mut i = 0;
            while i < parts.len() {
                if let Ok(n) = parse_dim(parts[i]) {
                    calls.push(GPUIMethodCall {
                        code: format!("border(px({n}.))"),
                    });
                    i += 1;
                } else if parse_color(parts[i]).is_ok() {
                    if let Ok(c) = parse_color(parts[i]) {
                        calls.push(GPUIMethodCall {
                            code: format!("border_color({c})"),
                        });
                    }
                    i += 1;
                } else if parts[i] == "solid" || parts[i] == "dashed" || parts[i] == "dotted" {
                    i += 1;
                } else {
                    return Err(format!("unexpected border value part: `{}`", parts[i]));
                }
            }
            Ok(calls)
        }
        "opacity" => {
            let n = value
                .parse::<f64>()
                .map_err(|_| format!("invalid opacity: `{value}`"))?;
            Ok(vec![GPUIMethodCall {
                code: format!("opacity({n})"),
            }])
        }
        "font-size" => {
            let n = parse_dim(value)?;
            let call = match n as i32 {
                12 => GPUIMethodCall {
                    code: "text_xs()".into(),
                },
                14 => GPUIMethodCall {
                    code: "text_sm()".into(),
                },
                16 => GPUIMethodCall {
                    code: "text_base()".into(),
                },
                18 => GPUIMethodCall {
                    code: "text_lg()".into(),
                },
                20 => GPUIMethodCall {
                    code: "text_xl()".into(),
                },
                24 => GPUIMethodCall {
                    code: "text_2xl()".into(),
                },
                30 | 32 => GPUIMethodCall {
                    code: "text_3xl()".into(),
                },
                _ => GPUIMethodCall {
                    code: format!("text_size(px({n}.))"),
                },
            };
            Ok(vec![call])
        }
        "font-weight" | "font_weight" => match value {
            "bold" | "700" => Ok(vec![GPUIMethodCall {
                code: "font_weight(FontWeight::BOLD)".into(),
            }]),
            "normal" | "400" => Ok(vec![]),
            other => Err(format!("unsupported font-weight: `{other}`")),
        },
        "text-align" | "text_align" => match value {
            "center" => Ok(vec![GPUIMethodCall {
                code: "text_center()".into(),
            }]),
            "left" | "right" => Ok(vec![]),
            other => Err(format!("unsupported text-align: `{other}`")),
        },
        "overflow" => match value {
            "hidden" => Ok(vec![GPUIMethodCall {
                code: "overflow_hidden()".into(),
            }]),
            "visible" | "auto" => Ok(vec![]),
            other => Err(format!("unsupported overflow: `{other}`")),
        },
        "position" => match value {
            "relative" => Ok(vec![GPUIMethodCall {
                code: "relative()".into(),
            }]),
            "absolute" => Ok(vec![GPUIMethodCall {
                code: "absolute()".into(),
            }]),
            other => Err(format!("unsupported position: `{other}`")),
        },
        "cursor" => match value {
            "pointer" => Ok(vec![GPUIMethodCall {
                code: "cursor_pointer()".into(),
            }]),
            "default" | "text" => Ok(vec![]),
            other => Err(format!("unsupported cursor: `{other}`")),
        },
        _ => Err(format!(
            "unsupported or unknown CSS property: `{name}` (value: `{value}`)"
        )),
    }
}

fn parse_dim(value: &str) -> Result<f64, String> {
    let s = value.trim();
    if s.is_empty() {
        return Err("empty dimension value".into());
    }
    let num_str = s
        .strip_suffix("px")
        .or(s.strip_suffix("rem"))
        .or(s.strip_suffix("em"))
        .or(s.strip_suffix("%"))
        .unwrap_or(s);
    num_str.parse::<f64>().map_err(|_| format!("invalid dimension: `{s}`"))
}

fn format_dim(value: &str) -> Result<String, String> {
    let s = value.trim();
    if s.ends_with('%') {
        let num_str = s.strip_suffix('%').unwrap_or(s).trim();
        let n: f64 = num_str.parse().map_err(|_| format!("invalid percentage: `{s}`"))?;
        let rel = n / 100.0;
        // Ensure float literal for GPUI's `relative(f32)`
        if rel.fract() == 0.0 {
            Ok(format!("relative({rel:.1})"))
        } else {
            Ok(format!("relative({rel})"))
        }
    } else {
        let n = parse_dim(s)?;
        Ok(format!("px({n}.)"))
    }
}

fn parse_color(value: &str) -> Result<String, String> {
    let s = value.trim();
    if let Some(c) = try_parse_color(s) {
        return Ok(c);
    }
    if let Some(hex) = s.strip_prefix('#') {
        let hex = hex.trim();
        let rgb = if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16);
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16);
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16);
            (r, g, b, Ok(0xff))
        } else if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16);
            let g = u8::from_str_radix(&hex[2..4], 16);
            let b = u8::from_str_radix(&hex[4..6], 16);
            (r, g, b, Ok(0xff))
        } else if hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16);
            let g = u8::from_str_radix(&hex[2..4], 16);
            let b = u8::from_str_radix(&hex[4..6], 16);
            let a = u8::from_str_radix(&hex[6..8], 16);
            (r, g, b, a)
        } else {
            return Err(format!("invalid hex color: `{s}`"));
        };
        match (rgb.0, rgb.1, rgb.2, rgb.3) {
            (Ok(r), Ok(g), Ok(b), Ok(a)) => {
                let hex_val: u32 = (r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | a as u32;
                Ok(format!("rgba({:#x})", hex_val))
            }
            _ => Err(format!("invalid hex color: `{s}`")),
        }
    } else if s.starts_with("rgb(") && s.ends_with(')') {
        let inner = s[4..s.len() - 1].trim();
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() != 3 {
            return Err(format!("invalid rgb(): `{s}`"));
        }
        let r: u8 = parts[0].parse().map_err(|_| format!("invalid rgb value: {s}"))?;
        let g: u8 = parts[1].parse().map_err(|_| format!("invalid rgb value: {s}"))?;
        let b: u8 = parts[2].parse().map_err(|_| format!("invalid rgb value: {s}"))?;
        let hex_val: u32 = (r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | 0xff;
        Ok(format!("rgba({:#x})", hex_val))
    } else {
        Err(format!("invalid color value: `{s}`"))
    }
}

fn try_parse_color(s: &str) -> Option<String> {
    let name = s.to_lowercase();
    let hex = match name.as_str() {
        "white" => "0xffffffff",
        "black" => "0x000000ff",
        "red" => "0xff0000ff",
        "green" => "0x00ff00ff",
        "blue" => "0x0000ffff",
        "yellow" => "0xffff00ff",
        "orange" => "0xffa500ff",
        "purple" => "0x800080ff",
        "pink" => "0xffc0cbff",
        "grey" | "gray" => "0x808080ff",
        "transparent" => "0x00000000",
        _ => return None,
    };
    Some(format!("rgba({hex})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_display_flex() {
        let calls = map("display", "flex").unwrap();
        assert_eq!(calls[0].code, "flex()");
    }

    #[test]
    fn test_map_padding() {
        let calls = map("padding", "16px").unwrap();
        assert_eq!(calls[0].code, "p(px(16.))");
    }

    #[test]
    fn test_map_padding_percent() {
        let calls = map("padding", "50%").unwrap();
        assert_eq!(calls[0].code, "p(relative(0.5))");
    }

    #[test]
    fn test_map_padding_percent_100() {
        let calls = map("padding", "100%").unwrap();
        assert_eq!(calls[0].code, "p(relative(1.0))");
    }

    #[test]
    fn test_map_gap() {
        let calls = map("gap", "12px").unwrap();
        assert_eq!(calls[0].code, "gap(px(12.))");
    }

    #[test]
    fn test_map_width_percent() {
        let calls = map("width", "100%").unwrap();
        assert_eq!(calls[0].code, "w(relative(1.0))");
    }

    #[test]
    fn test_map_color_hex() {
        let calls = map("color", "#0066cc").unwrap();
        assert_eq!(calls[0].code, "text_color(rgba(0x66ccff))");
    }

    #[test]
    fn test_map_color_hex_short() {
        let calls = map("bg", "#fff").unwrap();
        assert_eq!(calls[0].code, "bg(rgba(0xffffffff))");
    }

    #[test]
    fn test_map_color_named() {
        let calls = map("background", "white").unwrap();
        assert_eq!(calls[0].code, "bg(rgba(0xffffffff))");
    }

    #[test]
    fn test_map_border_compound() {
        let calls = map("border", "1px solid #ccc").unwrap();
        assert_eq!(calls[0].code, "border(px(1.))");
        assert_eq!(calls[1].code, "border_color(rgba(0xccccccff))");
    }

    #[test]
    fn test_map_font_size() {
        let calls = map("font-size", "24px").unwrap();
        assert_eq!(calls[0].code, "text_2xl()");
    }

    #[test]
    fn test_map_opacity() {
        let calls = map("opacity", "0.5").unwrap();
        assert_eq!(calls[0].code, "opacity(0.5)");
    }

    #[test]
    fn test_map_unknown_property_errors() {
        let result = map("animation", "fade");
        assert!(result.is_err());
    }

    #[test]
    fn test_map_flex_column() {
        let calls = map("flex-direction", "column").unwrap();
        assert_eq!(calls[0].code, "flex_col()");
    }

    #[test]
    fn test_map_position_absolute() {
        let calls = map("position", "absolute").unwrap();
        assert_eq!(calls[0].code, "absolute()");
    }

    #[test]
    fn test_map_cursor_pointer() {
        let calls = map("cursor", "pointer").unwrap();
        assert_eq!(calls[0].code, "cursor_pointer()");
    }

    #[test]
    fn test_map_align_items_center() {
        let calls = map("align-items", "center").unwrap();
        assert_eq!(calls[0].code, "items_center()");
    }

    #[test]
    fn test_map_align_items_start() {
        let calls = map("align-items", "flex-start").unwrap();
        assert_eq!(calls[0].code, "items_start()");
    }

    #[test]
    fn test_map_align_items_stretch() {
        let calls = map("align-items", "stretch").unwrap();
        assert!(calls.is_empty());
    }

    #[test]
    fn test_map_justify_content_center() {
        let calls = map("justify-content", "center").unwrap();
        assert_eq!(calls[0].code, "justify_center()");
    }

    #[test]
    fn test_map_justify_content_between() {
        let calls = map("justify-content", "space-between").unwrap();
        assert_eq!(calls[0].code, "justify_between()");
    }

    #[test]
    fn test_map_justify_content_invalid() {
        let result = map("justify-content", "space-evenly");
        assert!(result.is_err());
    }
}
