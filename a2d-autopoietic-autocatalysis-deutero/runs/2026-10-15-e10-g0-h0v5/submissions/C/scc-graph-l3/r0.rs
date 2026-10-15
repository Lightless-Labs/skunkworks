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

fn find_route(adjacency: &[Vec<usize>], start: usize, goal: usize) -> Option<Vec<usize>> {
    let mut parent = vec![usize::MAX; adjacency.len()];
    let mut queue = std::collections::VecDeque::new();
    parent[start] = start;
    queue.push_back(start);

    while let Some(v) = queue.pop_front() {
        if v == goal {
            break;
        }
        for &next in &adjacency[v] {
            if parent[next] == usize::MAX {
                parent[next] = v;
                queue.push_back(next);
            }
        }
    }

    if parent[goal] == usize::MAX {
        return None;
    }

    let mut route = Vec::new();
    let mut v = goal;
    loop {
        route.push(v);
        if v == start {
            break;
        }
        v = parent[v];
    }
    route.reverse();
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
        while let Some((v, next_index)) = stack.pop() {
            if next_index < adjacency[v].len() {
                stack.push((v, next_index + 1));
                let next = adjacency[v][next_index];
                if !seen[next] {
                    seen[next] = true;
                    stack.push((next, 0));
                }
            } else {
                finish_order.push(v);
            }
        }
    }

    let mut assigned = vec![false; n];
    let mut components = Vec::new();
    for &start in finish_order.iter().rev() {
        if assigned[start] {
            continue;
        }
        assigned[start] = true;
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(v) = stack.pop() {
            component.push(v);
            for &next in &reverse[v] {
                if !assigned[next] {
                    assigned[next] = true;
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
        for &v in component {
            component_of[v] = id;
        }
    }

    let count = components.len();
    let mut cyclic = components.iter().map(|c| c.len() > 1).collect::<Vec<_>>();
    let mut internal_edges = vec![0usize; count];
    let mut arc_data = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (edge_index, &(from, to)) in edges.iter().enumerate() {
        let source = component_of[from];
        let target = component_of[to];
        if source == target {
            internal_edges[source] += 1;
            if from == to {
                cyclic[source] = true;
            }
        } else {
            let entry = arc_data.entry((source, target)).or_insert((0, edge_index));
            entry.0 += 1;
            entry.1 = entry.1.min(edge_index);
        }
    }

    let mut arcs = arc_data
        .into_iter()
        .map(|((from, to), (multiplicity, first_edge))| Arc {
            from,
            to,
            multiplicity,
            first_edge,
        })
        .collect::<Vec<_>>();
    arcs.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| b.to.cmp(&a.to)));

    let mut dag = vec![Vec::new(); count];
    let mut indegree = vec![0usize; count];
    for arc in &arcs {
        dag[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    let mut ready = std::collections::BinaryHeap::new();
    for id in 0..count {
        if indegree[id] == 0 {
            ready.push((internal_edges[id], components[id][0], id));
        }
    }

    let mut topo = Vec::with_capacity(count);
    let mut depth = vec![0usize; count];
    while let Some((_, _, current)) = ready.pop() {
        topo.push(current);
        for &next in &dag[current] {
            depth[next] = depth[next].max(depth[current] + 1);
            indegree[next] -= 1;
            if indegree[next] == 0 {
                ready.push((internal_edges[next], components[next][0], next));
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

        let mut adjacency = vec![Vec::new(); self.components.len()];
        for arc in &self.arcs {
            adjacency[arc.from].push(arc.to);
        }
        for neighbors in &mut adjacency {
            neighbors.sort_unstable();
            neighbors.dedup();
        }

        if let Some(route) = find_route(&adjacency, a, b) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }
        if let Some(mut route) = find_route(&adjacency, b, a) {
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