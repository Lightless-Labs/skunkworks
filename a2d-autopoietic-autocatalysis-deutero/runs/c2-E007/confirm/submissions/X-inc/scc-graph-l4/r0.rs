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
    validate_endpoints(n, edges)
}

fn validate_endpoints(n: usize, edges: &[(usize, usize)]) -> Result<(), GraphError> {
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

    // Iterative Kosaraju avoids recursion on long directed paths.
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
                let (v, _) = stack.pop().unwrap();
                finish.push(v);
            }
        }
    }

    let mut components = Vec::<Vec<usize>>::new();
    let mut assigned = vec![false; n];
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

    components.sort_by(|a, b| {
        b.len()
            .cmp(&a.len())
            .then_with(|| a[0].cmp(&b[0]))
    });
    let mut component_of = vec![0usize; n];
    for (id, members) in components.iter().enumerate() {
        for &v in members {
            component_of[v] = id;
        }
    }

    let count = components.len();
    let mut cyclic: Vec<bool> = components.iter().map(|c| c.len() > 1).collect();
    let mut internal_edges = vec![0usize; count];
    let mut arc_data = HashMap::<(usize, usize), (usize, usize)>::new();

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
        .map(
            |((from, to), (multiplicity, first_edge))| Arc {
                from,
                to,
                multiplicity,
                first_edge,
            },
        )
        .collect();
    arcs.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| b.to.cmp(&a.to)));

    let mut outgoing = vec![Vec::<usize>::new(); count];
    let mut indegree = vec![0usize; count];
    for arc in &arcs {
        outgoing[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    let mut ready = BinaryHeap::<(usize, usize, Reverse<usize>)>::new();
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

fn shortest_path(graph: &Graph, start: usize, goal: usize) -> Option<Vec<usize>> {
    let count = graph.components.len();
    let mut outgoing = vec![Vec::<usize>::new(); count];
    let mut incoming = vec![Vec::<usize>::new(); count];

    for arc in &graph.arcs {
        outgoing[arc.from].push(arc.to);
        incoming[arc.to].push(arc.from);
    }
    for neighbors in &mut outgoing {
        neighbors.sort_unstable();
    }

    let mut distance = vec![usize::MAX; count];
    let mut queue = VecDeque::new();
    distance[goal] = 0;
    queue.push_back(goal);

    // Reverse BFS finds shortest distances to the goal.
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

    let mut path = vec![start];
    let mut current = start;
    while current != goal {
        let next = outgoing[current]
            .iter()
            .copied()
            .filter(|&v| distance[v] != usize::MAX && distance[v] + 1 == distance[current])
            .min()?;
        path.push(next);
        current = next;
    }
    Some(path)
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
            return if self.cyclic[a] {
                Ok(Query {
                    kind: Relation::Cycle,
                    route: vec![a, a],
                })
            } else {
                Ok(Query {
                    kind: Relation::Trivial,
                    route: vec![a],
                })
            };
        }

        if let Some(route) = shortest_path(self, a, b) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }
        if let Some(mut route) = shortest_path(self, b, a) {
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

    pub fn insert(&mut self, batch_edges: &[(usize, usize)]) -> Result<Batch, GraphError> {
        if self
            .edges
            .len()
            .checked_add(batch_edges.len())
            .map_or(true, |total| total > MAX_EDGES)
        {
            return Err(GraphError::TooManyEdges { limit: MAX_EDGES });
        }
        validate_endpoints(self.n, batch_edges)?;

        let mut steps = Vec::with_capacity(batch_edges.len());
        let mut merges = Vec::new();

        for &(u, v) in batch_edges {
            let edge_index = self.edges.len();
            let old_graph = &self.graph;
            let before = (old_graph.component_of[u], old_graph.component_of[v]);
            let duplicate = self.pairs.contains(&(u, v));

            let mut candidate_edges = self.edges.clone();
            candidate_edges.push((u, v));
            let new_graph = build_graph(self.n, &candidate_edges);
            let after = (new_graph.component_of[u], new_graph.component_of[v]);

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
                for (id, members) in old_graph.components.iter().enumerate() {
                    if members
                        .iter()
                        .any(|&vertex| new_graph.component_of[vertex] == after.0)
                    {
                        before_ids.push(id);
                    }
                }
                before_ids.sort_unstable();
                before_ids.dedup();

                let event = Merge {
                    edge_index,
                    before: before_ids,
                    after: after.0,
                    members: new_graph.components[after.0].clone(),
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
            self.edges.push((u, v));
            self.pairs.insert((u, v));
            self.graph = new_graph;
        }

        Ok(Batch {
            steps,
            merges,
            graph: self.graph.clone(),
        })
    }
}