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
    for (i, &(u, v)) in edges.iter().enumerate() {
        if u >= n {
            return Err(GraphError::BadEndpoint {
                edge: i,
                side: 0,
                vertex: u,
            });
        }
        if v >= n {
            return Err(GraphError::BadEndpoint {
                edge: i,
                side: 1,
                vertex: v,
            });
        }
    }
    Ok(())
}

fn build_graph(n: usize, edges: &[(usize, usize)]) -> Graph {
    let mut adj = vec![Vec::<usize>::new(); n];
    let mut rev = vec![Vec::<usize>::new(); n];
    for &(u, v) in edges {
        adj[u].push(v);
        rev[v].push(u);
    }

    let mut seen = vec![false; n];
    let mut finish = Vec::with_capacity(n);
    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0usize)];
        while let Some((v, next)) = stack.last_mut() {
            if *next < adj[*v].len() {
                let w = adj[*v][*next];
                *next += 1;
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                finish.push(*v);
                stack.pop();
            }
        }
    }

    let mut assigned = vec![false; n];
    let mut components = Vec::<Vec<usize>>::new();
    for &root in finish.iter().rev() {
        if assigned[root] {
            continue;
        }
        assigned[root] = true;
        let mut members = Vec::new();
        let mut stack = vec![root];
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
    let mut component_of = vec![0usize; n];
    for (id, members) in components.iter().enumerate() {
        for &v in members {
            component_of[v] = id;
        }
    }

    let count = components.len();
    let mut cyclic = vec![false; count];
    let mut internal_edges = vec![0usize; count];
    let mut pair_meta = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (idx, &(u, v)) in edges.iter().enumerate() {
        let a = component_of[u];
        let b = component_of[v];
        if a == b {
            internal_edges[a] += 1;
            if u == v {
                cyclic[a] = true;
            }
        } else {
            let entry = pair_meta.entry((a, b)).or_insert((0, idx));
            entry.0 += 1;
            if idx < entry.1 {
                entry.1 = idx;
            }
        }
    }
    for (id, members) in components.iter().enumerate() {
        if members.len() > 1 {
            cyclic[id] = true;
        }
    }

    let mut arcs: Vec<Arc> = pair_meta
        .into_iter()
        .map(|((from, to), (multiplicity, first_edge))| Arc {
            from,
            to,
            multiplicity,
            first_edge,
        })
        .collect();
    arcs.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| b.to.cmp(&a.to)));

    let mut outgoing = vec![Vec::<usize>::new(); count];
    let mut indegree = vec![0usize; count];
    for arc in &arcs {
        outgoing[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    let mut ready = std::collections::BinaryHeap::<(usize, usize, usize)>::new();
    for id in 0..count {
        if indegree[id] == 0 {
            ready.push((internal_edges[id], components[id][0], id));
        }
    }
    let mut topo = Vec::with_capacity(count);
    let mut depth = vec![0usize; count];
    while let Some((_, _, id)) = ready.pop() {
        topo.push(id);
        for &to in &outgoing[id] {
            depth[to] = depth[to].max(depth[id] + 1);
            indegree[to] -= 1;
            if indegree[to] == 0 {
                ready.push((internal_edges[to], components[to][0], to));
            }
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
    fn shortest_route(&self, start: usize, end: usize) -> Option<Vec<usize>> {
        let count = self.components.len();
        let mut outgoing = vec![Vec::<usize>::new(); count];
        for arc in &self.arcs {
            outgoing[arc.from].push(arc.to);
        }
        for neighbors in &mut outgoing {
            neighbors.sort_unstable();
        }

        let mut parent = vec![usize::MAX; count];
        let mut queue = std::collections::VecDeque::new();
        parent[start] = start;
        queue.push_back(start);
        while let Some(v) = queue.pop_front() {
            if v == end {
                break;
            }
            for &w in &outgoing[v] {
                if parent[w] == usize::MAX {
                    parent[w] = v;
                    queue.push_back(w);
                }
            }
        }
        if parent[end] == usize::MAX {
            return None;
        }
        let mut route = vec![end];
        let mut at = end;
        while at != start {
            at = parent[at];
            route.push(at);
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
        for (i, &(u, v)) in edges.iter().enumerate() {
            if u >= self.n {
                return Err(GraphError::BadEndpoint {
                    edge: i,
                    side: 0,
                    vertex: u,
                });
            }
            if v >= self.n {
                return Err(GraphError::BadEndpoint {
                    edge: i,
                    side: 1,
                    vertex: v,
                });
            }
        }

        let mut steps = Vec::with_capacity(edges.len());
        let mut merges = Vec::new();

        for &(u, v) in edges {
            let edge_index = self.edges.len();
            let before_graph = build_graph(self.n, &self.edges);
            let before = (
                before_graph.component_of[u],
                before_graph.component_of[v],
            );
            let duplicate = self.edges.iter().any(|&(x, y)| x == u && y == v);
            self.edges.push((u, v));
            let after_graph = build_graph(self.n, &self.edges);
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
                let mut before_ids = Vec::new();
                for (id, members) in before_graph.components.iter().enumerate() {
                    if members.iter().any(|&member| {
                        after_graph.component_of[member] == after.0
                    }) {
                        before_ids.push(id);
                    }
                }
                before_ids.sort_unstable();
                before_ids.dedup();
                let members = after_graph.components[after.0].clone();
                let event = Merge {
                    edge_index,
                    before: before_ids,
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
            graph: build_graph(self.n, &self.edges),
        })
    }
}

fn main() {}