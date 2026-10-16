use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

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

struct DfsFrame {
    vertex: usize,
    next_edge: usize,
}

fn reachable_route(adjacency: &[Vec<usize>], start: usize, target: usize) -> Option<Vec<usize>> {
    let count = adjacency.len();
    let mut seen = vec![false; count];
    let mut parent = vec![usize::MAX; count];
    let mut queue = VecDeque::new();

    seen[start] = true;
    queue.push_back(start);

    while let Some(vertex) = queue.pop_front() {
        if vertex == target {
            let mut route = Vec::new();
            let mut current = target;
            loop {
                route.push(current);
                if current == start {
                    break;
                }
                current = parent[current];
            }
            route.reverse();
            return Some(route);
        }

        for &next in &adjacency[vertex] {
            if !seen[next] {
                seen[next] = true;
                parent[next] = vertex;
                queue.push_back(next);
            }
        }
    }
    None
}

impl Graph {
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

        let mut adjacency = vec![Vec::new(); self.components.len()];
        for arc in &self.arcs {
            adjacency[arc.from].push(arc.to);
        }
        for neighbors in &mut adjacency {
            neighbors.sort_unstable();
        }

        if let Some(route) = reachable_route(&adjacency, a, b) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }
        if let Some(mut route) = reachable_route(&adjacency, b, a) {
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
    for (index, &(from, to)) in edges.iter().enumerate() {
        if from >= n {
            return Err(GraphError::BadEndpoint {
                edge: index,
                side: 0,
                vertex: from,
            });
        }
        if to >= n {
            return Err(GraphError::BadEndpoint {
                edge: index,
                side: 1,
                vertex: to,
            });
        }
    }

    let mut adjacency = vec![Vec::new(); n];
    let mut reverse = vec![Vec::new(); n];
    for &(from, to) in edges {
        adjacency[from].push(to);
        reverse[to].push(from);
    }

    // Iterative Kosaraju traversal avoids recursion on long paths.
    let mut visited = vec![false; n];
    let mut finish_order = Vec::with_capacity(n);
    for root in 0..n {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut stack = vec![DfsFrame {
            vertex: root,
            next_edge: 0,
        }];
        while let Some(frame) = stack.last_mut() {
            if frame.next_edge < adjacency[frame.vertex].len() {
                let next = adjacency[frame.vertex][frame.next_edge];
                frame.next_edge += 1;
                if !visited[next] {
                    visited[next] = true;
                    stack.push(DfsFrame {
                        vertex: next,
                        next_edge: 0,
                    });
                }
            } else {
                let vertex = frame.vertex;
                stack.pop();
                finish_order.push(vertex);
            }
        }
    }

    let mut raw_components = Vec::<Vec<usize>>::new();
    let mut raw_component_of = vec![usize::MAX; n];
    for &root in finish_order.iter().rev() {
        if raw_component_of[root] != usize::MAX {
            continue;
        }
        let id = raw_components.len();
        let mut component = Vec::new();
        let mut stack = vec![root];
        raw_component_of[root] = id;
        while let Some(vertex) = stack.pop() {
            component.push(vertex);
            for &next in &reverse[vertex] {
                if raw_component_of[next] == usize::MAX {
                    raw_component_of[next] = id;
                    stack.push(next);
                }
            }
        }
        component.sort_unstable();
        raw_components.push(component);
    }

    let mut order: Vec<usize> = (0..raw_components.len()).collect();
    order.sort_by(|&left, &right| {
        raw_components[right]
            .len()
            .cmp(&raw_components[left].len())
            .then_with(|| raw_components[left][0].cmp(&raw_components[right][0]))
    });

    let mut old_to_new = vec![0; raw_components.len()];
    let mut components = Vec::with_capacity(raw_components.len());
    for (new_id, &old_id) in order.iter().enumerate() {
        old_to_new[old_id] = new_id;
        components.push(raw_components[old_id].clone());
    }

    let mut component_of = vec![0; n];
    for vertex in 0..n {
        component_of[vertex] = old_to_new[raw_component_of[vertex]];
    }

    let component_count = components.len();
    let mut cyclic: Vec<bool> = components.iter().map(|c| c.len() > 1).collect();
    let mut internal_edges = vec![0usize; component_count];
    let mut arc_data = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (index, &(from, to)) in edges.iter().enumerate() {
        let source = component_of[from];
        let target = component_of[to];
        if source == target {
            internal_edges[source] += 1;
            if from == to {
                cyclic[source] = true;
            }
        } else {
            let entry = arc_data.entry((source, target)).or_insert((0, index));
            entry.0 += 1;
            entry.1 = entry.1.min(index);
        }
    }

    let mut arcs = Vec::with_capacity(arc_data.len());
    for ((from, to), (multiplicity, first_edge)) in arc_data {
        arcs.push(Arc {
            from,
            to,
            multiplicity,
            first_edge,
        });
    }
    arcs.sort_by(|left, right| {
        left.from
            .cmp(&right.from)
            .then_with(|| right.to.cmp(&left.to))
    });

    let mut outgoing = vec![Vec::new(); component_count];
    let mut indegree = vec![0usize; component_count];
    for arc in &arcs {
        outgoing[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    // Reverse tuple ordering makes BinaryHeap select the requested greatest
    // internal edge count, then greatest smallest vertex.
    let mut ready = BinaryHeap::new();
    for component in 0..component_count {
        if indegree[component] == 0 {
            ready.push((
                internal_edges[component],
                components[component][0],
                Reverse(component),
            ));
        }
    }

    let mut topo = Vec::with_capacity(component_count);
    let mut depth = vec![0usize; component_count];
    while let Some((_, _, Reverse(component))) = ready.pop() {
        topo.push(component);
        for &next in &outgoing[component] {
            depth[next] = depth[next].max(depth[component] + 1);
            indegree[next] -= 1;
            if indegree[next] == 0 {
                ready.push((
                    internal_edges[next],
                    components[next][0],
                    Reverse(next),
                ));
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