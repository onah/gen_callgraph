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
            out.push_str(&format!(
                "    \"{}\" [label=\"{}\"];\n",
                escape_dot(&node.id),
                escape_dot(&node.label)
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
