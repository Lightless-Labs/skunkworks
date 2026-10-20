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

    // Iterative Kosaraju first pass: record vertices in finish order.
    let mut seen = vec![false; n];
    let mut finish = Vec::new();
    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0usize)];
        while let Some((v, next_index)) = stack.last_mut() {
            if *next_index < adjacency[*v].len() {
                let next = adjacency[*v][*next_index];
                *next_index += 1;
                if !seen[next] {
                    seen[next] = true;
                    stack.push((next, 0));
                }
            } else {
                let (done, _) = stack.pop().unwrap();
                finish.push(done);
            }
        }
    }

    // Second pass assigns provisional SCC IDs.
    let mut provisional_of = vec![usize::MAX; n];
    let mut provisional_components: Vec<Vec<usize>> = Vec::new();
    for &root in finish.iter().rev() {
        if provisional_of[root] != usize::MAX {
            continue;
        }
        let id = provisional_components.len();
        let mut component = Vec::new();
        let mut stack = vec![root];
        provisional_of[root] = id;
        while let Some(v) = stack.pop() {
            component.push(v);
            for &next in &reverse[v] {
                if provisional_of[next] == usize::MAX {
                    provisional_of[next] = id;
                    stack.push(next);
                }
            }
        }
        component.sort_unstable();
        provisional_components.push(component);
    }

    // Canonical component order: size descending, minimum vertex ascending.
    let mut order: Vec<usize> = (0..provisional_components.len()).collect();
    order.sort_by(|&a, &b| {
        provisional_components[b]
            .len()
            .cmp(&provisional_components[a].len())
            .then_with(|| provisional_components[a][0].cmp(&provisional_components[b][0]))
    });

    let mut remap = vec![0usize; order.len()];
    let mut components = Vec::new();
    for (new_id, &old_id) in order.iter().enumerate() {
        remap[old_id] = new_id;
        components.push(provisional_components[old_id].clone());
    }
    let component_of: Vec<usize> = provisional_of
        .iter()
        .map(|&old_id| remap[old_id])
        .collect();

    let count = components.len();
    let mut cyclic: Vec<bool> = components.iter().map(|c| c.len() >= 2).collect();
    let mut internal_edges = vec![0usize; count];
    let mut pair_data = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (edge_index, &(from, to)) in edges.iter().enumerate() {
        let a = component_of[from];
        let b = component_of[to];
        if a == b {
            internal_edges[a] += 1;
            if from == to {
                cyclic[a] = true;
            }
        } else {
            let entry = pair_data.entry((a, b)).or_insert((0, edge_index));
            entry.0 += 1;
            entry.1 = entry.1.min(edge_index);
        }
    }

    let mut arcs: Vec<Arc> = pair_data
        .into_iter()
        .map(|((from, to), (multiplicity, first_edge))| Arc {
            from,
            to,
            multiplicity,
            first_edge,
        })
        .collect();
    arcs.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| b.to.cmp(&a.to)));

    let mut dag = vec![Vec::new(); count];
    let mut indegree = vec![0usize; count];
    for arc in &arcs {
        dag[arc.from].push(arc.to);
        indegree[arc.to] += 1;
    }

    // Priority Kahn traversal. Heap entries are (internal edge count,
    // minimum vertex, component ID); max-heap implements both greatest-first
    // tie-breaks.
    let mut ready = std::collections::BinaryHeap::<(usize, usize, usize)>::new();
    for id in 0..count {
        if indegree[id] == 0 {
            ready.push((internal_edges[id], components[id][0], id));
        }
    }

    let mut topo = Vec::new();
    let mut depth = vec![0usize; count];
    while let Some((_, _, id)) = ready.pop() {
        topo.push(id);
        for &next in &dag[id] {
            depth[next] = depth[next].max(depth[id] + 1);
            indegree[next] -= 1;
            if indegree[next] == 0 {
                ready.push((internal_edges[next], components[next][0], next));
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

        let count = self.components.len();
        let mut dag = vec![Vec::new(); count];
        for arc in &self.arcs {
            dag[arc.from].push(arc.to);
        }
        for outgoing in &mut dag {
            outgoing.sort_unstable();
            outgoing.dedup();
        }

        fn shortest_lex_route(dag: &[Vec<usize>], start: usize, goal: usize) -> Option<Vec<usize>> {
            let mut parent = vec![None; dag.len()];
            let mut seen = vec![false; dag.len()];
            let mut queue = std::collections::VecDeque::new();
            seen[start] = true;
            queue.push_back(start);

            while let Some(v) = queue.pop_front() {
                if v == goal {
                    break;
                }
                for &next in &dag[v] {
                    if !seen[next] {
                        seen[next] = true;
                        parent[next] = Some(v);
                        queue.push_back(next);
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

        if let Some(route) = shortest_lex_route(&dag, a, b) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }
        if let Some(mut route) = shortest_lex_route(&dag, b, a) {
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