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
pub enum EdgeStatus {
    Duplicate,
    Internal,
    Link,
    Merge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub edge_index: usize,
    pub status: EdgeStatus,
    pub before: (usize, usize),
    pub after: (usize, usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merge {
    pub edge_index: usize,
    pub before: Vec<usize>,
    pub after: usize,
    pub members: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    pub steps: Vec<Step>,
    pub merges: Vec<Merge>,
    pub graph: Graph,
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

fn validate(n: usize, edges: &[(usize, usize)]) -> Result<(), GraphError> {
    if n > MAX_VERTICES {
        return Err(GraphError::TooManyVertices {
            count: n,
            limit: MAX_VERTICES,
        });
    }
    if edges.len() > MAX_EDGES {
        return Err(GraphError::TooManyEdges { limit: MAX_EDGES });
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
    Ok(())
}

fn strongly_connected_components(n: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
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
        while let Some((v, next)) = stack.pop() {
            if next < adj[v].len() {
                stack.push((v, next + 1));
                let w = adj[v][next];
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
        let mut members = Vec::new();
        let mut stack = vec![root];
        assigned[root] = true;
        while let Some(v) = stack.pop() {
            members.push(v);
            for &w in &rev[v] {
                if !assigned[w] {
                    assigned[w] = true;
                    stack.push(w);
                }
            }
        }
        members.sort_unstable();
        components.push(members);
    }

    components.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    components
}

fn build_graph(n: usize, edges: &[(usize, usize)]) -> Graph {
    let components = strongly_connected_components(n, edges);
    let count = components.len();
    let mut component_of = vec![0usize; n];
    for (id, members) in components.iter().enumerate() {
        for &v in members {
            component_of[v] = id;
        }
    }

    let mut cyclic = components.iter().map(|c| c.len() > 1).collect::<Vec<_>>();
    let mut internal_edges = vec![0usize; count];
    let mut arc_map = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (idx, &(u, v)) in edges.iter().enumerate() {
        let a = component_of[u];
        let b = component_of[v];
        if a == b {
            internal_edges[a] += 1;
            if u == v {
                cyclic[a] = true;
            }
        } else {
            let entry = arc_map.entry((a, b)).or_insert((0, idx));
            entry.0 += 1;
            entry.1 = entry.1.min(idx);
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

    let mut depth = vec![0usize; count];
    let mut emitted = vec![false; count];
    let mut topo = Vec::with_capacity(count);

    loop {
        let mut best: Option<usize> = None;
        for v in 0..count {
            if !emitted[v] && indegree[v] == 0 {
                best = match best {
                    None => Some(v),
                    Some(current) => {
                        let bv = internal_edges[v];
                        let bc = internal_edges[current];
                        let min_v = components[v][0];
                        let min_c = components[current][0];
                        if bv > bc || (bv == bc && min_v > min_c) {
                            Some(v)
                        } else {
                            Some(current)
                        }
                    }
                };
            }
        }
        let Some(v) = best else { break };
        emitted[v] = true;
        topo.push(v);
        for &w in &outgoing[v] {
            depth[w] = depth[w].max(depth[v] + 1);
            indegree[w] -= 1;
        }
    }

    Graph {
        components,
        component_of,
        cyclic,
        internal_edges,
        arcs,
        topo,
        depth,
    }
}

pub fn analyze(n: usize, edges: &[(usize, usize)]) -> Result<Graph, GraphError> {
    validate(n, edges)?;
    Ok(build_graph(n, edges))
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

        let forward = shortest_route(self, a, b);
        if let Some(route) = forward {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }

        if let Some(mut route) = shortest_route(self, b, a) {
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

fn shortest_route(graph: &Graph, start: usize, goal: usize) -> Option<Vec<usize>> {
    let n = graph.components.len();
    let mut outgoing = vec![Vec::new(); n];
    for arc in &graph.arcs {
        outgoing[arc.from].push(arc.to);
    }
    for list in &mut outgoing {
        list.sort_unstable();
        list.dedup();
    }

    let mut seen = vec![false; n];
    let mut parent = vec![usize::MAX; n];
    let mut queue = std::collections::VecDeque::new();
    seen[start] = true;
    queue.push_back(start);

    while let Some(v) = queue.pop_front() {
        if v == goal {
            break;
        }
        for &w in &outgoing[v] {
            if !seen[w] {
                seen[w] = true;
                parent[w] = v;
                queue.push_back(w);
            }
        }
    }

    if !seen[goal] {
        return None;
    }
    let mut route = vec![goal];
    let mut at = goal;
    while at != start {
        at = parent[at];
        route.push(at);
    }
    route.reverse();
    Some(route)
}

#[derive(Clone, Debug)]
pub struct Incremental {
    n: usize,
    edges: Vec<(usize, usize)>,
    history: Vec<Merge>,
}

impl Incremental {
    pub fn new(n: usize, edges: &[(usize, usize)]) -> Result<Self, GraphError> {
        validate(n, edges)?;
        Ok(Self {
            n,
            edges: edges.to_vec(),
            history: Vec::new(),
        })
    }

    pub fn snapshot(&self) -> Graph {
        build_graph(self.n, &self.edges)
    }

    pub fn history(&self) -> Vec<Merge> {
        self.history.clone()
    }

    pub fn insert(&mut self, edges: &[(usize, usize)]) -> Result<Batch, GraphError> {
        if self.edges.len() + edges.len() > MAX_EDGES {
            return Err(GraphError::TooManyEdges { limit: MAX_EDGES });
        }
        for (i, &(from, to)) in edges.iter().enumerate() {
            if from >= self.n {
                return Err(GraphError::BadEndpoint {
                    edge: i,
                    side: 0,
                    vertex: from,
                });
            }
            if to >= self.n {
                return Err(GraphError::BadEndpoint {
                    edge: i,
                    side: 1,
                    vertex: to,
                });
            }
        }

        let mut steps = Vec::with_capacity(edges.len());
        let mut merges = Vec::new();

        for &(u, v) in edges {
            let edge_index = self.edges.len();
            let before_graph = self.snapshot();
            let before = (before_graph.component_of[u], before_graph.component_of[v]);
            let duplicate = self.edges.iter().any(|&(x, y)| x == u && y == v);

            self.edges.push((u, v));
            let after_graph = self.snapshot();
            let after = (after_graph.component_of[u], after_graph.component_of[v]);

            let status = if duplicate {
                EdgeStatus::Duplicate
            } else if before.0 == before.1 {
                EdgeStatus::Internal
            } else if after.0 == after.1 {
                EdgeStatus::Merge
            } else {
                EdgeStatus::Link
            };

            if status == EdgeStatus::Merge {
                let mut united_ids = Vec::new();
                for (id, members) in before_graph.components.iter().enumerate() {
                    if members.iter().any(|&vertex| {
                        after_graph.component_of[vertex] == after.0
                    }) {
                        united_ids.push(id);
                    }
                }
                united_ids.sort_unstable();
                united_ids.dedup();
                let members = after_graph.components[after.0].clone();
                let event = Merge {
                    edge_index,
                    before: united_ids,
                    after: after.0,
                    members,
                };
                self.history.push(event.clone());
                merges.push(event);
            }

            steps.push(Step {
                edge_index,
                status,
                before,
                after,
            });
        }

        Ok(Batch {
            steps,
            merges,
            graph: self.snapshot(),
        })
    }
}