use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};

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

    // Iterative Kosaraju traversal avoids recursion on long paths.
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

    seen.fill(false);
    let mut components = Vec::new();
    for &root in order.iter().rev() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut component = Vec::new();
        let mut stack = vec![root];
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

fn build_graph(n: usize, edges: &[(usize, usize)]) -> Graph {
    let components = strongly_connected_components(n, edges);
    let count = components.len();
    let mut component_of = vec![0usize; n];
    for (id, component) in components.iter().enumerate() {
        for &v in component {
            component_of[v] = id;
        }
    }

    let mut cyclic: Vec<bool> = components.iter().map(|c| c.len() >= 2).collect();
    let mut internal_edges = vec![0usize; count];
    let mut arc_map: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();

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

    // BinaryHeap is a max heap: prioritize more internal edges, then larger
    // smallest original vertex. The final tie-break is only defensive.
    let mut ready = BinaryHeap::new();
    for id in 0..count {
        if indegree[id] == 0 {
            ready.push((internal_edges[id], components[id][0], Reverse(id)));
        }
    }

    let mut topo = Vec::with_capacity(count);
    let mut depth = vec![0usize; count];
    while let Some((_, _, Reverse(v))) = ready.pop() {
        topo.push(v);
        for &w in &outgoing[v] {
            depth[w] = depth[w].max(depth[v] + 1);
            indegree[w] -= 1;
            if indegree[w] == 0 {
                ready.push((internal_edges[w], components[w][0], Reverse(w)));
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
    }

    let mut distance = vec![usize::MAX; n];
    let mut queue = VecDeque::new();
    distance[goal] = 0;
    queue.push_back(goal);

    let mut incoming = vec![Vec::new(); n];
    for (u, neighbors) in outgoing.iter().enumerate() {
        for &v in neighbors {
            incoming[v].push(u);
        }
    }

    while let Some(v) = queue.pop_front() {
        for &u in &incoming[v] {
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

    pub fn insert(&mut self, batch_edges: &[(usize, usize)]) -> Result<Batch, GraphError> {
        if self.edges.len() + batch_edges.len() > MAX_EDGES {
            return Err(GraphError::TooManyEdges { limit: MAX_EDGES });
        }
        for (i, &(from, to)) in batch_edges.iter().enumerate() {
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

        let mut steps = Vec::with_capacity(batch_edges.len());
        let mut merges = Vec::new();
        let mut seen_pairs: BTreeSet<(usize, usize)> = self.edges.iter().copied().collect();

        for &(from, to) in batch_edges {
            let edge_index = self.edges.len();
            let before_graph = build_graph(self.n, &self.edges);
            let before = (
                before_graph.component_of[from],
                before_graph.component_of[to],
            );
            let duplicate = seen_pairs.contains(&(from, to));

            self.edges.push((from, to));
            seen_pairs.insert((from, to));
            let after_graph = build_graph(self.n, &self.edges);
            let after = (
                after_graph.component_of[from],
                after_graph.component_of[to],
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
                let mut before_ids = BTreeSet::new();
                for &vertex in &members {
                    before_ids.insert(before_graph.component_of[vertex]);
                }
                let event = Merge {
                    edge_index,
                    before: before_ids.into_iter().collect(),
                    after: merged_id,
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