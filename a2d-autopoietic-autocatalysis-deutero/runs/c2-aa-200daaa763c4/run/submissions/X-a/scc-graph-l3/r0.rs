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

    let mut adj = vec![Vec::<usize>::new(); n];
    let mut rev = vec![Vec::<usize>::new(); n];
    for &(from, to) in edges {
        adj[from].push(to);
        rev[to].push(from);
    }

    // Iterative Kosaraju pass 1.
    let mut seen = vec![false; n];
    let mut finish = Vec::with_capacity(n);
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
                finish.push(v);
            }
        }
    }

    // Iterative Kosaraju pass 2.
    seen.fill(false);
    let mut raw_components = Vec::<Vec<usize>>::new();
    for &root in finish.iter().rev() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut members = Vec::new();
        let mut stack = vec![root];
        while let Some(v) = stack.pop() {
            members.push(v);
            for &w in &rev[v] {
                if !seen[w] {
                    seen[w] = true;
                    stack.push(w);
                }
            }
        }
        members.sort_unstable();
        raw_components.push(members);
    }

    raw_components.sort_by(|a, b| {
        b.len()
            .cmp(&a.len())
            .then_with(|| a[0].cmp(&b[0]))
    });

    let mut component_of = vec![0usize; n];
    for (id, members) in raw_components.iter().enumerate() {
        for &v in members {
            component_of[v] = id;
        }
    }

    let count = raw_components.len();
    let mut cyclic = raw_components
        .iter()
        .map(|members| members.len() > 1)
        .collect::<Vec<_>>();
    let mut internal_edges = vec![0usize; count];
    let mut arc_data = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (i, &(from, to)) in edges.iter().enumerate() {
        let a = component_of[from];
        let b = component_of[to];
        if a == b {
            internal_edges[a] += 1;
            if from == to {
                cyclic[a] = true;
            }
        } else {
            let entry = arc_data.entry((a, b)).or_insert((0, i));
            entry.0 += 1;
            entry.1 = entry.1.min(i);
        }
    }

    let mut arcs = arc_data
        .into_iter()
        .map(|((from, to), (multiplicity, first_edge))| Arc {
            from,
            to,
            multiplicity,
            first_edge,
        })
        .collect::<Vec<_>>();
    arcs.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| b.to.cmp(&a.to)));

    let mut outgoing = vec![Vec::<usize>::new(); count];
    let mut indegree = vec![0usize; count];
    for arc in &arcs {
        outgoing[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    // Priority Kahn: larger internal edge count, then larger minimum vertex.
    let mut ready = std::collections::BinaryHeap::<(usize, usize, usize)>::new();
    for id in 0..count {
        if indegree[id] == 0 {
            ready.push((internal_edges[id], raw_components[id][0], id));
        }
    }

    let mut topo = Vec::with_capacity(count);
    let mut depth = vec![0usize; count];
    while let Some((_, _, v)) = ready.pop() {
        topo.push(v);
        for &w in &outgoing[v] {
            depth[w] = depth[w].max(depth[v] + 1);
            indegree[w] -= 1;
            if indegree[w] == 0 {
                ready.push((internal_edges[w], raw_components[w][0], w));
            }
        }
    }

    Ok(Graph {
        components: raw_components,
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

        let mut outgoing = vec![Vec::<usize>::new(); self.components.len()];
        for arc in &self.arcs {
            outgoing[arc.from].push(arc.to);
        }
        for neighbors in &mut outgoing {
            neighbors.sort_unstable();
            neighbors.dedup();
        }

        fn shortest_lex_route(
            start: usize,
            goal: usize,
            outgoing: &[Vec<usize>],
        ) -> Option<Vec<usize>> {
            let mut parent = vec![None; outgoing.len()];
            let mut seen = vec![false; outgoing.len()];
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
            let mut route = Vec::new();
            let mut current = goal;
            route.push(current);
            while current != start {
                current = parent[current]?;
                route.push(current);
            }
            route.reverse();
            Some(route)
        }

        if let Some(route) = shortest_lex_route(a, b, &outgoing) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }
        if let Some(mut route) = shortest_lex_route(b, a, &outgoing) {
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