use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, VecDeque};

pub const MAX_VERTICES: usize = 4096;
pub const MAX_EDGES: usize = 32768;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    TooManyVertices { count: usize, limit: usize },
    TooManyEdges { limit: usize },
    BadEndpoint { edge: usize, side: u8, vertex: usize },
    BadQuery { side: u8, vertex: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arc {
    pub from: usize,
    pub to: usize,
    pub multiplicity: usize,
    pub first_edge: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Relation {
    Cycle,
    Trivial,
    Forward,
    Backward,
    Separate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub kind: Relation,
    pub route: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Graph {
    pub components: Vec<Vec<usize>>,
    pub component_of: Vec<usize>,
    pub cyclic: Vec<bool>,
    pub internal_edges: Vec<usize>,
    pub arcs: Vec<Arc>,
    pub topo: Vec<usize>,
    pub depth: Vec<usize>,
}

pub fn analyze(n: usize, edges: &[(usize, usize)]) -> Result<Graph, GraphError> {
    if n > MAX_VERTICES {
        return Err(GraphError::TooManyVertices {
            count: n,
            limit: MAX_VERTICES,
        });
    }
    if edges.len() > MAX_EDGES {
        return Err(GraphError::TooManyEdges {
            limit: MAX_EDGES,
        });
    }
    for (edge, &(from, to)) in edges.iter().enumerate() {
        if from >= n {
            return Err(GraphError::BadEndpoint {
                edge,
                side: 0,
                vertex: from,
            });
        }
        if to >= n {
            return Err(GraphError::BadEndpoint {
                edge,
                side: 1,
                vertex: to,
            });
        }
    }

    let mut outgoing = vec![Vec::new(); n];
    let mut incoming = vec![Vec::new(); n];
    for &(from, to) in edges {
        outgoing[from].push(to);
        incoming[to].push(from);
    }
    for neighbors in &mut outgoing {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    for neighbors in &mut incoming {
        neighbors.sort_unstable();
        neighbors.dedup();
    }

    // Iterative first pass of Kosaraju's algorithm.
    let mut visited = vec![false; n];
    let mut finish_order = Vec::with_capacity(n);
    for start in 0..n {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((vertex, next_index)) = stack.last_mut() {
            if *next_index < outgoing[*vertex].len() {
                let next = outgoing[*vertex][*next_index];
                *next_index += 1;
                if !visited[next] {
                    visited[next] = true;
                    stack.push((next, 0));
                }
            } else {
                finish_order.push(*vertex);
                stack.pop();
            }
        }
    }

    // Iterative second pass on the reversed graph.
    visited.fill(false);
    let mut components = Vec::new();
    for &start in finish_order.iter().rev() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut stack = vec![start];
        let mut component = Vec::new();
        while let Some(vertex) = stack.pop() {
            component.push(vertex);
            for &next in &incoming[vertex] {
                if !visited[next] {
                    visited[next] = true;
                    stack.push(next);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }

    components.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));

    let mut component_of = vec![0; n];
    for (id, component) in components.iter().enumerate() {
        for &vertex in component {
            component_of[vertex] = id;
        }
    }

    let component_count = components.len();
    let mut cyclic: Vec<bool> = components.iter().map(|c| c.len() > 1).collect();
    let mut internal_edges = vec![0usize; component_count];
    let mut arc_data: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();

    for (edge_index, &(from, to)) in edges.iter().enumerate() {
        let source = component_of[from];
        let target = component_of[to];
        if source == target {
            internal_edges[source] += 1;
            if from == to {
                cyclic[source] = true;
            }
        } else {
            let entry = arc_data
                .entry((source, target))
                .or_insert((0usize, edge_index));
            entry.0 += 1;
            entry.1 = entry.1.min(edge_index);
        }
    }

    let mut arcs: Vec<Arc> = arc_data
        .into_iter()
        .map(|((from, to), (multiplicity, first_edge))| Arc {
            from,
            to,
            multiplicity,
            first_edge,
        })
        .collect();
    arcs.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| b.to.cmp(&a.to)));

    let mut dag = vec![Vec::new(); component_count];
    let mut indegree = vec![0usize; component_count];
    for arc in &arcs {
        dag[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    let mut available = BinaryHeap::new();
    for id in 0..component_count {
        if indegree[id] == 0 {
            available.push((internal_edges[id], components[id][0]));
        }
    }
    let mut id_by_priority = BTreeMap::new();
    for id in 0..component_count {
        id_by_priority.insert((internal_edges[id], components[id][0]), id);
    }

    let mut topo = Vec::with_capacity(component_count);
    let mut depth = vec![0usize; component_count];
    while let Some(priority) = available.pop() {
        let id = id_by_priority[&priority];
        topo.push(id);
        for &next in &dag[id] {
            depth[next] = depth[next].max(depth[id] + 1);
            indegree[next] -= 1;
            if indegree[next] == 0 {
                available.push((internal_edges[next], components[next][0]));
            }
        }
    }

    Ok(Graph {
        components,
        component_of,
        cyclic,
        internal_edges,
        arcs,
        topo,
        depth,
    })
}

impl Graph {
    fn shortest_route(&self, start: usize, goal: usize) -> Option<Vec<usize>> {
        let count = self.components.len();
        let mut outgoing = vec![Vec::new(); count];
        for arc in &self.arcs {
            outgoing[arc.from].push(arc.to);
        }
        for neighbors in &mut outgoing {
            neighbors.sort_unstable();
            neighbors.dedup();
        }

        let mut predecessor = vec![None; count];
        let mut seen = vec![false; count];
        let mut queue = VecDeque::new();
        seen[start] = true;
        queue.push_back(start);

        while let Some(vertex) = queue.pop_front() {
            if vertex == goal {
                break;
            }
            for &next in &outgoing[vertex] {
                if !seen[next] {
                    seen[next] = true;
                    predecessor[next] = Some(vertex);
                    queue.push_back(next);
                }
            }
        }

        if !seen[goal] {
            return None;
        }

        let mut route = vec![goal];
        let mut current = goal;
        while current != start {
            current = predecessor[current]?;
            route.push(current);
        }
        route.reverse();
        Some(route)
    }

    pub fn query(&self, from: usize, to: usize) -> Result<Query, GraphError> {
        if from >= self.component_of.len() {
            return Err(GraphError::BadQuery {
                side: 0,
                vertex: from,
            });
        }
        if to >= self.component_of.len() {
            return Err(GraphError::BadQuery {
                side: 1,
                vertex: to,
            });
        }

        let a = self.component_of[from];
        let b = self.component_of[to];

        if a == b {
            if self.cyclic[a] {
                return Ok(Query {
                    kind: Relation::Cycle,
                    route: vec![a, a],
                });
            }
            return Ok(Query {
                kind: Relation::Trivial,
                route: vec![a],
            });
        }

        if let Some(route) = self.shortest_route(a, b) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }

        if let Some(mut route) = self.shortest_route(b, a) {
            route.reverse();
            return Ok(Query {
                kind: Relation::Backward,
                route,
            });
        }

        Ok(Query {
            kind: Relation::Separate,
            route: Vec::new(),
        })
    }
}

fn main() {}