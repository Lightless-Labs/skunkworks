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
        return Err(GraphError::TooManyEdges { limit: MAX_EDGES });
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

    let mut adjacency = vec![Vec::new(); n];
    let mut reverse = vec![Vec::new(); n];
    for &(from, to) in edges {
        adjacency[from].push(to);
        reverse[to].push(from);
    }

    // Iterative Kosaraju traversal avoids stack growth on long paths.
    let mut seen = vec![false; n];
    let mut finish = Vec::with_capacity(n);
    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0usize)];
        while let Some((vertex, next_index)) = stack.pop() {
            if next_index < adjacency[vertex].len() {
                stack.push((vertex, next_index + 1));
                let next = adjacency[vertex][next_index];
                if !seen[next] {
                    seen[next] = true;
                    stack.push((next, 0));
                }
            } else {
                finish.push(vertex);
            }
        }
    }

    let mut raw_components = Vec::<Vec<usize>>::new();
    let mut assigned = vec![false; n];
    for &root in finish.iter().rev() {
        if assigned[root] {
            continue;
        }
        assigned[root] = true;
        let mut component = Vec::new();
        let mut stack = vec![root];
        while let Some(vertex) = stack.pop() {
            component.push(vertex);
            for &next in &reverse[vertex] {
                if !assigned[next] {
                    assigned[next] = true;
                    stack.push(next);
                }
            }
        }
        component.sort_unstable();
        raw_components.push(component);
    }

    raw_components.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    let components = raw_components;
    let mut component_of = vec![0usize; n];
    for (id, component) in components.iter().enumerate() {
        for &vertex in component {
            component_of[vertex] = id;
        }
    }

    let count = components.len();
    let mut cyclic = vec![false; count];
    for (id, component) in components.iter().enumerate() {
        cyclic[id] = component.len() > 1;
    }

    let mut internal_edges = vec![0usize; count];
    let mut arc_data = BTreeMap::<(usize, usize), (usize, usize)>::new();
    for (edge_index, &(from, to)) in edges.iter().enumerate() {
        let a = component_of[from];
        let b = component_of[to];
        if a == b {
            internal_edges[a] += 1;
            if from == to {
                cyclic[a] = true;
            }
        } else {
            let entry = arc_data.entry((a, b)).or_insert((0, edge_index));
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

    let mut outgoing = vec![Vec::new(); count];
    let mut indegree = vec![0usize; count];
    for arc in &arcs {
        outgoing[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    // Heap key: prefer greater internal edge count, then greater minimum vertex.
    let mut ready = BinaryHeap::<(usize, usize, Reverse<usize>)>::new();
    for id in 0..count {
        if indegree[id] == 0 {
            ready.push((internal_edges[id], components[id][0], Reverse(id)));
        }
    }

    let mut topo = Vec::with_capacity(count);
    let mut depth = vec![0usize; count];
    while let Some((_, _, Reverse(id))) = ready.pop() {
        topo.push(id);
        for &next in &outgoing[id] {
            depth[next] = depth[next].max(depth[id] + 1);
            indegree[next] -= 1;
            if indegree[next] == 0 {
                ready.push((internal_edges[next], components[next][0], Reverse(next)));
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

        let mut outgoing = vec![Vec::new(); self.components.len()];
        for arc in &self.arcs {
            outgoing[arc.from].push(arc.to);
        }
        for neighbors in &mut outgoing {
            neighbors.sort_unstable();
            neighbors.dedup();
        }

        if let Some(route) = shortest_lex_route(&outgoing, a, b) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }
        if let Some(mut route) = shortest_lex_route(&outgoing, b, a) {
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

fn shortest_lex_route(adjacency: &[Vec<usize>], start: usize, goal: usize) -> Option<Vec<usize>> {
    let n = adjacency.len();
    let mut distance = vec![usize::MAX; n];
    let mut queue = VecDeque::new();
    distance[goal] = 0;
    queue.push_back(goal);

    // Distances to goal, computed over reversed edges.
    let mut reverse = vec![Vec::new(); n];
    for (from, neighbors) in adjacency.iter().enumerate() {
        for &to in neighbors {
            reverse[to].push(from);
        }
    }
    while let Some(vertex) = queue.pop_front() {
        for &previous in &reverse[vertex] {
            if distance[previous] == usize::MAX {
                distance[previous] = distance[vertex] + 1;
                queue.push_back(previous);
            }
        }
    }

    if distance[start] == usize::MAX {
        return None;
    }

    let mut route = vec![start];
    let mut current = start;
    while current != goal {
        let next = adjacency[current]
            .iter()
            .copied()
            .filter(|&candidate| {
                distance[candidate] != usize::MAX
                    && distance[candidate] + 1 == distance[current]
            })
            .min()?;
        route.push(next);
        current = next;
    }
    Some(route)
}