#[derive(Debug, Clone)]
pub struct CallGraphNode {
    pub id: String,
    pub label: String,
    pub group: String,
}

#[derive(Debug, Clone)]
pub struct CallGraphEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone)]
pub struct CallGraph {
    pub nodes: Vec<CallGraphNode>,
    pub edges: Vec<CallGraphEdge>,
}
