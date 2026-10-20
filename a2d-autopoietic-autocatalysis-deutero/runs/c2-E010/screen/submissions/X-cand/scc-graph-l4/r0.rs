use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

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

// Iterative Kosaraju SCC decomposition.
fn scc(n: usize, edges: &[(usize, usize)]) -> (Vec<Vec<usize>>, Vec<usize>) {
    let mut out = vec![Vec::new(); n];
    let mut rev = vec![Vec::new(); n];
    for &(u, v) in edges {
        out[u].push(v);
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
            if *next < out[*v].len() {
                let w = out[*v][*next];
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

    let mut raw_of = vec![usize::MAX; n];
    let mut raw = Vec::<Vec<usize>>::new();
    for &root in finish.iter().rev() {
        if raw_of[root] != usize::MAX {
            continue;
        }
        let id = raw.len();
        let mut members = Vec::new();
        let mut stack = vec![root];
        raw_of[root] = id;
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
        raw.push(members);
    }

    let mut order: Vec<usize> = (0..raw.len()).collect();
    order.sort_by_key(|&i| (Reverse(raw[i].len()), raw[i][0]));
    let mut canonical_of_raw = vec![0; raw.len()];
    let mut components = Vec::with_capacity(raw.len());
    for (new_id, old_id) in order.into_iter().enumerate() {
        canonical_of_raw[old_id] = new_id;
        components.push(std::mem::take(&mut raw[old_id]));
    }
    let component_of = raw_of
        .into_iter()
        .map(|id| canonical_of_raw[id])
        .collect();
    (components, component_of)
}

fn build_graph(n: usize, edges: &[(usize, usize)]) -> Graph {
    let (components, component_of) = scc(n, edges);
    let k = components.len();
    let mut cyclic = vec![false; k];
    let mut internal_edges = vec![0usize; k];
    let mut arc_data: HashMap<(usize, usize), (usize, usize)> = HashMap::new();

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
        }
    }
    for (id, members) in components.iter().enumerate() {
        if members.len() > 1 {
            cyclic[id] = true;
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
    arcs.sort_by_key(|a| (a.from, Reverse(a.to)));

    let mut outgoing = vec![Vec::new(); k];
    let mut indegree = vec![0usize; k];
    for a in &arcs {
        outgoing[a.from].push(a.to);
        indegree[a.to] += 1;
    }
    let mut ready = BinaryHeap::new();
    for id in 0..k {
        if indegree[id] == 0 {
            ready.push((internal_edges[id], components[id][0], Reverse(id)));
        }
    }
    let mut topo = Vec::with_capacity(k);
    let mut depth = vec![0usize; k];
    while let Some((_, _, Reverse(id))) = ready.pop() {
        topo.push(id);
        for &to in &outgoing[id] {
            depth[to] = depth[to].max(depth[id] + 1);
            indegree[to] -= 1;
            if indegree[to] == 0 {
                ready.push((internal_edges[to], components[to][0], Reverse(to)));
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

fn shortest_route(g: &Graph, start: usize, target: usize) -> Option<Vec<usize>> {
    if start == target {
        return Some(vec![start]);
    }
    let k = g.components.len();
    let mut outgoing = vec![Vec::new(); k];
    for a in &g.arcs {
        outgoing[a.from].push(a.to);
    }
    for list in &mut outgoing {
        list.sort_unstable();
        list.dedup();
    }

    let mut dist = vec![usize::MAX; k];
    let mut q = VecDeque::new();
    dist[target] = 0;
    q.push_back(target);
    let mut incoming = vec![Vec::new(); k];
    for a in &g.arcs {
        incoming[a.to].push(a.from);
    }
    while let Some(v) = q.pop_front() {
        for &p in &incoming[v] {
            if dist[p] == usize::MAX {
                dist[p] = dist[v] + 1;
                q.push_back(p);
            }
        }
    }
    if dist[start] == usize::MAX {
        return None;
    }
    let mut route = vec![start];
    let mut cur = start;
    while cur != target {
        let next = outgoing[cur]
            .iter()
            .copied()
            .filter(|&v| dist[v].checked_add(1) == Some(dist[cur]))
            .min()
            .expect("distance invariant");
        route.push(next);
        cur = next;
    }
    Some(route)
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
            return Ok(if self.cyclic[a] {
                Query {
                    kind: Relation::Cycle,
                    route: vec![a, a],
                }
            } else {
                Query {
                    kind: Relation::Trivial,
                    route: vec![a],
                }
            });
        }
        if let Some(route) = shortest_route(self, a, b) {
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

#[derive(Clone, Debug)]
pub struct Incremental {
    n: usize,
    edges: Vec<(usize, usize)>,
    graph: Graph,
    history: Vec<Merge>,
    pairs: HashSet<(usize, usize)>,
}

impl Incremental {
    pub fn new(n: usize, edges: &[(usize, usize)]) -> Result<Self, GraphError> {
        let graph = analyze(n, edges)?;
        Ok(Self {
            n,
            edges: edges.to_vec(),
            graph,
            history: Vec::new(),
            pairs: edges.iter().copied().collect(),
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
            let before_graph = self.graph.clone();
            let before = (before_graph.component_of[u], before_graph.component_of[v]);
            let duplicate = self.pairs.contains(&(u, v));
            self.edges.push((u, v));
            self.pairs.insert((u, v));
            self.graph = build_graph(self.n, &self.edges);
            let after = (self.graph.component_of[u], self.graph.component_of[v]);

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
                let members = self.graph.components[after.0].clone();
                let mut before_ids = HashSet::new();
                for &vertex in &members {
                    before_ids.insert(before_graph.component_of[vertex]);
                }
                let mut before_ids: Vec<usize> = before_ids.into_iter().collect();
                before_ids.sort_unstable();
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
            graph: self.graph.clone(),
        })
    }
}
