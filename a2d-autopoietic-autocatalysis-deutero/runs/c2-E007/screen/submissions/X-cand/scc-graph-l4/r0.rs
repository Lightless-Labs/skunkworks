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
    Ok(())
}

fn build_graph(n: usize, edges: &[(usize, usize)]) -> Graph {
    let mut out = vec![Vec::new(); n];
    let mut rev = vec![Vec::new(); n];
    for &(u, v) in edges {
        out[u].push(v);
        rev[v].push(u);
    }

    let mut seen = vec![false; n];
    let mut finish = Vec::with_capacity(n);
    for start in 0..n {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((v, next)) = stack.pop() {
            if next < out[v].len() {
                stack.push((v, next + 1));
                let w = out[v][next];
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                finish.push(v);
            }
        }
    }

    let mut assigned = vec![false; n];
    let mut components = Vec::new();
    for &start in finish.iter().rev() {
        if assigned[start] {
            continue;
        }
        assigned[start] = true;
        let mut members = Vec::new();
        let mut stack = vec![start];
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
    let mut component_of = vec![0; n];
    for (id, members) in components.iter().enumerate() {
        for &v in members {
            component_of[v] = id;
        }
    }

    let count = components.len();
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

    let mut ready = std::collections::BinaryHeap::new();
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
    fn shortest_route(&self, start: usize, target: usize) -> Option<Vec<usize>> {
        let count = self.components.len();
        let mut outgoing = vec![Vec::new(); count];
        for arc in &self.arcs {
            outgoing[arc.from].push(arc.to);
        }
        for neighbors in &mut outgoing {
            neighbors.sort_unstable();
        }

        let mut previous = vec![None; count];
        let mut visited = vec![false; count];
        let mut queue = std::collections::VecDeque::new();
        visited[start] = true;
        queue.push_back(start);

        while let Some(v) = queue.pop_front() {
            if v == target {
                break;
            }
            for &w in &outgoing[v] {
                if !visited[w] {
                    visited[w] = true;
                    previous[w] = Some(v);
                    queue.push_back(w);
                }
            }
        }

        if !visited[target] {
            return None;
        }
        let mut route = vec![target];
        let mut current = target;
        while current != start {
            current = previous[current]?;
            route.push(current);
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
    graph: Graph,
}

impl Incremental {
    pub fn new(n: usize, edges: &[(usize, usize)]) -> Result<Self, GraphError> {
        let graph = analyze(n, edges)?;
        Ok(Self {
            n,
            edges: edges.to_vec(),
            history: Vec::new(),
            graph,
        })
    }

    pub fn snapshot(&self) -> Graph {
        self.graph.clone()
    }

    pub fn history(&self) -> Vec<Merge> {
        self.history.clone()
    }

    pub fn insert(&mut self, edges: &[(usize, usize)]) -> Result<Batch, GraphError> {
        if self.edges.len() + edges.len() > MAX_EDGES {
            return Err(GraphError::TooManyEdges {
                limit: MAX_EDGES,
            });
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

        for &(from, to) in edges {
            let edge_index = self.edges.len();
            let before = (
                self.graph.component_of[from],
                self.graph.component_of[to],
            );
            let duplicate = self.edges.iter().any(|&(u, v)| u == from && v == to);

            self.edges.push((from, to));
            self.graph = build_graph(self.n, &self.edges);

            let after = (
                self.graph.component_of[from],
                self.graph.component_of[to],
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
                let mut before_ids = Vec::new();
                for v in 0..self.n {
                    let old_id = if v == from {
                        before.0
                    } else if v == to {
                        before.1
                    } else {
                        // Recover the old component ID by analyzing the prefix.
                        // The prefix graph is reconstructed below only when a merge occurs.
                        usize::MAX
                    };
                    if old_id != usize::MAX {
                        before_ids.push(old_id);
                    }
                }
                let old_graph = build_graph(self.n, &self.edges[..edge_index]);
                let mut united = Vec::new();
                for (id, members) in old_graph.components.iter().enumerate() {
                    if self.graph.component_of[members[0]] == after.0 {
                        before_ids.push(id);
                        united.extend_from_slice(members);
                    }
                }
                before_ids.sort_unstable();
                before_ids.dedup();
                united.sort_unstable();

                let event = Merge {
                    edge_index,
                    before: before_ids,
                    after: after.0,
                    members: united,
                };
                merges.push(event.clone());
                self.history.push(event);
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
            graph: self.graph.clone(),
        })
    }
}