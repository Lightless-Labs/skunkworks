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

pub fn analyze(n: usize, edges: &[(usize, usize)]) -> Result<Graph, GraphError> {
    validate(n, edges)?;
    Ok(build_graph(n, edges))
}

fn build_graph(n: usize, edges: &[(usize, usize)]) -> Graph {
    let mut out = vec![Vec::<usize>::new(); n];
    let mut rev = vec![Vec::<usize>::new(); n];
    for &(u, v) in edges {
        out[u].push(v);
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
            if *next < out[*v].len() {
                let w = out[*v][*next];
                *next += 1;
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                let (v, _) = stack.pop().unwrap();
                order.push(v);
            }
        }
    }

    let mut assigned = vec![false; n];
    let mut components = Vec::<Vec<usize>>::new();
    for &start in order.iter().rev() {
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
    let mut component_of = vec![0usize; n];
    for (id, members) in components.iter().enumerate() {
        for &v in members {
            component_of[v] = id;
        }
    }

    let count = components.len();
    let mut cyclic: Vec<bool> = components.iter().map(|c| c.len() >= 2).collect();
    let mut internal_edges = vec![0usize; count];
    let mut arc_data = BTreeMap::<(usize, usize), (usize, usize)>::new();

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

    let mut dag = vec![Vec::<usize>::new(); count];
    let mut indegree = vec![0usize; count];
    for arc in &arcs {
        dag[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    let mut heap = BinaryHeap::<(usize, usize, usize)>::new();
    for id in 0..count {
        if indegree[id] == 0 {
            heap.push((internal_edges[id], components[id][0], id));
        }
    }
    let mut topo = Vec::with_capacity(count);
    let mut depth = vec![0usize; count];
    while let Some((_, _, id)) = heap.pop() {
        topo.push(id);
        for &to in &dag[id] {
            depth[to] = depth[to].max(depth[id] + 1);
            indegree[to] -= 1;
            if indegree[to] == 0 {
                heap.push((internal_edges[to], components[to][0], to));
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

impl Graph {
    pub fn query(&self, from: usize, to: usize) -> Result<Query, GraphError> {
        if from >= self.component_of.len() {
            return Err(GraphError::BadQuery {
                side: 0,
                vertex: from,
            });
        }
        if to >= self.component_of.len() {
            return Err(GraphError::BadQuery { side: 1, vertex: to });
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

fn shortest_route(graph: &Graph, start: usize, target: usize) -> Option<Vec<usize>> {
    let n = graph.components.len();
    let mut adjacency = vec![Vec::<usize>::new(); n];
    for arc in &graph.arcs {
        adjacency[arc.from].push(arc.to);
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
    }

    let mut distance = vec![usize::MAX; n];
    let mut queue = VecDeque::new();
    distance[target] = 0;
    queue.push_back(target);
    while let Some(v) = queue.pop_front() {
        for arc in &graph.arcs {
            if arc.to == v && distance[arc.from] == usize::MAX {
                distance[arc.from] = distance[v] + 1;
                queue.push_back(arc.from);
            }
        }
    }
    if distance[start] == usize::MAX {
        return None;
    }

    let mut route = vec![start];
    let mut current = start;
    while current != target {
        let next = adjacency[current]
            .iter()
            .copied()
            .find(|&v| distance[v].checked_add(1) == Some(distance[current]))?;
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
        let mut occurred: BTreeSet<(usize, usize)> = self.edges.iter().copied().collect();

        for &(u, v) in edges {
            let edge_index = self.edges.len();
            let before_graph = build_graph(self.n, &self.edges);
            let before = (
                before_graph.component_of[u],
                before_graph.component_of[v],
            );
            let duplicate = occurred.contains(&(u, v));
            self.edges.push((u, v));
            occurred.insert((u, v));
            let after_graph = build_graph(self.n, &self.edges);
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
                let after_id = after.0;
                let members = after_graph.components[after_id].clone();
                let member_set: BTreeSet<usize> = members.iter().copied().collect();
                let mut before_ids: Vec<usize> = before_graph
                    .components
                    .iter()
                    .enumerate()
                    .filter_map(|(id, component)| {
                        component.iter().any(|x| member_set.contains(x)).then_some(id)
                    })
                    .collect();
                before_ids.sort_unstable();
                let merge = Merge {
                    edge_index,
                    before: before_ids,
                    after: after_id,
                    members,
                };
                merges.push(merge.clone());
                self.history.push(merge);
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

fn main() {}