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

    let mut adjacency = vec![Vec::new(); n];
    let mut reverse = vec![Vec::new(); n];
    for &(from, to) in edges {
        adjacency[from].push(to);
        reverse[to].push(from);
    }

    // Iterative Kosaraju traversal avoids recursion on long paths.
    let mut seen = vec![false; n];
    let mut finish_order = Vec::with_capacity(n);
    for start in 0..n {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((vertex, next_index)) = stack.pop() {
            if next_index < adjacency[vertex].len() {
                stack.push((vertex, next_index + 1));
                let next = adjacency[vertex][next_index];
                if !seen[next] {
                    seen[next] = true;
                    stack.push((next, 0));
                }
            } else {
                finish_order.push(vertex);
            }
        }
    }

    seen.fill(false);
    let mut raw_components = Vec::new();
    for &start in finish_order.iter().rev() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(vertex) = stack.pop() {
            component.push(vertex);
            for &next in &reverse[vertex] {
                if !seen[next] {
                    seen[next] = true;
                    stack.push(next);
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

    let components = raw_components;
    let component_count = components.len();
    let mut component_of = vec![0usize; n];
    for (id, component) in components.iter().enumerate() {
        for &vertex in component {
            component_of[vertex] = id;
        }
    }

    let mut cyclic: Vec<bool> = components.iter().map(|c| c.len() > 1).collect();
    let mut internal_edges = vec![0usize; component_count];
    let mut arc_data = vec![vec![None::<(usize, usize)>; component_count]; component_count];

    for (edge_index, &(from, to)) in edges.iter().enumerate() {
        let source = component_of[from];
        let target = component_of[to];
        if source == target {
            internal_edges[source] += 1;
            if from == to {
                cyclic[source] = true;
            }
        } else {
            let entry = &mut arc_data[source][target];
            match entry {
                Some((multiplicity, _)) => *multiplicity += 1,
                None => *entry = Some((1, edge_index)),
            }
        }
    }

    let mut arcs = Vec::new();
    let mut outgoing = vec![Vec::new(); component_count];
    let mut indegree = vec![0usize; component_count];
    for from in 0..component_count {
        for to in (0..component_count).rev() {
            if let Some((multiplicity, first_edge)) = arc_data[from][to] {
                arcs.push(Arc {
                    from,
                    to,
                    multiplicity,
                    first_edge,
                });
                outgoing[from].push(to);
                indegree[to] += 1;
            }
        }
    }

    // Priority Kahn traversal: larger internal edge count, then larger minimum vertex.
    let mut emitted = vec![false; component_count];
    let mut topo = Vec::with_capacity(component_count);
    let mut depth = vec![0usize; component_count];
    while topo.len() < component_count {
        let mut best = None;
        for id in 0..component_count {
            if !emitted[id] && indegree[id] == 0 {
                best = match best {
                    None => Some(id),
                    Some(current) => {
                        let candidate_key = (internal_edges[id], components[id][0]);
                        let current_key = (internal_edges[current], components[current][0]);
                        if candidate_key > current_key {
                            Some(id)
                        } else {
                            Some(current)
                        }
                    }
                };
            }
        }
        let id = best.expect("condensation graph must be acyclic");
        emitted[id] = true;
        topo.push(id);
        for &next in &outgoing[id] {
            depth[next] = depth[next].max(depth[id] + 1);
            indegree[next] -= 1;
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

        let count = self.components.len();
        let mut outgoing = vec![Vec::new(); count];
        for arc in &self.arcs {
            outgoing[arc.from].push(arc.to);
        }

        fn shortest_lex_route(
            start: usize,
            target: usize,
            outgoing: &[Vec<usize>],
        ) -> Option<Vec<usize>> {
            let mut distance = vec![usize::MAX; outgoing.len()];
            let mut queue = std::collections::VecDeque::new();
            distance[target] = 0;
            queue.push_back(target);

            // Reverse reachability distances can be computed from the target by
            // constructing the reverse adjacency once for this query.
            let mut reverse = vec![Vec::new(); outgoing.len()];
            for (from, neighbors) in outgoing.iter().enumerate() {
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
            while current != target {
                let next = outgoing[current]
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

        if let Some(route) = shortest_lex_route(a, b, &outgoing) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }
        if let Some(mut route) = shortest_lex_route(b, a, &outgoing) {
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