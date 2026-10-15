use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

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

pub fn analyze(n: usize, edges: &[(usize, usize)]) -> Result<Graph, GraphError> {
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

    let mut adjacency = vec![Vec::new(); n];
    let mut reverse = vec![Vec::new(); n];
    for &(from, to) in edges {
        adjacency[from].push(to);
        reverse[to].push(from);
    }

    // Iterative Kosaraju avoids recursion on long paths.
    let mut seen = vec![false; n];
    let mut finish = Vec::with_capacity(n);
    for start in 0..n {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((v, next_index)) = stack.pop() {
            if next_index < adjacency[v].len() {
                stack.push((v, next_index + 1));
                let w = adjacency[v][next_index];
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                finish.push(v);
            }
        }
    }

    seen.fill(false);
    let mut raw_components = Vec::<Vec<usize>>::new();
    for &start in finish.iter().rev() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(v) = stack.pop() {
            component.push(v);
            for &w in &reverse[v] {
                if !seen[w] {
                    seen[w] = true;
                    stack.push(w);
                }
            }
        }
        component.sort_unstable();
        raw_components.push(component);
    }

    raw_components.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    let components = raw_components;
    let mut component_of = vec![0usize; n];
    for (id, component) in components.iter().enumerate() {
        for &v in component {
            component_of[v] = id;
        }
    }

    let count = components.len();
    let mut cyclic = vec![false; count];
    for (id, component) in components.iter().enumerate() {
        cyclic[id] = component.len() > 1;
    }
    let mut internal_edges = vec![0usize; count];
    let mut arc_data = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (edge_index, &(from, to)) in edges.iter().enumerate() {
        let a = component_of[from];
        let b = component_of[to];
        if a == b {
            internal_edges[a] += 1;
            if from == to {
                cyclic[a] = true;
            }
        } else {
            let entry = arc_data.entry((a, b)).or_insert((0, edge_index));
            entry.0 += 1;
            entry.1 = entry.1.min(edge_index);
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

    // Reverse heap ordering gives greatest internal edge count, then greatest
    // smallest vertex, at the top of the min-oriented BinaryHeap.
    let mut ready = BinaryHeap::new();
    for id in 0..count {
        if indegree[id] == 0 {
            ready.push((
                internal_edges[id],
                components[id][0],
                Reverse(id),
            ));
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
                ready.push((
                    internal_edges[w],
                    components[w][0],
                    Reverse(w),
                ));
            }
        }
    }

    Ok(Graph {
        components,
        component_of,
        cyclic,
        internal_edges,
        arcs,
        topo,
        depth,
    })
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

        let count = self.components.len();
        let mut outgoing = vec![Vec::new(); count];
        for arc in &self.arcs {
            outgoing[arc.from].push(arc.to);
        }
        for neighbors in &mut outgoing {
            neighbors.sort_unstable();
        }

        fn shortest_lex_route(
            outgoing: &[Vec<usize>],
            start: usize,
            target: usize,
        ) -> Option<Vec<usize>> {
            let mut distance = vec![usize::MAX; outgoing.len()];
            let mut queue = VecDeque::new();
            distance[target] = 0;
            queue.push_back(target);

            // Distances from each node to target, using reversed edges.
            let mut reverse = vec![Vec::new(); outgoing.len()];
            for (v, neighbors) in outgoing.iter().enumerate() {
                for &w in neighbors {
                    reverse[w].push(v);
                }
            }
            while let Some(v) = queue.pop_front() {
                for &p in &reverse[v] {
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
            while current != target {
                let next = outgoing[current]
                    .iter()
                    .copied()
                    .filter(|&w| distance[w] + 1 == distance[current])
                    .min()?;
                route.push(next);
                current = next;
            }
            Some(route)
        }

        if let Some(route) = shortest_lex_route(&outgoing, a, b) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }
        if let Some(mut route) = shortest_lex_route(&outgoing, b, a) {
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