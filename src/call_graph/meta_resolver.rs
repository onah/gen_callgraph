use lsp_types::{CallHierarchyItem, SymbolInformation};
use std::path::Path;

#[derive(Debug, Clone)]
pub(crate) struct FunctionMeta {
    pub(crate) qualified_label: String,
    pub(crate) group: String,
}

pub(crate) fn resolve_function_meta(
    item: &CallHierarchyItem,
    function_symbols: &[SymbolInformation],
    workspace_root_path: &Path,
    crate_name: &str,
) -> FunctionMeta {
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

    if let Some(detail) = &item.detail {
        let detail = detail.trim();
        if let Some(after_impl) = detail.strip_prefix("impl ") {
            let candidate = if let Some(for_pos) = after_impl.find(" for ") {
                after_impl[for_pos + 5..].trim()
            } else if after_impl.starts_with('<') {
                if let Some(end) = after_impl.find('>') {
                    after_impl[end + 1..].trim()
                } else {
                    after_impl
                }
            } else {
                after_impl
            };

            let base = candidate
                .split_whitespace()
                .next()
                .unwrap_or(candidate)
                .split('<')
                .next()
                .unwrap_or(candidate)
                .trim()
                .to_string();

            if !base.is_empty() {
                return FunctionMeta {
                    qualified_label: format!("{}::{}", base, item.name),
                    group: base,
                };
            }
        }
    }

    if let Some(owner) = infer_impl_owner_from_source(item) {
        return FunctionMeta {
            qualified_label: format!("{}::{}", owner, item.name),
            group: owner,
        };
    }

    if let Some(module) = infer_module_owner_from_uri(item, workspace_root_path, crate_name) {
        return FunctionMeta {
            qualified_label: format!("{}::{}", module, item.name),
            group: module,
        };
    }

    FunctionMeta {
        qualified_label: item.name.clone(),
        group: String::from("global"),
    }
}

fn infer_impl_owner_from_source(item: &CallHierarchyItem) -> Option<String> {
    let path = item.uri.to_file_path().ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let target_line = (item.selection_range.start.line as usize).min(lines.len() - 1);
    let fn_line = find_nearest_fn_line(&lines, target_line, &item.name)?;

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
