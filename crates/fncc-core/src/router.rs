use anyhow::{Context, Result, bail};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Information about a single route discovered by scanning the routes directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteInfo {
    /// Relative path from the routes root directory (e.g., "users/[id].fui").
    pub relative_path: PathBuf,
    /// Absolute file path to the `.fui` file.
    pub file_path: PathBuf,
    /// Canonical URL pattern (e.g., "/", "/settings", "/users/:id").
    pub pattern: String,
    /// Generated PascalCase enum variant name (e.g., "Index", "Settings", "UsersId").
    pub variant_name: String,
    /// Dynamic parameter names extracted from path segments (e.g., ["id"]).
    pub params: Vec<String>,
    /// Relative paths to layout files wrapping this route, from outer to inner.
    pub layout_chain: Vec<PathBuf>,
    /// True if this is the application fallback screen (`fallback.fui`).
    pub is_fallback: bool,
}

impl RouteInfo {
    /// Compare priority for route resolution.
    /// Priority order (highest to lowest):
    /// 1. Positional static segments take precedence over dynamic segments.
    /// 2. Greater total segment count.
    pub fn cmp_priority(&self, other: &Self) -> Ordering {
        let self_segments: Vec<&str> = self.pattern.split('/').filter(|s| !s.is_empty()).collect();
        let other_segments: Vec<&str> = other.pattern.split('/').filter(|s| !s.is_empty()).collect();

        let min_len = self_segments.len().min(other_segments.len());
        for i in 0..min_len {
            let s_dyn = self_segments[i].starts_with(':');
            let o_dyn = other_segments[i].starts_with(':');

            match (s_dyn, o_dyn) {
                (false, true) => return Ordering::Less, // static wins over dynamic
                (true, false) => return Ordering::Greater,
                _ => {}
            }
        }

        // More specific segment count wins
        self_segments.len().cmp(&other_segments.len()).reverse()
    }
}

/// Scanned route tree containing all discovered routes and layout hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RouteTree {
    /// Discovered routes sorted by resolution priority.
    pub routes: Vec<RouteInfo>,
    /// Fallback route (`fallback.fui`), if present.
    pub fallback: Option<RouteInfo>,
}

impl RouteTree {
    /// Scan the `routes` directory (typically `src/ui/routes/` or `<ui_dir>/routes`)
    /// and build a validated `RouteTree`.
    pub fn scan(routes_dir: &Path) -> Result<Self> {
        if !routes_dir.exists() || !routes_dir.is_dir() {
            return Ok(RouteTree::default());
        }

        let mut tree = RouteTree::default();
        let layouts = Vec::new();
        let path_segments = Vec::new();
        let variant_parts = Vec::new();

        tree.scan_dir(routes_dir, routes_dir, &layouts, &path_segments, &variant_parts)?;

        tree.validate()?;
        tree.sort_by_priority();

        Ok(tree)
    }

    fn scan_dir(
        &mut self,
        current_dir: &Path,
        routes_root: &Path,
        parent_layouts: &[PathBuf],
        path_segments: &[PathSegment],
        variant_parts: &[String],
    ) -> Result<()> {
        let entries = std::fs::read_dir(current_dir)
            .with_context(|| format!("failed to read routes directory {:?}", current_dir))?;

        let mut files = Vec::new();
        let mut dirs = Vec::new();

        for entry in entries {
            let entry = entry.with_context(|| format!("failed to read entry in {:?}", current_dir))?;
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|ext| ext == "fui") {
                files.push(path);
            }
        }

        // Sort for deterministic traversal order
        dirs.sort();
        files.sort();

        // Check if current directory has a layout.fui
        let layout_file = current_dir.join("layout.fui");
        let has_local_layout = layout_file.is_file();

        let mut current_layouts = parent_layouts.to_vec();
        if has_local_layout {
            let is_stateful = if let Ok(source) = std::fs::read_to_string(&layout_file)
                && let Ok(ast) = crate::parser::parse(&source)
            {
                ast.state_type.is_some()
            } else {
                false
            };

            if !is_stateful {
                let rel_layout = layout_file
                    .strip_prefix(routes_root)
                    .unwrap_or(&layout_file)
                    .to_path_buf();
                current_layouts.push(rel_layout);
            }
        }

        // Process files in current directory
        for file in files {
            let file_name = file.file_name().unwrap_or_default().to_string_lossy();
            let stem = file.file_stem().unwrap_or_default().to_string_lossy();

            if file_name == "layout.fui" {
                continue; // Layouts are processed via current_layouts chain
            }

            let rel_path = file.strip_prefix(routes_root).unwrap_or(&file).to_path_buf();

            if file_name == "fallback.fui" {
                if let Some(existing) = &self.fallback {
                    bail!(
                        "multiple fallback routes found: '{:?}' and '{:?}'",
                        existing.file_path,
                        file
                    );
                }
                self.fallback = Some(RouteInfo {
                    relative_path: rel_path,
                    file_path: file,
                    pattern: "*".to_string(),
                    variant_name: "Fallback".to_string(),
                    params: Vec::new(),
                    layout_chain: current_layouts.clone(),
                    is_fallback: true,
                });
                continue;
            }

            // Route processing
            let mut file_path_segments = path_segments.to_vec();
            let mut file_variant_parts = variant_parts.to_vec();

            if stem == "index" {
                // Index route represents the directory itself
            } else {
                let parsed_segment = parse_segment(&stem, &file)?;
                match parsed_segment {
                    PathSegment::Static(name) => {
                        file_variant_parts.push(to_pascal_case(&name));
                        file_path_segments.push(PathSegment::Static(name));
                    }
                    PathSegment::Dynamic(param) => {
                        file_variant_parts.push(to_pascal_case(&param));
                        file_path_segments.push(PathSegment::Dynamic(param));
                    }
                    PathSegment::Group(group_name) => {
                        bail!(
                            "files cannot represent route groups: standalone route file '{:?}' has group syntax '({})'",
                            file,
                            group_name
                        );
                    }
                }
            }

            let pattern = build_pattern(&file_path_segments);
            let params = extract_params(&file_path_segments);
            let variant_name = if file_variant_parts.is_empty() {
                "Index".to_string()
            } else {
                file_variant_parts.join("")
            };

            self.routes.push(RouteInfo {
                relative_path: rel_path,
                file_path: file,
                pattern,
                variant_name,
                params,
                layout_chain: current_layouts.clone(),
                is_fallback: false,
            });
        }

        // Process subdirectories
        for dir in dirs {
            let dir_name = dir.file_name().unwrap_or_default().to_string_lossy();
            let parsed_segment = parse_segment(&dir_name, &dir)?;

            let mut sub_path_segments = path_segments.to_vec();
            let mut sub_variant_parts = variant_parts.to_vec();

            match parsed_segment {
                PathSegment::Static(name) => {
                    sub_variant_parts.push(to_pascal_case(&name));
                    sub_path_segments.push(PathSegment::Static(name));
                }
                PathSegment::Dynamic(param) => {
                    sub_variant_parts.push(to_pascal_case(&param));
                    sub_path_segments.push(PathSegment::Dynamic(param));
                }
                PathSegment::Group(_) => {
                    // Group directories are pathless — do not alter path or variant name
                }
            }

            self.scan_dir(
                &dir,
                routes_root,
                &current_layouts,
                &sub_path_segments,
                &sub_variant_parts,
            )?;
        }

        Ok(())
    }

    /// Perform compile-time validations on the discovered routes:
    /// - Duplicated route patterns
    /// - Duplicated enum variant names
    /// - Duplicate parameter names in a single route
    pub fn validate(&self) -> Result<()> {
        let mut patterns: HashMap<&str, &RouteInfo> = HashMap::new();
        let mut variants: HashMap<&str, &RouteInfo> = HashMap::new();

        if let Some(ref fb) = self.fallback {
            patterns.insert(fb.pattern.as_str(), fb);
            variants.insert(fb.variant_name.as_str(), fb);
        }

        for route in &self.routes {
            // Validate duplicate parameter names within single route
            let mut seen_params = std::collections::HashSet::new();
            for param in &route.params {
                if !seen_params.insert(param) {
                    bail!(
                        "duplicate dynamic parameter '{param}' in route '{}' ('{:?}')",
                        route.pattern,
                        route.file_path
                    );
                }
            }

            // Validate duplicate route pattern
            if let Some(existing) = patterns.get(route.pattern.as_str()) {
                bail!(
                    "duplicated route pattern '{}' found in '{:?}' and '{:?}'",
                    route.pattern,
                    existing.file_path,
                    route.file_path
                );
            }
            patterns.insert(&route.pattern, route);

            // Validate duplicate route variant name
            if let Some(existing) = variants.get(route.variant_name.as_str()) {
                bail!(
                    "duplicated route variant name '{}' found in '{:?}' and '{:?}'",
                    route.variant_name,
                    existing.file_path,
                    route.file_path
                );
            }
            variants.insert(&route.variant_name, route);
        }

        // Validate unique layout function names
        let mut layouts: std::collections::HashSet<&Path> = std::collections::HashSet::new();
        for route in &self.routes {
            for layout in &route.layout_chain {
                layouts.insert(layout.as_path());
            }
        }
        if let Some(ref fb) = self.fallback {
            for layout in &fb.layout_chain {
                layouts.insert(layout.as_path());
            }
        }

        let mut layout_names: HashMap<String, &Path> = HashMap::new();
        for layout in layouts {
            let fn_name = layout_fn_name(layout);
            if let Some(existing) = layout_names.get(&fn_name) {
                bail!(
                    "duplicate generated layout function name '{}' from distinct layouts '{:?}' and '{:?}'",
                    fn_name,
                    existing,
                    layout
                );
            }
            layout_names.insert(fn_name, layout);
        }

        Ok(())
    }

    /// Sort routes by priority: static routes first, then dynamic routes.
    pub fn sort_by_priority(&mut self) {
        self.routes.sort_by(|a, b| a.cmp_priority(b));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathSegment {
    Static(String),
    Dynamic(String),
    Group(String),
}

fn parse_segment(stem: &str, path: &Path) -> Result<PathSegment> {
    if stem.starts_with('(') && stem.ends_with(')') {
        let group_name = &stem[1..stem.len() - 1];
        if group_name.is_empty() {
            bail!("invalid route group syntax '()' in '{:?}'", path);
        }
        Ok(PathSegment::Group(group_name.to_string()))
    } else if stem.starts_with('[') || stem.ends_with(']') {
        if stem.starts_with('[') && stem.ends_with(']') {
            let param_name = &stem[1..stem.len() - 1];
            if !is_valid_param_name(param_name) {
                bail!(
                    "invalid dynamic parameter syntax '[{}]' in '{:?}': parameter name must be a valid non-empty identifier",
                    param_name,
                    path
                );
            }
            Ok(PathSegment::Dynamic(param_name.to_string()))
        } else {
            bail!("invalid dynamic parameter syntax '{}' in '{:?}'", stem, path);
        }
    } else {
        Ok(PathSegment::Static(stem.to_string()))
    }
}

fn is_valid_param_name(name: &str) -> bool {
    const RUST_KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self",
        "Self", "static", "struct", "super", "trait", "true", "type", "union", "unsafe", "use", "where", "while",
        "abstract", "become", "box", "do", "final", "macro", "override", "priv", "try", "typeof", "unsized", "virtual",
        "yield",
    ];

    if name.is_empty() || RUST_KEYWORDS.contains(&name) {
        return false;
    }
    let mut chars = name.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn build_pattern(segments: &[PathSegment]) -> String {
    let mut pattern = String::new();
    for seg in segments {
        match seg {
            PathSegment::Static(s) => {
                pattern.push('/');
                pattern.push_str(s);
            }
            PathSegment::Dynamic(d) => {
                pattern.push('/');
                pattern.push(':');
                pattern.push_str(d);
            }
            PathSegment::Group(_) => {}
        }
    }
    if pattern.is_empty() { "/".to_string() } else { pattern }
}

fn extract_params(segments: &[PathSegment]) -> Vec<String> {
    segments
        .iter()
        .filter_map(|seg| match seg {
            PathSegment::Dynamic(d) => Some(d.clone()),
            _ => None,
        })
        .collect()
}

/// Convert a string segment to PascalCase for enum variant names.
pub fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;

    for c in s.chars() {
        if c == '_' || c == '-' || c == ' ' {
            capitalize_next = true;
        } else if c.is_ascii_alphanumeric() {
            if capitalize_next {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(c);
            }
        }
    }

    if result.is_empty() { "Route".to_string() } else { result }
}

fn validate_layout_outlet_count(layout_abs: &Path) -> Result<(), String> {
    let source = std::fs::read_to_string(layout_abs).map_err(|e| format!("failed to read layout file: {e}"))?;
    let doc = crate::parser::parse(&source)?;

    fn count_outlets(el: &crate::parser::Element) -> usize {
        let mut count = if el.name == "RouterOutlet" { 1 } else { 0 };
        for child in &el.children {
            if let crate::parser::Node::Element(child_el) = child {
                count += count_outlets(child_el);
            }
        }
        count
    }

    let count = count_outlets(&doc.root);
    if count != 1 {
        return Err(format!(
            "layout file must contain exactly one <RouterOutlet /> (found {})",
            count
        ));
    }
    Ok(())
}

/// Generate Rust code for the compile-time router:
/// - `pub enum Route` with variants for all screens and fallback
/// - `Route::from_uri(&str) -> Option<Route>` for deep link parsing
/// - `Route::path(&self) -> String` for path serialization
/// - `Route::render(&self) -> impl IntoElement` for rendering screen + nested layouts
/// - `pub fn render_router_outlet(route: &Route) -> impl IntoElement`
pub fn generate_router_code(tree: &RouteTree) -> String {
    let mut out = String::new();

    out.push_str("/// Generated Route enum representing all application screens.\n");
    out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
    out.push_str("pub enum Route {\n");

    for route in &tree.routes {
        out.push_str(&format!("    /// Pattern: {}\n", route.pattern));
        if route.params.is_empty() {
            out.push_str(&format!("    {},\n", route.variant_name));
        } else {
            out.push_str(&format!("    {} {{\n", route.variant_name));
            for param in &route.params {
                out.push_str(&format!("        {}: String,\n", param));
            }
            out.push_str("    },\n");
        }
    }

    if let Some(fb) = &tree.fallback {
        out.push_str("    /// Fallback route\n");
        out.push_str(&format!("    {},\n", fb.variant_name));
    }

    out.push_str("}\n\n");

    // impl Route
    out.push_str("impl Route {\n");
    out.push_str("    /// Parse a deep link URI or path into a Route.\n");
    out.push_str("    pub fn from_uri(uri: &str) -> Option<Self> {\n");
    out.push_str("        let path = if let Some(pos) = uri.find(\"://\") {\n");
    out.push_str("            &uri[pos + 3..]\n");
    out.push_str("        } else {\n");
    out.push_str("            uri\n");
    out.push_str("        };\n");
    out.push_str("        let path = path.split('?').next().unwrap_or(path).split('#').next().unwrap_or(path);\n");
    out.push_str("        let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();\n\n");

    for route in &tree.routes {
        let pattern_segs: Vec<&str> = route.pattern.split('/').filter(|s| !s.is_empty()).collect();
        if pattern_segs.is_empty() {
            out.push_str("        if segs.is_empty() {\n");
            out.push_str(&format!("            return Some(Route::{});\n", route.variant_name));
            out.push_str("        }\n");
        } else {
            let mut pattern_match = String::new();
            pattern_match.push('[');
            for (i, seg) in pattern_segs.iter().enumerate() {
                if i > 0 {
                    pattern_match.push_str(", ");
                }
                if let Some(param) = seg.strip_prefix(':') {
                    pattern_match.push_str(param);
                } else {
                    pattern_match.push_str(&format!("{:?}", seg));
                }
            }
            pattern_match.push(']');

            out.push_str(&format!("        if let {} = segs.as_slice() {{\n", pattern_match));
            if route.params.is_empty() {
                out.push_str(&format!("            return Some(Route::{});\n", route.variant_name));
            } else {
                out.push_str(&format!("            return Some(Route::{} {{\n", route.variant_name));
                for param in &route.params {
                    out.push_str(&format!("                {}: (*{}).to_string(),\n", param, param));
                }
                out.push_str("            });\n");
            }
            out.push_str("        }\n");
        }
    }

    if tree.fallback.is_some() {
        out.push_str("        Some(Route::Fallback)\n");
    } else {
        out.push_str("        None\n");
    }

    out.push_str("    }\n\n");

    // pub fn path(&self) -> String
    out.push_str("    /// Get canonical path for this route.\n");
    out.push_str("    pub fn path(&self) -> String {\n");
    out.push_str("        match self {\n");
    for route in &tree.routes {
        if route.params.is_empty() {
            out.push_str(&format!(
                "            Route::{} => {:?}.to_string(),\n",
                route.variant_name, route.pattern
            ));
        } else {
            let mut fmt_str = route.pattern.clone();
            let mut args = String::new();
            for param in &route.params {
                fmt_str = fmt_str.replace(&format!(":{}", param), &format!("{{{}}}", param));
                args.push_str(&format!(", {} = {}", param, param));
            }
            out.push_str(&format!(
                "            Route::{} {{ {} }} => format!({:?}{}),\n",
                route.variant_name,
                route.params.join(", "),
                fmt_str,
                args
            ));
        }
    }
    if let Some(fb) = &tree.fallback {
        out.push_str(&format!(
            "            Route::{} => \"*\".to_string(),\n",
            fb.variant_name
        ));
    }
    out.push_str("        }\n");
    out.push_str("    }\n\n");

    // pub fn render(&self) -> AnyElement
    out.push_str("    /// Render the view hierarchy corresponding to this route.\n");
    out.push_str("    pub fn render(&self) -> AnyElement {\n");
    out.push_str("        match self {\n");

    for route in &tree.routes {
        let target_fn = format!("render_{}", crate::codegen::to_snake_case(&route.variant_name));

        let mut target_call = if route.params.is_empty() {
            format!("{}()", target_fn)
        } else {
            let param_args: Vec<String> = route.params.iter().map(|p| format!("{}.as_str()", p)).collect();
            format!("{}({})", target_fn, param_args.join(", "))
        };

        // Wrap target_call in layout_chain from innermost to outermost
        for layout_rel in route.layout_chain.iter().rev() {
            let mut routes_root = route.file_path.clone();
            for _ in route.relative_path.components() {
                routes_root.pop();
            }
            let layout_abs = routes_root.join(layout_rel);
            if let Err(err) = validate_layout_outlet_count(&layout_abs) {
                panic!(
                    "route-level diagnostic: layout file '{}' is invalid: {}",
                    layout_rel.display(),
                    err
                );
            }
            let layout_fn = layout_fn_name(layout_rel);
            target_call = format!("{}({})", layout_fn, target_call);
        }

        if route.params.is_empty() {
            out.push_str(&format!(
                "            Route::{} => {}.into_any_element(),\n",
                route.variant_name, target_call
            ));
        } else {
            out.push_str(&format!(
                "            Route::{} {{ {} }} => {}.into_any_element(),\n",
                route.variant_name,
                route.params.join(", "),
                target_call
            ));
        }
    }

    if let Some(fb) = &tree.fallback {
        let mut target_call = "render_fallback()".to_string();
        for layout_rel in fb.layout_chain.iter().rev() {
            let mut routes_root = fb.file_path.clone();
            for _ in fb.relative_path.components() {
                routes_root.pop();
            }
            let layout_abs = routes_root.join(layout_rel);
            if let Err(err) = validate_layout_outlet_count(&layout_abs) {
                panic!(
                    "route-level diagnostic: layout file '{}' is invalid: {}",
                    layout_rel.display(),
                    err
                );
            }
            let layout_fn = layout_fn_name(layout_rel);
            target_call = format!("{}({})", layout_fn, target_call);
        }
        out.push_str(&format!(
            "            Route::{} => {}.into_any_element(),\n",
            fb.variant_name, target_call
        ));
    }

    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");

    out.push_str("/// Render the active route outlet.\n");
    out.push_str("pub fn render_router_outlet(route: &Route) -> AnyElement {\n");
    out.push_str("    route.render()\n");
    out.push_str("}\n\n");

    out
}

fn layout_fn_name(layout_rel: &Path) -> String {
    let parent = layout_rel.parent().unwrap_or_else(|| Path::new(""));
    let parent_str = parent.to_string_lossy();
    if parent_str.is_empty() {
        "render_layout".to_string()
    } else {
        let pascal = to_pascal_case(&parent_str.replace('/', "_"));
        format!("render_{}_layout", crate::codegen::to_snake_case(&pascal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn test_routes_dir() -> (PathBuf, PathBuf) {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let root = std::env::temp_dir().join(format!("fncc_router_test_{pid}_{id}"));
        let routes = root.join("routes");
        std::fs::create_dir_all(&routes).unwrap();
        (root, routes)
    }

    #[test]
    fn test_scan_basic_routes() {
        let (root, routes) = test_routes_dir();
        std::fs::write(routes.join("index.fui"), "<Text>Home</Text>").unwrap();
        std::fs::write(routes.join("settings.fui"), "<Text>Settings</Text>").unwrap();

        let tree = RouteTree::scan(&routes).unwrap();

        assert_eq!(tree.routes.len(), 2);
        let home = tree.routes.iter().find(|r| r.pattern == "/").unwrap();
        assert_eq!(home.variant_name, "Index");

        let settings = tree.routes.iter().find(|r| r.pattern == "/settings").unwrap();
        assert_eq!(settings.variant_name, "Settings");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn test_scan_dynamic_and_groups() {
        let (root, routes) = test_routes_dir();

        // (app)/dashboard.fui
        let app_dir = routes.join("(app)");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::write(app_dir.join("dashboard.fui"), "<Text>Dashboard</Text>").unwrap();

        // users/[id].fui
        let users_dir = routes.join("users");
        std::fs::create_dir_all(&users_dir).unwrap();
        std::fs::write(users_dir.join("[id].fui"), "<Text>User</Text>").unwrap();

        let tree = RouteTree::scan(&routes).unwrap();

        let dash = tree.routes.iter().find(|r| r.pattern == "/dashboard").unwrap();
        assert_eq!(dash.variant_name, "Dashboard");

        let user = tree.routes.iter().find(|r| r.pattern == "/users/:id").unwrap();
        assert_eq!(user.variant_name, "UsersId");
        assert_eq!(user.params, vec!["id"]);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn test_scan_layouts() {
        let (root, routes) = test_routes_dir();

        std::fs::write(routes.join("layout.fui"), "<RootLayout />").unwrap();
        std::fs::write(routes.join("index.fui"), "<Home />").unwrap();

        let dash_dir = routes.join("dashboard");
        std::fs::create_dir_all(&dash_dir).unwrap();
        std::fs::write(dash_dir.join("layout.fui"), "<DashLayout />").unwrap();
        std::fs::write(dash_dir.join("index.fui"), "<Dash />").unwrap();

        let tree = RouteTree::scan(&routes).unwrap();

        let home = tree.routes.iter().find(|r| r.pattern == "/").unwrap();
        assert_eq!(home.layout_chain, vec![PathBuf::from("layout.fui")]);

        let dash = tree.routes.iter().find(|r| r.pattern == "/dashboard").unwrap();
        assert_eq!(
            dash.layout_chain,
            vec![PathBuf::from("layout.fui"), PathBuf::from("dashboard/layout.fui")]
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn test_fallback_route() {
        let (root, routes) = test_routes_dir();

        std::fs::write(routes.join("fallback.fui"), "<NotFound />").unwrap();

        let tree = RouteTree::scan(&routes).unwrap();

        assert!(tree.fallback.is_some());
        let fb = tree.fallback.unwrap();
        assert_eq!(fb.variant_name, "Fallback");
        assert_eq!(fb.pattern, "*");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn test_validation_duplicate_pattern_errors() {
        let (root, routes) = test_routes_dir();

        std::fs::write(routes.join("users.fui"), "<Users />").unwrap();
        let users_dir = routes.join("users");
        std::fs::create_dir_all(&users_dir).unwrap();
        std::fs::write(users_dir.join("index.fui"), "<UsersIndex />").unwrap();

        let res = RouteTree::scan(&routes);
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("duplicated route pattern '/users'"));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn test_validation_invalid_param_syntax_errors() {
        let (root, routes) = test_routes_dir();

        let users_dir = routes.join("users");
        std::fs::create_dir_all(&users_dir).unwrap();
        std::fs::write(users_dir.join("[123].fui"), "<Invalid />").unwrap();

        let res = RouteTree::scan(&routes);
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("invalid dynamic parameter syntax"));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn test_generate_router_code() {
        let (root, routes) = test_routes_dir();

        std::fs::write(routes.join("layout.fui"), "<RootLayout><RouterOutlet /></RootLayout>").unwrap();
        std::fs::write(routes.join("index.fui"), "<Home />").unwrap();
        std::fs::write(routes.join("settings.fui"), "<Settings />").unwrap();

        let users_dir = routes.join("users");
        std::fs::create_dir_all(&users_dir).unwrap();
        std::fs::write(users_dir.join("[id].fui"), "<UserDetail />").unwrap();

        std::fs::write(routes.join("fallback.fui"), "<NotFound />").unwrap();

        let tree = RouteTree::scan(&routes).unwrap();
        let code = generate_router_code(&tree);

        assert!(code.contains("pub enum Route"));
        assert!(code.contains("Index,"));
        assert!(code.contains("Settings,"));
        assert!(code.contains("UsersId {"));
        assert!(code.contains("id: String,"));
        assert!(code.contains("Fallback,"));

        assert!(code.contains("pub fn from_uri(uri: &str) -> Option<Self>"));
        assert!(code.contains("if let [\"settings\"] = segs.as_slice()"));
        assert!(code.contains("if let [\"users\", id] = segs.as_slice()"));

        assert!(code.contains("pub fn path(&self) -> String"));
        assert!(code.contains("pub fn render(&self) -> AnyElement"));
        assert!(code.contains("pub fn render_router_outlet(route: &Route) -> AnyElement"));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn test_scan_group_file_error() {
        let (root, routes) = test_routes_dir();

        std::fs::write(routes.join("(app).fui"), "<Invalid />").unwrap();

        let res = RouteTree::scan(&routes);
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("files cannot represent route groups"));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn test_route_priority_static_over_dynamic() {
        let (root, routes) = test_routes_dir();

        let users_dir = routes.join("users");
        std::fs::create_dir_all(&users_dir).unwrap();
        std::fs::write(users_dir.join("settings.fui"), "<Text>Settings</Text>").unwrap();
        std::fs::write(users_dir.join("[id].fui"), "<Text>UserDetail</Text>").unwrap();

        let tree = RouteTree::scan(&routes).unwrap();

        assert_eq!(tree.routes.len(), 2);
        assert_eq!(tree.routes[0].pattern, "/users/settings");
        assert_eq!(tree.routes[1].pattern, "/users/:id");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    #[should_panic]
    fn test_layout_validation_duplicate_outlets() {
        let (root, routes) = test_routes_dir();

        std::fs::write(
            routes.join("layout.fui"),
            "<Stack><RouterOutlet /><RouterOutlet /></Stack>",
        )
        .unwrap();
        std::fs::write(routes.join("index.fui"), "<Home />").unwrap();

        let tree = RouteTree::scan(&routes).unwrap();
        let _code = generate_router_code(&tree);

        std::fs::remove_dir_all(&root).unwrap();
    }
}
