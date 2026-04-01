use serde::{Deserialize, Serialize};

use crate::graph::CausalGraph;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RAFReport {
    pub node_count: usize,
    pub edge_count: usize,
    pub repair_coverage: f64,
    pub raf_connectivity: f64,
    pub raf_connected: bool,
    pub bottlenecks: Vec<String>,
}

impl RAFReport {
    pub fn from_graph(graph: &CausalGraph) -> Self {
        Self {
            node_count: graph.node_count(),
            edge_count: graph.edge_count(),
            repair_coverage: graph.repair_coverage(),
            raf_connectivity: graph.raf_connectivity(),
            raf_connected: graph.is_raf_connected(),
            bottlenecks: graph.bottlenecks(),
        }
    }
}
