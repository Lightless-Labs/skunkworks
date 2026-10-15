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

        if let Some(route) = shortest_lex_route(&self.arcs, a, b, self.components.len()) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }

        if let Some(mut route) = shortest_lex_route(&self.arcs, b, a, self.components.len()) {
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

fn shortest_lex_route(arcs: &[Arc], start: usize, goal: usize, n: usize) -> Option<Vec<usize>> {
    let mut adjacency = vec![Vec::new(); n];
    for arc in arcs {
        adjacency[arc.from].push(arc.to);
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
        neighbors.dedup();
    }

    let mut distance = vec![usize::MAX; n];
    let mut queue = std::collections::VecDeque::new();
    distance[goal] = 0;
    queue.push_back(goal);

    let mut reverse_adjacency = vec![Vec::new(); n];
    for (from, neighbors) in adjacency.iter().enumerate() {
        for &to in neighbors {
            reverse_adjacency[to].push(from);
        }
    }

    while let Some(v) = queue.pop_front() {
        let next_distance = distance[v] + 1;
        for &prev in &reverse_adjacency[v] {
            if distance[prev] == usize::MAX {
                distance[prev] = next_distance;
                queue.push_back(prev);
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
            .filter(|&v| distance[v] + 1 == distance[current])
            .min()?;
        route.push(next);
        current = next;
    }
    Some(route)
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

    let mut adjacency = vec![Vec::new(); n];
    for &(from, to) in edges {
        adjacency[from].push(to);
    }

    // Iterative Kosaraju pass avoids recursion on long paths.
    let mut visited = vec![false; n];
    let mut finish_order = Vec::with_capacity(n);

    for start in 0..n {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((vertex, next_index)) = stack.last_mut() {
            if *next_index < adjacency[*vertex].len() {
                let neighbor = adjacency[*vertex][*next_index];
                *next_index += 1;
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    stack.push((neighbor, 0));
                }
            } else {
                let (finished, _) = stack.pop().unwrap();
                finish_order.push(finished);
            }
        }
    }

    let mut reverse_adjacency = vec![Vec::new(); n];
    for &(from, to) in edges {
        reverse_adjacency[to].push(from);
    }

    let mut assigned = vec![false; n];
    let mut raw_components = Vec::new();
    for &start in finish_order.iter().rev() {
        if assigned[start] {
            continue;
        }
        assigned[start] = true;
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(vertex) = stack.pop() {
            component.push(vertex);
            for &neighbor in &reverse_adjacency[vertex] {
                if !assigned[neighbor] {
                    assigned[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        component.sort_unstable();
        raw_components.push(component);
    }

    raw_components.sort_by(|a, b| {
        b.len()
            .cmp(&a.len())
            .then_with(|| a[0].cmp(&b[0]))
    });

    let mut component_of = vec![0usize; n];
    for (component_id, component) in raw_components.iter().enumerate() {
        for &vertex in component {
            component_of[vertex] = component_id;
        }
    }

    let count = raw_components.len();
    let mut cyclic = raw_components
        .iter()
        .map(|component| component.len() >= 2)
        .collect::<Vec<_>>();
    let mut internal_edges = vec![0usize; count];
    let mut arc_map = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (edge_index, &(from, to)) in edges.iter().enumerate() {
        let source = component_of[from];
        let target = component_of[to];
        if source == target {
            internal_edges[source] += 1;
            if from == to {
                cyclic[source] = true;
            }
        } else {
            let entry = arc_map.entry((source, target)).or_insert((0, edge_index));
            entry.0 += 1;
            entry.1 = entry.1.min(edge_index);
        }
    }

    let mut arcs = arc_map
        .into_iter()
        .map(|((from, to), (multiplicity, first_edge))| Arc {
            from,
            to,
            multiplicity,
            first_edge,
        })
        .collect::<Vec<_>>();
    arcs.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| b.to.cmp(&a.to)));

    let mut outgoing = vec![Vec::new(); count];
    let mut indegree = vec![0usize; count];
    for arc in &arcs {
        outgoing[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    // The heap stores priority keys in reverse order so its maximum is selected.
    let mut ready = std::collections::BinaryHeap::new();
    for component_id in 0..count {
        if indegree[component_id] == 0 {
            ready.push((
                internal_edges[component_id],
                raw_components[component_id][0],
                component_id,
            ));
        }
    }

    let mut topo = Vec::with_capacity(count);
    let mut depth = vec![0usize; count];

    while let Some((_, _, component_id)) = ready.pop() {
        topo.push(component_id);
        for &target in &outgoing[component_id] {
            depth[target] = depth[target].max(depth[component_id] + 1);
            indegree[target] -= 1;
            if indegree[target] == 0 {
                ready.push((
                    internal_edges[target],
                    raw_components[target][0],
                    target,
                ));
            }
        }
    }

    Ok(Graph {
        components: raw_components,
        component_of,
        cyclic,
        internal_edges,
        arcs,
        topo,
        depth,
    })
}

fn main() {}