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

fn strongly_connected_components(
    n: usize,
    edges: &[(usize, usize)],
) -> Vec<Vec<usize>> {
    let mut adj = vec![Vec::new(); n];
    let mut rev = vec![Vec::new(); n];
    for &(u, v) in edges {
        adj[u].push(v);
        rev[v].push(u);
    }

    let mut seen = vec![false; n];
    let mut order = Vec::with_capacity(n);
    for start in 0..n {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((v, next)) = stack.last_mut() {
            if *next < adj[*v].len() {
                let w = adj[*v][*next];
                *next += 1;
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                let (done, _) = stack.pop().unwrap();
                order.push(done);
            }
        }
    }

    seen.fill(false);
    let mut components = Vec::new();
    for &start in order.iter().rev() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(v) = stack.pop() {
            component.push(v);
            for &w in &rev[v] {
                if !seen[w] {
                    seen[w] = true;
                    stack.push(w);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }

    components.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    components
}

fn analyze_validated(n: usize, edges: &[(usize, usize)]) -> Graph {
    let components = strongly_connected_components(n, edges);
    let count = components.len();
    let mut component_of = vec![0usize; n];
    for (id, members) in components.iter().enumerate() {
        for &v in members {
            component_of[v] = id;
        }
    }

    let mut cyclic: Vec<bool> = components.iter().map(|c| c.len() > 1).collect();
    let mut internal_edges = vec![0usize; count];
    let mut arc_data = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (i, &(u, v)) in edges.iter().enumerate() {
        let a = component_of[u];
        let b = component_of[v];
        if a == b {
            internal_edges[a] += 1;
            if u == v {
                cyclic[a] = true;
            }
        } else {
            let entry = arc_data.entry((a, b)).or_insert((0, i));
            entry.0 += 1;
            entry.1 = entry.1.min(i);
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

    let minima: Vec<usize> = components.iter().map(|c| c[0]).collect();
    let mut ready = Vec::new();
    for id in 0..count {
        if indegree[id] == 0 {
            ready.push(id);
        }
    }

    let mut topo = Vec::with_capacity(count);
    let mut depth = vec![0usize; count];
    while !ready.is_empty() {
        let best_pos = (0..ready.len())
            .max_by(|&i, &j| {
                internal_edges[ready[i]]
                    .cmp(&internal_edges[ready[j]])
                    .then_with(|| minima[ready[i]].cmp(&minima[ready[j]]))
            })
            .unwrap();
        let id = ready.swap_remove(best_pos);
        topo.push(id);
        for &to in &outgoing[id] {
            depth[to] = depth[to].max(depth[id] + 1);
            indegree[to] -= 1;
            if indegree[to] == 0 {
                ready.push(to);
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
    Ok(analyze_validated(n, edges))
}

impl Graph {
    fn shortest_route(&self, start: usize, goal: usize) -> Option<Vec<usize>> {
        let n = self.components.len();
        let mut outgoing = vec![Vec::new(); n];
        for arc in &self.arcs {
            outgoing[arc.from].push(arc.to);
        }
        for neighbors in &mut outgoing {
            neighbors.sort_unstable();
            neighbors.dedup();
        }

        let mut parent = vec![None; n];
        let mut seen = vec![false; n];
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
                    parent[w] = Some(v);
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
            at = parent[at]?;
            route.push(at);
        }
        route.reverse();
        Some(route)
    }

    pub fn query(&self, from: usize, to: usize) -> Result<Query, GraphError> {
        let n = self.component_of.len();
        if from >= n {
            return Err(GraphError::BadQuery {
                side: 0,
                vertex: from,
            });
        }
        if to >= n {
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
        analyze_validated(self.n, &self.edges)
    }

    pub fn history(&self) -> Vec<Merge> {
        self.history.clone()
    }

    pub fn insert(&mut self, edges: &[(usize, usize)]) -> Result<Batch, GraphError> {
        if self.edges.len() > MAX_EDGES - edges.len().min(MAX_EDGES) || edges.len() > MAX_EDGES {
            return Err(GraphError::TooManyEdges { limit: MAX_EDGES });
        }
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
            let before_graph = analyze_validated(self.n, &self.edges);
            let before = (
                before_graph.component_of[u],
                before_graph.component_of[v],
            );
            let duplicate = self.edges.iter().any(|&(a, b)| a == u && b == v);
            let old_component_count = before_graph.components.len();

            self.edges.push((u, v));
            let after_graph = analyze_validated(self.n, &self.edges);
            let after = (
                after_graph.component_of[u],
                after_graph.component_of[v],
            );

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
                let merged_id = after.0;
                let members = after_graph.components[merged_id].clone();
                let member_set: Vec<bool> = {
                    let mut set = vec![false; self.n];
                    for &member in &members {
                        set[member] = true;
                    }
                    set
                };
                let mut before_ids = Vec::new();
                for (id, component) in before_graph.components.iter().enumerate() {
                    if component.iter().any(|&member| member_set[member]) {
                        before_ids.push(id);
                    }
                }
                before_ids.sort_unstable();
                before_ids.dedup();

                let event = Merge {
                    edge_index,
                    before: before_ids,
                    after: merged_id,
                    members,
                };
                self.history.push(event.clone());
                merges.push(event);
            }

            let _ = old_component_count;
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