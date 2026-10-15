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

fn shortest_route(adj: &[Vec<usize>], start: usize, end: usize) -> Option<Vec<usize>> {
    let n = adj.len();
    let mut reverse = vec![Vec::new(); n];
    for (u, outs) in adj.iter().enumerate() {
        for &v in outs {
            reverse[v].push(u);
        }
    }

    let mut distance = vec![usize::MAX; n];
    let mut queue = std::collections::VecDeque::new();
    distance[end] = 0;
    queue.push_back(end);

    while let Some(v) = queue.pop_front() {
        for &u in &reverse[v] {
            if distance[u] == usize::MAX {
                distance[u] = distance[v] + 1;
                queue.push_back(u);
            }
        }
    }

    if distance[start] == usize::MAX {
        return None;
    }

    let mut route = vec![start];
    let mut current = start;
    while current != end {
        let next = adj[current]
            .iter()
            .copied()
            .filter(|&v| distance[v] != usize::MAX && distance[v] + 1 == distance[current])
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

    let mut adj = vec![Vec::new(); n];
    let mut rev = vec![Vec::new(); n];
    for &(u, v) in edges {
        adj[u].push(v);
        rev[v].push(u);
    }

    let mut seen = vec![false; n];
    let mut order = Vec::with_capacity(n);
    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0usize)];
        while let Some((v, next_i)) = stack.pop() {
            if next_i < adj[v].len() {
                stack.push((v, next_i + 1));
                let w = adj[v][next_i];
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                order.push(v);
            }
        }
    }

    let mut assigned = vec![false; n];
    let mut components = Vec::new();
    for &root in order.iter().rev() {
        if assigned[root] {
            continue;
        }
        assigned[root] = true;
        let mut component = Vec::new();
        let mut stack = vec![root];
        while let Some(v) = stack.pop() {
            component.push(v);
            for &w in &rev[v] {
                if !assigned[w] {
                    assigned[w] = true;
                    stack.push(w);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }

    components.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    let mut component_of = vec![0; n];
    for (cid, component) in components.iter().enumerate() {
        for &v in component {
            component_of[v] = cid;
        }
    }

    let c = components.len();
    let mut cyclic: Vec<bool> = components.iter().map(|component| component.len() >= 2).collect();
    let mut internal_edges = vec![0usize; c];
    let mut arc_map = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (i, &(u, v)) in edges.iter().enumerate() {
        let a = component_of[u];
        let b = component_of[v];
        if a == b {
            internal_edges[a] += 1;
            if u == v {
                cyclic[a] = true;
            }
        } else {
            let entry = arc_map.entry((a, b)).or_insert((0, i));
            entry.0 += 1;
            entry.1 = entry.1.min(i);
        }
    }

    let mut arcs: Vec<Arc> = arc_map
        .iter()
        .map(|(&(from, to), &(multiplicity, first_edge))| Arc {
            from,
            to,
            multiplicity,
            first_edge,
        })
        .collect();
    arcs.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| b.to.cmp(&a.to)));

    let mut dag = vec![Vec::new(); c];
    let mut indegree = vec![0usize; c];
    for arc in &arcs {
        dag[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    let mut ready = std::collections::BinaryHeap::new();
    for cid in 0..c {
        if indegree[cid] == 0 {
            ready.push((internal_edges[cid], components[cid][0], cid));
        }
    }

    let mut topo = Vec::with_capacity(c);
    let mut depth = vec![0usize; c];
    while let Some((_, _, v)) = ready.pop() {
        topo.push(v);
        for &w in &dag[v] {
            depth[w] = depth[w].max(depth[v] + 1);
            indegree[w] -= 1;
            if indegree[w] == 0 {
                ready.push((internal_edges[w], components[w][0], w));
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

        let mut adj = vec![Vec::new(); self.components.len()];
        for arc in &self.arcs {
            adj[arc.from].push(arc.to);
        }
        for outs in &mut adj {
            outs.sort_unstable();
            outs.dedup();
        }

        if let Some(route) = shortest_route(&adj, a, b) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }
        if let Some(mut route) = shortest_route(&adj, b, a) {
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