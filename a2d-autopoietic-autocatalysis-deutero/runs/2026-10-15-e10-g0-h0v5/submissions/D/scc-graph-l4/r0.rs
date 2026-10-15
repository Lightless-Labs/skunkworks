use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
    let mut adj = vec![Vec::new(); n];
    let mut rev = vec![Vec::new(); n];
    for &(u, v) in edges {
        adj[u].push(v);
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
            if next < adj[v].len() {
                stack.push((v, next + 1));
                let w = adj[v][next];
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                finish.push(v);
            }
        }
    }

    let mut raw_of = vec![usize::MAX; n];
    let mut raw_components = Vec::<Vec<usize>>::new();
    for &start in finish.iter().rev() {
        if raw_of[start] != usize::MAX {
            continue;
        }
        let id = raw_components.len();
        raw_of[start] = id;
        let mut members = Vec::new();
        let mut stack = vec![start];
        while let Some(v) = stack.pop() {
            members.push(v);
            for &w in &rev[v] {
                if raw_of[w] == usize::MAX {
                    raw_of[w] = id;
                    stack.push(w);
                }
            }
        }
        members.sort_unstable();
        raw_components.push(members);
    }

    let mut order: Vec<usize> = (0..raw_components.len()).collect();
    order.sort_by(|&a, &b| {
        raw_components[b]
            .len()
            .cmp(&raw_components[a].len())
            .then_with(|| raw_components[a][0].cmp(&raw_components[b][0]))
    });

    let mut remap = vec![0usize; raw_components.len()];
    let mut components = Vec::with_capacity(raw_components.len());
    for (new_id, &old_id) in order.iter().enumerate() {
        remap[old_id] = new_id;
        components.push(raw_components[old_id].clone());
    }

    let mut component_of = vec![0usize; n];
    for v in 0..n {
        component_of[v] = remap[raw_of[v]];
    }

    let count = components.len();
    let mut cyclic = vec![false; count];
    let mut internal_edges = vec![0usize; count];
    let mut arc_map = BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (edge_index, &(u, v)) in edges.iter().enumerate() {
        let a = component_of[u];
        let b = component_of[v];
        if a == b {
            internal_edges[a] += 1;
            if u == v {
                cyclic[a] = true;
            }
        } else {
            let entry = arc_map.entry((a, b)).or_insert((0, edge_index));
            entry.0 += 1;
            entry.1 = entry.1.min(edge_index);
        }
    }
    for (id, members) in components.iter().enumerate() {
        if members.len() > 1 {
            cyclic[id] = true;
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

    let mut outgoing = vec![Vec::new(); count];
    let mut indegree = vec![0usize; count];
    for arc in &arcs {
        outgoing[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    let mut ready = BTreeSet::<(usize, usize, usize)>::new();
    for id in 0..count {
        if indegree[id] == 0 {
            ready.insert((
                internal_edges[id],
                components[id][0],
                id,
            ));
        }
    }

    let mut topo = Vec::with_capacity(count);
    let mut depth = vec![0usize; count];
    while let Some(key) = ready.iter().next_back().copied() {
        ready.remove(&key);
        let id = key.2;
        topo.push(id);
        for &to in &outgoing[id] {
            depth[to] = depth[to].max(depth[id] + 1);
            indegree[to] -= 1;
            if indegree[to] == 0 {
                ready.insert((
                    internal_edges[to],
                    components[to][0],
                    to,
                ));
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

        if let Some(route) = shortest_lex_route(self, a, b) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }
        if let Some(mut route) = shortest_lex_route(self, b, a) {
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

fn shortest_lex_route(graph: &Graph, start: usize, goal: usize) -> Option<Vec<usize>> {
    let n = graph.components.len();
    let mut outgoing = vec![Vec::new(); n];
    for arc in &graph.arcs {
        outgoing[arc.from].push(arc.to);
    }
    for neighbors in &mut outgoing {
        neighbors.sort_unstable();
        neighbors.dedup();
    }

    let mut distance = vec![usize::MAX; n];
    let mut queue = VecDeque::new();
    distance[goal] = 0;
    queue.push_back(goal);

    let mut incoming = vec![Vec::new(); n];
    for arc in &graph.arcs {
        incoming[arc.to].push(arc.from);
    }
    while let Some(v) = queue.pop_front() {
        for &p in &incoming[v] {
            if distance[p] == usize::MAX {
                distance[p] = distance[v] + 1;
                queue.push_back(p);
            }
        }
    }
    if distance[start] == usize::MAX {
        return None;
    }

    let mut route = vec![start];
    let mut current = start;
    while current != goal {
        let next = outgoing[current]
            .iter()
            .copied()
            .filter(|&v| distance[v].checked_add(1) == Some(distance[current]))
            .min()?;
        route.push(next);
        current = next;
    }
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
            let before_graph = build_graph(self.n, &self.edges);
            let before_pair = (
                before_graph.component_of[from],
                before_graph.component_of[to],
            );
            let duplicate = self.edges.iter().any(|&(u, v)| u == from && v == to);

            self.edges.push((from, to));
            let after_graph = build_graph(self.n, &self.edges);
            let after_pair = (
                after_graph.component_of[from],
                after_graph.component_of[to],
            );

            let status = if duplicate {
                EdgeStatus::Duplicate
            } else if before_pair.0 == before_pair.1 {
                EdgeStatus::Internal
            } else if after_pair.0 == after_pair.1 {
                EdgeStatus::Merge
            } else {
                EdgeStatus::Link
            };

            if status == EdgeStatus::Merge {
                let mut before_ids = BTreeSet::new();
                for &vertex in &after_graph.components[after_pair.0] {
                    before_ids.insert(before_graph.component_of[vertex]);
                }
                let merge = Merge {
                    edge_index,
                    before: before_ids.into_iter().collect(),
                    after: after_pair.0,
                    members: after_graph.components[after_pair.0].clone(),
                };
                self.history.push(merge.clone());
                merges.push(merge);
            }

            steps.push(Step {
                edge_index,
                status,
                before: before_pair,
                after: after_pair,
            });
        }

        Ok(Batch {
            steps,
            merges,
            graph: self.snapshot(),
        })
    }
}