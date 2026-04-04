use std::collections::{HashMap, VecDeque};

use petgraph::Direction::{Incoming, Outgoing};
use petgraph::algo::has_path_connecting;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::report::RAFReport;

const EPSILON: f64 = 1e-9;

#[derive(Clone, Debug, Default)]
pub struct CausalGraph {
    graph: DiGraph<String, f64>,
    node_indices: HashMap<String, NodeIndex>,
}

impl CausalGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, id: impl Into<String>) {
        let id = id.into();
        self.ensure_node(id);
    }

    pub fn add_edge(&mut self, from: impl Into<String>, to: impl Into<String>, confidence: f64) {
        let from = from.into();
        let to = to.into();
        let confidence = confidence.clamp(0.0, 1.0);
        let from_index = self.ensure_node(from);
        let to_index = self.ensure_node(to);

        if let Some(edge_index) = self.graph.find_edge(from_index, to_index) {
            if let Some(weight) = self.graph.edge_weight_mut(edge_index) {
                *weight = confidence;
            }
        } else {
            self.graph.add_edge(from_index, to_index, confidence);
        }
    }

    pub fn remove_node(&mut self, id: &str) -> bool {
        if let Some(node_index) = self.node_indices.remove(id) {
            self.graph.remove_node(node_index);
            if node_index.index() < self.graph.node_count() {
                let swapped_node_id = self.graph[node_index].clone();
                self.node_indices.insert(swapped_node_id, node_index);
            }
            true
        } else {
            false
        }
    }

    pub fn repair_coverage(&self) -> f64 {
        let node_count = self.graph.node_count();
        if node_count == 0 {
            return 0.0;
        }

        let covered = self
            .graph
            .node_indices()
            .filter(|&node| {
                self.graph
                    .neighbors_directed(node, Incoming)
                    .any(|incoming| incoming != node)
            })
            .count();

        covered as f64 / node_count as f64
    }

    pub fn bottlenecks(&self) -> Vec<String> {
        let scores = self.betweenness_scores();
        let max_score = scores.values().copied().fold(0.0, f64::max);

        if max_score <= EPSILON {
            return Vec::new();
        }

        let mut bottlenecks = scores
            .into_iter()
            .filter(|(_, score)| (*score - max_score).abs() <= EPSILON)
            .map(|(node, _)| self.graph[node].clone())
            .collect::<Vec<_>>();
        bottlenecks.sort();
        bottlenecks
    }

    pub fn is_raf_connected(&self) -> bool {
        let node_count = self.graph.node_count();
        if node_count < 2 {
            return false;
        }

        self.graph.node_indices().all(|target| {
            self.graph
                .node_indices()
                .filter(|&source| source != target)
                .any(|source| has_path_connecting(&self.graph, source, target, None))
        })
    }

    pub fn report(&self) -> RAFReport {
        RAFReport::from_graph(self)
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub(crate) fn raf_connectivity(&self) -> f64 {
        let node_count = self.graph.node_count();
        if node_count == 0 {
            return 0.0;
        }

        let reachable = self
            .graph
            .node_indices()
            .filter(|&target| {
                self.graph
                    .node_indices()
                    .filter(|&source| source != target)
                    .any(|source| has_path_connecting(&self.graph, source, target, None))
            })
            .count();

        reachable as f64 / node_count as f64
    }

    fn ensure_node(&mut self, id: String) -> NodeIndex {
        if let Some(&node_index) = self.node_indices.get(&id) {
            return node_index;
        }

        let node_index = self.graph.add_node(id.clone());
        self.node_indices.insert(id, node_index);
        node_index
    }

    fn betweenness_scores(&self) -> HashMap<NodeIndex, f64> {
        let mut scores = self
            .graph
            .node_indices()
            .map(|node| (node, 0.0))
            .collect::<HashMap<_, _>>();

        for source in self.graph.node_indices() {
            let mut stack = Vec::new();
            let mut predecessors = self
                .graph
                .node_indices()
                .map(|node| (node, Vec::<NodeIndex>::new()))
                .collect::<HashMap<_, _>>();
            let mut path_counts = self
                .graph
                .node_indices()
                .map(|node| (node, 0.0))
                .collect::<HashMap<_, _>>();
            let mut distances = self
                .graph
                .node_indices()
                .map(|node| (node, None::<usize>))
                .collect::<HashMap<_, _>>();
            let mut queue = VecDeque::new();

            path_counts.insert(source, 1.0);
            distances.insert(source, Some(0));
            queue.push_back(source);

            while let Some(node) = queue.pop_front() {
                stack.push(node);
                let distance = distances[&node].expect("queued nodes always have a distance");

                for neighbor in self.graph.neighbors_directed(node, Outgoing) {
                    if distances[&neighbor].is_none() {
                        distances.insert(neighbor, Some(distance + 1));
                        queue.push_back(neighbor);
                    }

                    if distances[&neighbor] == Some(distance + 1) {
                        let candidate_paths = path_counts[&neighbor] + path_counts[&node];
                        path_counts.insert(neighbor, candidate_paths);
                        predecessors
                            .get_mut(&neighbor)
                            .expect("all nodes have predecessor buckets")
                            .push(node);
                    }
                }
            }

            let mut dependencies = self
                .graph
                .node_indices()
                .map(|node| (node, 0.0))
                .collect::<HashMap<_, _>>();

            while let Some(node) = stack.pop() {
                let node_dependency = dependencies[&node];
                let node_paths = path_counts[&node];

                for predecessor in predecessors[&node].iter().copied() {
                    let predecessor_paths = path_counts[&predecessor];
                    if node_paths > 0.0 {
                        let contribution =
                            (predecessor_paths / node_paths) * (1.0 + node_dependency);
                        let updated = dependencies[&predecessor] + contribution;
                        dependencies.insert(predecessor, updated);
                    }
                }

                if node != source {
                    let updated = scores[&node] + node_dependency;
                    scores.insert(node, updated);
                }
            }
        }

        scores
    }
}

#[cfg(test)]
mod tests {
    use super::CausalGraph;

    #[test]
    fn empty_graph_has_no_coverage_or_connectivity() {
        let graph = CausalGraph::new();

        assert_eq!(graph.repair_coverage(), 0.0);
        assert!(graph.bottlenecks().is_empty());
        assert!(!graph.is_raf_connected());

        let report = graph.report();
        assert_eq!(report.node_count, 0);
        assert_eq!(report.edge_count, 0);
        assert_eq!(report.repair_coverage, 0.0);
        assert_eq!(report.raf_connectivity, 0.0);
        assert!(!report.raf_connected);
    }

    #[test]
    fn single_node_is_not_repairable() {
        let mut graph = CausalGraph::new();
        graph.add_node("kernel");

        assert_eq!(graph.repair_coverage(), 0.0);
        assert!(graph.bottlenecks().is_empty());
        assert!(!graph.is_raf_connected());
    }

    #[test]
    fn linear_chain_has_partial_coverage_and_one_bottleneck() {
        let mut graph = CausalGraph::new();
        graph.add_edge("a", "b", 0.9);
        graph.add_edge("b", "c", 0.8);

        assert!((graph.repair_coverage() - (2.0 / 3.0)).abs() < 1e-9);
        assert_eq!(graph.bottlenecks(), vec!["b".to_string()]);
        assert!(!graph.is_raf_connected());
    }

    #[test]
    fn fully_connected_graph_has_full_coverage_and_connectivity() {
        let mut graph = CausalGraph::new();

        for node in ["a", "b", "c"] {
            graph.add_node(node);
        }

        for from in ["a", "b", "c"] {
            for to in ["a", "b", "c"] {
                if from != to {
                    graph.add_edge(from, to, 1.0);
                }
            }
        }

        assert_eq!(graph.repair_coverage(), 1.0);
        assert!(graph.bottlenecks().is_empty());
        assert!(graph.is_raf_connected());
    }

    #[test]
    fn disconnected_components_are_not_raf_connected() {
        let mut graph = CausalGraph::new();
        graph.add_edge("a", "b", 1.0);
        graph.add_edge("b", "c", 1.0);
        graph.add_edge("d", "e", 1.0);

        assert!((graph.repair_coverage() - (3.0 / 5.0)).abs() < 1e-9);
        assert_eq!(graph.bottlenecks(), vec!["b".to_string()]);
        assert!(!graph.is_raf_connected());
    }

    #[test]
    fn can_remove_nodes() {
        let mut graph = CausalGraph::new();
        graph.add_edge("a", "b", 1.0);
        graph.add_edge("b", "c", 1.0);

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);

        assert!(graph.remove_node("b"));
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 0);

        assert!(!graph.remove_node("d")); // Doesn't exist
    }
}
