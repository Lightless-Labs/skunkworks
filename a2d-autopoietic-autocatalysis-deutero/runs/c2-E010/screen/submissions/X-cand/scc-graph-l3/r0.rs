use std::collections::{BTreeSet, VecDeque};

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

fn postorder(adj: &[Vec<usize>]) -> Vec<usize> {
    let mut seen = vec![false; adj.len()];
    let mut result = Vec::with_capacity(adj.len());
    for root in 0..adj.len() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0usize)];
        while !stack.is_empty() {
            let last = stack.len() - 1;
            let v = stack[last].0;
            if stack[last].1 < adj[v].len() {
                let w = adj[v][stack[last].1];
                stack[last].1 += 1;
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                result.push(v);
                stack.pop();
            }
        }
    }
    result
}

pub fn analyze(n: usize, edges: &[(usize, usize)]) -> Result<Graph, GraphError> {
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

    let mut adj = vec![Vec::new(); n];
    let mut rev = vec![Vec::new(); n];
    for &(u, v) in edges {
        adj[u].push(v);
        rev[v].push(u);
    }

    let order = postorder(&adj);
    let mut visited = vec![false; n];
    let mut groups = Vec::new();
    for &root in order.iter().rev() {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut stack = vec![root];
        let mut group = Vec::new();
        while let Some(v) = stack.pop() {
            group.push(v);
            for &w in &rev[v] {
                if !visited[w] {
                    visited[w] = true;
                    stack.push(w);
                }
            }
        }
        group.sort_unstable();
        groups.push(group);
    }
    groups.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));

    let mut component_of = vec![0usize; n];
    for (id, group) in groups.iter().enumerate() {
        for &v in group {
            component_of[v] = id;
        }
    }

    let k = groups.len();
    let mut cyclic: Vec<bool> = groups.iter().map(|g| g.len() > 1).collect();
    let mut internal_edges = vec![0usize; k];
    let mut pair_info = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();
    for (edge_index, &(u, v)) in edges.iter().enumerate() {
        let from = component_of[u];
        let to = component_of[v];
        if from == to {
            internal_edges[from] += 1;
            if u == v {
                cyclic[from] = true;
            }
        } else {
            let entry = pair_info.entry((from, to)).or_insert((0, edge_index));
            entry.0 += 1;
        }
    }

    let mut arcs: Vec<Arc> = pair_info
        .into_iter()
        .map(|((from, to), (multiplicity, first_edge))| Arc {
            from,
            to,
            multiplicity,
            first_edge,
        })
        .collect();
    arcs.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| b.to.cmp(&a.to)));

    let mut out = vec![Vec::<usize>::new(); k];
    let mut indegree = vec![0usize; k];
    for arc in &arcs {
        out[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    let mut ready = BTreeSet::<(usize, usize, usize)>::new();
    for id in 0..k {
        if indegree[id] == 0 {
            ready.insert((internal_edges[id], groups[id][0], id));
        }
    }
    let mut topo = Vec::with_capacity(k);
    let mut depth = vec![0usize; k];
    while let Some(&(internal_count, min_vertex, id)) = ready.iter().next_back() {
        ready.remove(&(internal_count, min_vertex, id));
        topo.push(id);
        for &next in &out[id] {
            depth[next] = depth[next].max(depth[id] + 1);
            indegree[next] -= 1;
            if indegree[next] == 0 {
                ready.insert((internal_edges[next], groups[next][0], next));
            }
        }
    }

    Ok(Graph {
        components: groups,
        component_of,
        cyclic,
        internal_edges,
        arcs,
        topo,
        depth,
    })
}

impl Graph {
    fn shortest(&self, start: usize, goal: usize) -> Option<Vec<usize>> {
        let k = self.components.len();
        let mut rev = vec![Vec::<usize>::new(); k];
        let mut out = vec![Vec::<usize>::new(); k];
        for arc in &self.arcs {
            out[arc.from].push(arc.to);
            rev[arc.to].push(arc.from);
        }
        let mut dist = vec![usize::MAX; k];
        let mut queue = VecDeque::new();
        dist[goal] = 0;
        queue.push_back(goal);
        while let Some(v) = queue.pop_front() {
            for &p in &rev[v] {
                if dist[p] == usize::MAX {
                    dist[p] = dist[v] + 1;
                    queue.push_back(p);
                }
            }
        }
        if dist[start] == usize::MAX {
            return None;
        }
        for neighbors in &mut out {
            neighbors.sort_unstable();
            neighbors.dedup();
        }
        let mut route = vec![start];
        let mut at = start;
        while at != goal {
            let next = out[at]
                .iter()
                .copied()
                .find(|&w| dist[w] != usize::MAX && dist[w] + 1 == dist[at])?;
            route.push(next);
            at = next;
        }
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
            return Ok(Query {
                kind: if self.cyclic[a] {
                    Relation::Cycle
                } else {
                    Relation::Trivial
                },
                route: if self.cyclic[a] {
                    vec![a, a]
                } else {
                    vec![a]
                },
            });
        }
        if let Some(route) = self.shortest(a, b) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }
        if let Some(mut route) = self.shortest(b, a) {
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
