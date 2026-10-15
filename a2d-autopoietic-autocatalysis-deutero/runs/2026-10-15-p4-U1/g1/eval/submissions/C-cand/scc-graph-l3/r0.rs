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

    for (i, &(from, to)) in edges.iter().enumerate() {
        if from >= n {
            return Err(GraphError::BadEndpoint {
                edge: i,
                side: 0,
                vertex: from,
            });
        }
        if to >= n {
            return Err(GraphError::BadEndpoint {
                edge: i,
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

    // Iterative DFS finishing order.
    let mut seen = vec![false; n];
    let mut finish = Vec::with_capacity(n);
    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0usize)];
        while let Some((v, next_index)) = stack.pop() {
            if next_index < outgoing[v].len() {
                stack.push((v, next_index + 1));
                let w = outgoing[v][next_index];
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                finish.push(v);
            }
        }
    }

    // Reverse-graph traversal in decreasing finishing order.
    seen.fill(false);
    let mut raw_components = Vec::<Vec<usize>>::new();
    for &root in finish.iter().rev() {
        if seen[root] {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![root];
        seen[root] = true;
        while let Some(v) = stack.pop() {
            component.push(v);
            for &w in &incoming[v] {
                if !seen[w] {
                    seen[w] = true;
                    stack.push(w);
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
    let mut component_of = vec![0usize; n];
    for (id, component) in components.iter().enumerate() {
        for &v in component {
            component_of[v] = id;
        }
    }

    let count = components.len();
    let mut cyclic = vec![false; count];
    for (id, component) in components.iter().enumerate() {
        if component.len() > 1 {
            cyclic[id] = true;
        }
    }

    let mut internal_edges = vec![0usize; count];
    let mut arc_map = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();
    for (edge_index, &(from, to)) in edges.iter().enumerate() {
        let a = component_of[from];
        let b = component_of[to];
        if a == b {
            internal_edges[a] += 1;
            if from == to {
                cyclic[a] = true;
            }
        } else {
            let entry = arc_map.entry((a, b)).or_insert((0, edge_index));
            entry.0 += 1;
            entry.1 = entry.1.min(edge_index);
        }
    }

    let mut arcs: Vec<Arc> = arc_map
        .into_iter()
        .map(|((from, to), (multiplicity, first_edge))| Arc {
            from,
            to,
            multiplicity,
            first_edge,
        })
        .collect();
    arcs.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| b.to.cmp(&a.to)));

    let mut dag_out = vec![Vec::<usize>::new(); count];
    let mut indegree = vec![0usize; count];
    for arc in &arcs {
        dag_out[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    // The ready set is ordered by decreasing internal edge count, then
    // decreasing smallest vertex. IDs are unique, so the final key is stable.
    let mut ready = std::collections::BTreeSet::<(usize, usize, usize)>::new();
    for id in 0..count {
        if indegree[id] == 0 {
            ready.insert((internal_edges[id], components[id][0], id));
        }
    }

    let mut topo = Vec::with_capacity(count);
    let mut depth = vec![0usize; count];
    while let Some(&(edge_count, smallest, id)) = ready.iter().next_back() {
        ready.remove(&(edge_count, smallest, id));
        topo.push(id);
        for &to in &dag_out[id] {
            depth[to] = depth[to].max(depth[id] + 1);
            indegree[to] -= 1;
            if indegree[to] == 0 {
                ready.insert((internal_edges[to], components[to][0], to));
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

        let mut adjacency = vec![Vec::<usize>::new(); self.components.len()];
        for arc in &self.arcs {
            adjacency[arc.from].push(arc.to);
        }
        for neighbors in &mut adjacency {
            neighbors.sort_unstable();
            neighbors.dedup();
        }

        if let Some(route) = shortest_lex_route(a, b, &adjacency) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }
        if let Some(mut route) = shortest_lex_route(b, a, &adjacency) {
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

fn shortest_lex_route(start: usize, target: usize, adjacency: &[Vec<usize>]) -> Option<Vec<usize>> {
    let n = adjacency.len();
    let mut distance = vec![usize::MAX; n];
    let mut queue = std::collections::VecDeque::new();

    distance[target] = 0;
    queue.push_back(target);

    // Compute distances to target by walking the reverse condensation graph.
    let mut reverse = vec![Vec::<usize>::new(); n];
    for (from, neighbors) in adjacency.iter().enumerate() {
        for &to in neighbors {
            reverse[to].push(from);
        }
    }
    while let Some(v) = queue.pop_front() {
        for &previous in &reverse[v] {
            if distance[previous] == usize::MAX {
                distance[previous] = distance[v] + 1;
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
        let next = *adjacency[current]
            .iter()
            .filter(|&&candidate| {
                distance[candidate] != usize::MAX
                    && distance[candidate] + 1 == distance[current]
            })
            .min()?;
        route.push(next);
        current = next;
    }
    Some(route)
}

fn main() {}