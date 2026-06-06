//! Renders a [`CallGraph`](crate::call_graph::CallGraph) into a GraphViz DOT format string.
//!
//! No LSP or analysis knowledge; this module depends only on `CallGraph`. The output is a
//! `digraph` with:
//! - One `subgraph cluster_*` per group (rendered left-to-right via `rankdir=LR`)
//! - Labelled nodes inside each cluster
//! - Directed edges (`from -> to`) outside the clusters

use crate::call_graph::{CallGraph, CallGraphNode};
use std::collections::BTreeMap;

fn escape_dot(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

pub fn to_dot(graph: &CallGraph) -> String {
    let mut out = String::from("digraph callgraph {\n");
    out.push_str("  rankdir=LR;\n");
    out.push_str("  compound=true;\n");

    let mut grouped: BTreeMap<String, Vec<&CallGraphNode>> = BTreeMap::new();
    for node in &graph.nodes {
        grouped.entry(node.group.clone()).or_default().push(node);
    }

    for (idx, (group, nodes)) in grouped.iter().enumerate() {
        out.push_str(&format!(
            "  subgraph \"cluster_{}\" {{\n",
            escape_dot(&format!("{}", idx))
        ));
        out.push_str(&format!("    label=\"{}\";\n", escape_dot(group)));
        out.push_str("    color=lightgray;\n");

        for node in nodes {
            // The subgraph cluster already shows the container/module via its label,
            // so display only the bare function/method name here.
            let short_label = node.label.rsplit("::").next().unwrap_or(&node.label);
            out.push_str(&format!(
                "    \"{}\" [label=\"{}\"];\n",
                escape_dot(&node.id),
                escape_dot(short_label)
            ));
        }
        out.push_str("  }\n");
    }

    for edge in &graph.edges {
        out.push_str(&format!(
            "  \"{}\" -> \"{}\";\n",
            escape_dot(&edge.from),
            escape_dot(&edge.to)
        ));
    }

    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_graph::{CallGraphEdge, CallGraphNode};

    fn make_graph(nodes: Vec<(&str, &str, &str)>, edges: Vec<(&str, &str)>) -> CallGraph {
        CallGraph {
            nodes: nodes
                .into_iter()
                .map(|(id, label, group)| CallGraphNode {
                    id: id.to_string(),
                    label: label.to_string(),
                    group: group.to_string(),
                })
                .collect(),
            edges: edges
                .into_iter()
                .map(|(from, to)| CallGraphEdge {
                    from: from.to_string(),
                    to: to.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn node_label_shows_only_short_name_when_qualified() {
        let graph = make_graph(vec![("id1", "MyStruct::my_method", "MyStruct")], vec![]);
        let dot = to_dot(&graph);
        assert!(
            dot.contains("[label=\"my_method\"]"),
            "expected short name in label, got:\n{dot}"
        );
        assert!(
            !dot.contains("[label=\"MyStruct::my_method\"]"),
            "qualified label should not appear in node label, got:\n{dot}"
        );
    }

    #[test]
    fn node_label_is_unchanged_when_no_separator() {
        let graph = make_graph(vec![("id1", "standalone_fn", "functions")], vec![]);
        let dot = to_dot(&graph);
        assert!(
            dot.contains("[label=\"standalone_fn\"]"),
            "plain name should appear unchanged, got:\n{dot}"
        );
    }

    #[test]
    fn subgraph_cluster_label_uses_group() {
        let graph = make_graph(vec![("id1", "MyStruct::my_method", "MyStruct")], vec![]);
        let dot = to_dot(&graph);
        assert!(
            dot.contains("label=\"MyStruct\";"),
            "cluster label should be the group name, got:\n{dot}"
        );
    }

    #[test]
    fn edges_are_rendered_with_node_ids() {
        let graph = make_graph(
            vec![("id1", "A::foo", "A"), ("id2", "B::bar", "B")],
            vec![("id1", "id2")],
        );
        let dot = to_dot(&graph);
        assert!(
            dot.contains("\"id1\" -> \"id2\";"),
            "edge should use node ids, got:\n{dot}"
        );
    }
}
