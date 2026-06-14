use lsp_types::{CallHierarchyItem, SymbolInformation};
use std::path::Path;

/// Metadata about a function extracted from LSP server responses.
#[derive(Debug, Clone)]
pub(crate) struct FunctionMeta {
    /// Fully qualified label for display (e.g., "MyStruct::method_name")
    pub(crate) qualified_label: String,
    /// Group/container name for categorization (e.g., "MyStruct", "module::path")
    pub(crate) group: String,
}

/// Resolves function metadata using LSP server-provided information and fallback heuristics.
///
/// This function uses a prioritized strategy:
/// 1. SymbolInformation.container_name (most reliable when available)
/// 2. CallHierarchyItem.detail (rust-analyzer provides context like "impl MyStruct")
/// 3. Source file analysis (find impl blocks by reading the file)
/// 4. Module path inference (derive from file path)
/// 5. Default: "functions" group
///
/// The combination of LSP data and fallback heuristics ensures proper grouping/subgraphs.
pub(crate) fn resolve_function_meta(
    item: &CallHierarchyItem,
    function_symbols: &[SymbolInformation],
    workspace_root_path: &Path,
    crate_name: &str,
) -> FunctionMeta {
    // Priority 1: Try to find the symbol in the workspace symbols and use container_name
    let same_file_and_name: Vec<&SymbolInformation> = function_symbols
        .iter()
        .filter(|s| s.name == item.name && s.location.uri == item.uri)
        .collect();

    let matched = same_file_and_name
        .iter()
        .min_by_key(|s| {
            let a = s.location.range.start.line as i64;
            let b = item.selection_range.start.line as i64;
            (a - b).abs()
        })
        .copied();

    if let Some(symbol) = matched {
        if let Some(container) = &symbol.container_name {
            let group = container.trim().to_string();
            if !group.is_empty() {
                return FunctionMeta {
                    qualified_label: format!("{}::{}", group, item.name),
                    group,
                };
            }
        }
    }

    // Priority 2: Try to extract qualified name from CallHierarchyItem's detail field
    // rust-analyzer provides context like "impl MyStruct" or "fn function_name"
    // Priority 2: Try to extract qualified name from CallHierarchyItem's detail field
    // rust-analyzer provides context like "impl MyStruct" or "fn function_name"
    if let Some(detail) = &item.detail {
        if let Some(meta) = extract_from_detail(detail, &item.name) {
            return meta;
        }
    }

    // Priority 3: Try to infer owner from source file (find impl block)
    if let Some(owner) = infer_impl_owner_from_source(item) {
        return FunctionMeta {
            qualified_label: format!("{}::{}", owner, item.name),
            group: owner,
        };
    }

    // Priority 4: Infer module from file path
    if let Some(module) = infer_module_owner_from_uri(item, workspace_root_path, crate_name) {
        return FunctionMeta {
            qualified_label: format!("{}::{}", module, item.name),
            group: module,
        };
    }

    // Fallback: use default group
    FunctionMeta {
        qualified_label: item.name.clone(),
        group: String::from("functions"),
    }
}

/// Extracts metadata from the detail field provided by the LSP server.
///
/// The detail field typically contains contextual information like:
/// - "impl MyStruct" for methods
/// - "impl<T> MyStruct<T>" for generic impls
/// - "impl MyTrait for MyStruct" for trait implementations
fn extract_from_detail(detail: &str, function_name: &str) -> Option<FunctionMeta> {
    let detail = detail.trim();

    // Handle "impl" blocks
    if let Some(after_impl) = detail.strip_prefix("impl ") {
        let impl_target = parse_impl_target(after_impl)?;
        return Some(FunctionMeta {
            qualified_label: format!("{}::{}", impl_target, function_name),
            group: impl_target,
        });
    }

    None
}

/// Parses the target type from an impl declaration.
///
/// Examples:
/// - "MyStruct" → "MyStruct"
/// - "MyStruct for MyTrait" → "MyStruct" (trait impl)
/// - "<T> MyStruct<T>" → "MyStruct" (generic impl)
fn parse_impl_target(impl_decl: &str) -> Option<String> {
    let impl_decl = impl_decl.trim();

    // Handle trait implementations: "impl MyTrait for MyStruct"
    if let Some(for_pos) = impl_decl.find(" for ") {
        let target = impl_decl[for_pos + 5..].trim();
        return Some(extract_type_name(target));
    }

    // Handle generic bounds: "impl<T> MyStruct<T>"
    let target = if impl_decl.starts_with('<') {
        // Skip generic parameters: "<T> MyStruct<T>" → "MyStruct<T>"
        impl_decl
            .find('>')
            .map(|pos| impl_decl[pos + 1..].trim())
            .unwrap_or(impl_decl)
    } else {
        impl_decl
    };

    Some(extract_type_name(target))
}

/// Extracts the base type name from a potentially complex type expression.
///
/// Examples:
/// - "MyStruct<T>" → "MyStruct"
/// - "MyStruct" → "MyStruct"
/// - "&mut MyStruct" → "MyStruct"
fn extract_type_name(type_expr: &str) -> String {
    type_expr
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .split_whitespace()
        .next()
        .unwrap_or(type_expr)
        .split('<')
        .next()
        .unwrap_or(type_expr)
        .trim()
        .to_string()
}

/// Infers the impl owner by reading the source file and finding the impl block.
/// This is used when LSP server doesn't provide container_name or detail.
fn infer_impl_owner_from_source(item: &CallHierarchyItem) -> Option<String> {
    let path = item.uri.to_file_path().ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let target_line = (item.selection_range.start.line as usize).min(lines.len() - 1);
    let fn_line = find_nearest_fn_line(&lines, target_line, &item.name)?;

    // Search backwards for impl block
    for start in (0..=fn_line).rev() {
        if !looks_like_impl_header_start(lines[start]) {
            continue;
        }

        let header = collect_header_until_brace(&lines, start);
        if !header.contains("impl") {
            continue;
        }

        if !header_block_contains_line(&lines, start, fn_line) {
            continue;
        }

        if let Some(owner) = parse_impl_owner(&header) {
            return Some(owner);
        }
    }

    None
}

fn looks_like_impl_header_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("impl ")
        || trimmed.starts_with("impl<")
        || trimmed.starts_with("unsafe impl ")
        || trimmed.starts_with("unsafe impl<")
}

fn find_nearest_fn_line(lines: &[&str], target_line: usize, fn_name: &str) -> Option<usize> {
    let marker = format!("fn {}", fn_name);
    let mut i = target_line;
    loop {
        if lines[i].contains(&marker) {
            return Some(i);
        }
        if i == 0 {
            break;
        }
        i -= 1;
    }
    None
}

fn collect_header_until_brace(lines: &[&str], start: usize) -> String {
    let mut joined = String::new();
    let end = (start + 8).min(lines.len());
    for line in lines.iter().take(end).skip(start) {
        if !joined.is_empty() {
            joined.push(' ');
        }
        joined.push_str(line.trim());
        if line.contains('{') {
            break;
        }
    }
    joined
}

fn header_block_contains_line(lines: &[&str], start: usize, target: usize) -> bool {
    let mut depth: i32 = 0;
    for line in lines.iter().take(target + 1).skip(start) {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
            }
        }
    }
    depth > 0
}

fn parse_impl_owner(header: &str) -> Option<String> {
    let after_impl = header.split_once("impl")?.1.trim();

    let candidate = if let Some(pos) = after_impl.find(" for ") {
        after_impl[pos + 5..].trim()
    } else if after_impl.starts_with('<') {
        if let Some(end) = after_impl.find('>') {
            after_impl[end + 1..].trim()
        } else {
            after_impl
        }
    } else {
        after_impl
    };

    let token = candidate
        .split_whitespace()
        .next()
        .unwrap_or(candidate)
        .trim_matches('{')
        .trim_matches('(')
        .trim_matches(')')
        .trim_matches('&')
        .trim_start_matches("mut ")
        .split('<')
        .next()
        .unwrap_or(candidate)
        .trim();

    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

/// Infers the module path from the file URI.
/// This provides a sensible default grouping based on file structure.
fn infer_module_owner_from_uri(
    item: &CallHierarchyItem,
    workspace_root_path: &Path,
    crate_name: &str,
) -> Option<String> {
    let path = item.uri.to_file_path().ok()?;
    let rel = path.strip_prefix(workspace_root_path).ok()?;

    if rel == Path::new("src/main.rs") || rel == Path::new("src/lib.rs") {
        return Some(crate_name.to_string());
    }

    if !rel.starts_with("src") {
        return None;
    }

    let no_src = rel.strip_prefix("src").ok()?;
    let mut parts: Vec<String> = no_src
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    if parts.is_empty() {
        return None;
    }

    if let Some(last) = parts.last_mut() {
        if last == "mod.rs" {
            parts.pop();
        } else if let Some(stem) = last.strip_suffix(".rs") {
            *last = stem.to_string();
        }
    }

    if parts.is_empty() {
        return Some(crate_name.to_string());
    }

    Some(parts.join("::"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{CallHierarchyItem, Location, Position, Range, SymbolInformation, SymbolKind, Url};

    fn make_item(name: &str, uri: &str, line: u32, detail: Option<&str>) -> CallHierarchyItem {
        let url = Url::parse(uri).unwrap();
        let pos = Position { line, character: 0 };
        let range = Range { start: pos, end: pos };
        CallHierarchyItem {
            name: name.to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: detail.map(|s| s.to_string()),
            uri: url,
            range,
            selection_range: range,
            data: None,
        }
    }

    #[allow(deprecated)]
    fn make_symbol(name: &str, uri: &str, line: u32, container: Option<&str>) -> SymbolInformation {
        let url = Url::parse(uri).unwrap();
        let pos = Position { line, character: 0 };
        SymbolInformation {
            name: name.to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            location: Location {
                uri: url,
                range: Range { start: pos, end: pos },
            },
            container_name: container.map(|s| s.to_string()),
        }
    }

    // --- extract_from_detail ---

    #[test]
    fn test_extract_from_detail_simple_impl() {
        let meta = extract_from_detail("impl MyStruct", "my_method").unwrap();
        assert_eq!(meta.qualified_label, "MyStruct::my_method");
        assert_eq!(meta.group, "MyStruct");
    }

    #[test]
    fn test_extract_from_detail_generic_impl() {
        let meta = extract_from_detail("impl <T> MyStruct<T>", "my_method").unwrap();
        assert_eq!(meta.qualified_label, "MyStruct::my_method");
        assert_eq!(meta.group, "MyStruct");
    }

    #[test]
    fn test_extract_from_detail_trait_impl() {
        let meta = extract_from_detail("impl Display for MyStruct", "fmt").unwrap();
        assert_eq!(meta.qualified_label, "MyStruct::fmt");
        assert_eq!(meta.group, "MyStruct");
    }

    #[test]
    fn test_extract_from_detail_returns_none_for_fn_prefix() {
        assert!(extract_from_detail("fn my_func", "my_func").is_none());
    }

    #[test]
    fn test_extract_from_detail_returns_none_when_no_impl_space() {
        // "impl<T>" without a trailing space does not match the "impl " prefix strip
        assert!(extract_from_detail("impl<T> MyStruct<T>", "my_method").is_none());
    }

    // --- extract_type_name ---

    #[test]
    fn test_extract_type_name_simple() {
        assert_eq!(extract_type_name("MyStruct"), "MyStruct");
    }

    #[test]
    fn test_extract_type_name_generic() {
        assert_eq!(extract_type_name("MyStruct<T>"), "MyStruct");
    }

    #[test]
    fn test_extract_type_name_reference() {
        assert_eq!(extract_type_name("&mut MyStruct"), "MyStruct");
    }

    #[test]
    fn test_extract_type_name_immutable_reference() {
        assert_eq!(extract_type_name("&MyStruct"), "MyStruct");
    }

    // --- parse_impl_target ---

    #[test]
    fn test_parse_impl_target_simple_struct() {
        assert_eq!(parse_impl_target("MyStruct").unwrap(), "MyStruct");
    }

    #[test]
    fn test_parse_impl_target_trait_impl() {
        assert_eq!(parse_impl_target("Display for MyStruct").unwrap(), "MyStruct");
    }

    #[test]
    fn test_parse_impl_target_generic_struct() {
        assert_eq!(parse_impl_target("<T> MyStruct<T>").unwrap(), "MyStruct");
    }

    // --- looks_like_impl_header_start ---

    #[test]
    fn test_impl_header_start_plain_impl() {
        assert!(looks_like_impl_header_start("impl MyStruct {"));
    }

    #[test]
    fn test_impl_header_start_generic_impl() {
        assert!(looks_like_impl_header_start("impl<T> MyStruct<T> {"));
    }

    #[test]
    fn test_impl_header_start_unsafe_impl() {
        assert!(looks_like_impl_header_start("unsafe impl Sync for MyStruct {}"));
    }

    #[test]
    fn test_impl_header_start_unsafe_generic_impl() {
        assert!(looks_like_impl_header_start("unsafe impl<T> Send for MyStruct<T> {}"));
    }

    #[test]
    fn test_impl_header_start_indented() {
        assert!(looks_like_impl_header_start("    impl MyStruct {"));
    }

    #[test]
    fn test_impl_header_start_rejects_fn() {
        assert!(!looks_like_impl_header_start("fn my_func() {"));
    }

    #[test]
    fn test_impl_header_start_rejects_empty_string() {
        assert!(!looks_like_impl_header_start(""));
    }

    // --- find_nearest_fn_line ---

    #[test]
    fn test_find_nearest_fn_line_at_exact_position() {
        let lines = vec!["fn other() {}", "    fn my_fn() {", "}"];
        assert_eq!(find_nearest_fn_line(&lines, 1, "my_fn"), Some(1));
    }

    #[test]
    fn test_find_nearest_fn_line_searches_backward() {
        let lines = vec!["fn my_fn() {", "    let x = 1;", "    x", "}"];
        assert_eq!(find_nearest_fn_line(&lines, 3, "my_fn"), Some(0));
    }

    #[test]
    fn test_find_nearest_fn_line_returns_none_when_absent() {
        let lines = vec!["fn other() {}", "    let x = 1;"];
        assert_eq!(find_nearest_fn_line(&lines, 1, "nonexistent"), None);
    }

    // --- collect_header_until_brace ---

    #[test]
    fn test_collect_header_single_line_with_brace() {
        let lines = vec!["impl MyStruct {", "    fn foo() {}", "}"];
        let header = collect_header_until_brace(&lines, 0);
        assert!(header.contains("impl MyStruct"));
        assert!(header.contains('{'));
        assert!(!header.contains("fn foo"));
    }

    #[test]
    fn test_collect_header_multiline_stops_at_brace() {
        let lines = vec!["impl MyStruct", "{", "    fn foo() {}", "}"];
        let header = collect_header_until_brace(&lines, 0);
        assert!(header.contains("impl MyStruct"));
        assert!(header.contains('{'));
        assert!(!header.contains("fn foo"));
    }

    // --- header_block_contains_line ---

    #[test]
    fn test_header_block_contains_inner_line() {
        let lines = vec!["impl MyStruct {", "    fn foo() {}", "}"];
        assert!(header_block_contains_line(&lines, 0, 1));
    }

    #[test]
    fn test_header_block_excludes_line_after_closing_brace() {
        let lines = vec!["impl MyStruct {", "    fn foo() {}", "}", "fn bar() {}"];
        assert!(!header_block_contains_line(&lines, 0, 3));
    }

    // --- parse_impl_owner ---

    #[test]
    fn test_parse_impl_owner_simple_struct() {
        assert_eq!(parse_impl_owner("impl MyStruct {").unwrap(), "MyStruct");
    }

    #[test]
    fn test_parse_impl_owner_trait_impl() {
        assert_eq!(
            parse_impl_owner("impl Display for MyStruct {").unwrap(),
            "MyStruct"
        );
    }

    #[test]
    fn test_parse_impl_owner_generic_struct() {
        assert_eq!(parse_impl_owner("impl<T> MyStruct<T> {").unwrap(), "MyStruct");
    }

    // --- infer_module_owner_from_uri ---

    #[test]
    fn test_module_owner_main_rs_returns_crate_name() {
        let item = make_item("my_fn", "file:///workspace/src/main.rs", 0, None);
        let result = infer_module_owner_from_uri(&item, Path::new("/workspace"), "my_crate");
        assert_eq!(result.unwrap(), "my_crate");
    }

    #[test]
    fn test_module_owner_lib_rs_returns_crate_name() {
        let item = make_item("my_fn", "file:///workspace/src/lib.rs", 0, None);
        let result = infer_module_owner_from_uri(&item, Path::new("/workspace"), "my_crate");
        assert_eq!(result.unwrap(), "my_crate");
    }

    #[test]
    fn test_module_owner_regular_src_file() {
        let item = make_item("my_fn", "file:///workspace/src/renderer.rs", 0, None);
        let result = infer_module_owner_from_uri(&item, Path::new("/workspace"), "my_crate");
        assert_eq!(result.unwrap(), "renderer");
    }

    #[test]
    fn test_module_owner_nested_module() {
        let item = make_item("my_fn", "file:///workspace/src/lsp/client.rs", 0, None);
        let result = infer_module_owner_from_uri(&item, Path::new("/workspace"), "my_crate");
        assert_eq!(result.unwrap(), "lsp::client");
    }

    #[test]
    fn test_module_owner_mod_rs_strips_filename() {
        let item = make_item("my_fn", "file:///workspace/src/lsp/mod.rs", 0, None);
        let result = infer_module_owner_from_uri(&item, Path::new("/workspace"), "my_crate");
        assert_eq!(result.unwrap(), "lsp");
    }

    #[test]
    fn test_module_owner_outside_src_returns_none() {
        let item = make_item("my_fn", "file:///workspace/tests/helper.rs", 0, None);
        let result = infer_module_owner_from_uri(&item, Path::new("/workspace"), "my_crate");
        assert!(result.is_none());
    }

    // --- resolve_function_meta ---

    #[test]
    fn test_resolve_meta_priority1_uses_container_name() {
        let item = make_item("my_method", "file:///workspace/src/foo.rs", 5, None);
        let symbol = make_symbol("my_method", "file:///workspace/src/foo.rs", 5, Some("MyStruct"));
        let meta = resolve_function_meta(&item, &[symbol], Path::new("/workspace"), "my_crate");
        assert_eq!(meta.qualified_label, "MyStruct::my_method");
        assert_eq!(meta.group, "MyStruct");
    }

    #[test]
    fn test_resolve_meta_priority1_skips_empty_container_name() {
        // container_name is present but empty — should fall through to detail (priority 2)
        let item = make_item(
            "my_method",
            "file:///workspace/src/foo.rs",
            5,
            Some("impl MyStruct"),
        );
        let symbol = make_symbol("my_method", "file:///workspace/src/foo.rs", 5, Some(""));
        let meta = resolve_function_meta(&item, &[symbol], Path::new("/workspace"), "my_crate");
        assert_eq!(meta.qualified_label, "MyStruct::my_method");
        assert_eq!(meta.group, "MyStruct");
    }

    #[test]
    fn test_resolve_meta_priority2_uses_detail_when_no_symbol() {
        let item = make_item(
            "my_method",
            "file:///workspace/src/foo.rs",
            5,
            Some("impl MyStruct"),
        );
        let meta = resolve_function_meta(&item, &[], Path::new("/workspace"), "my_crate");
        assert_eq!(meta.qualified_label, "MyStruct::my_method");
        assert_eq!(meta.group, "MyStruct");
    }

    #[test]
    fn test_resolve_meta_priority4_uses_module_path() {
        // No container_name, no detail, no source file match → falls through to file path
        let item = make_item("my_fn", "file:///workspace/src/renderer.rs", 0, None);
        let meta = resolve_function_meta(&item, &[], Path::new("/workspace"), "my_crate");
        assert_eq!(meta.qualified_label, "renderer::my_fn");
        assert_eq!(meta.group, "renderer");
    }

    #[test]
    fn test_resolve_meta_fallback_uses_functions_group() {
        // No container_name, no detail, file outside src/ → default group
        let item = make_item("my_fn", "file:///workspace/tests/helper.rs", 0, None);
        let meta = resolve_function_meta(&item, &[], Path::new("/workspace"), "my_crate");
        assert_eq!(meta.qualified_label, "my_fn");
        assert_eq!(meta.group, "functions");
    }
}
