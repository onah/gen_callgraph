//! Domain model types for the call graph. Pure data structures with no I/O or analysis logic.

/// A single node in the call graph, representing one function or method.
#[derive(Debug, Clone)]
pub struct CallGraphNode {
    /// Unique stable identifier (e.g. fully-qualified symbol name or LSP URI + range).
    pub id: String,
    /// Human-readable display label shown in the rendered output.
    pub label: String,
    /// Logical group (e.g. crate or module name) used to cluster nodes in the DOT output.
    pub group: String,
}

/// A directed call edge: `from` calls `to`.
#[derive(Debug, Clone)]
pub struct CallGraphEdge {
    /// `id` of the calling node.
    pub from: String,
    /// `id` of the callee node.
    pub to: String,
}

/// The complete call graph: a collection of nodes and directed edges.
#[derive(Debug, Clone)]
pub struct CallGraph {
    pub nodes: Vec<CallGraphNode>,
    pub edges: Vec<CallGraphEdge>,
}
