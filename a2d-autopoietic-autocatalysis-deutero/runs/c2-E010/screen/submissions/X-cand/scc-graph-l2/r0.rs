pub const MAX_VERTICES: usize = 4096;
pub const MAX_EDGES: usize = 32768;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    TooManyVertices { count: usize, limit: usize },
    TooManyEdges { limit: usize },
    BadEndpoint { edge: usize, side: u8, vertex: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arc {
    pub from: usize,
    pub to: usize,
    pub multiplicity: usize,
    pub first_edge: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Graph {
    pub components: Vec<Vec<usize>>,
    pub component_of: Vec<usize>,
    pub cyclic: Vec<bool>,
    pub internal_edges: Vec<usize>,
    pub arcs: Vec<Arc>,
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

    let mut adj = vec![Vec::new(); n];
    let mut rev = vec![Vec::new(); n];
    for &(from, to) in edges {
        adj[from].push(to);
        rev[to].push(from);
    }

    // Iterative DFS finishing order.
    let mut seen = vec![false; n];
    let mut order = Vec::with_capacity(n);
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
                order.push(v);
            }
        }
    }

    // Reverse-graph traversal assigns provisional SCCs.
    let mut provisional = vec![usize::MAX; n];
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for &root in order.iter().rev() {
        if provisional[root] != usize::MAX {
            continue;
        }
        let id = groups.len();
        let mut group = Vec::new();
        let mut stack = vec![root];
        provisional[root] = id;
        while let Some(v) = stack.pop() {
            group.push(v);
            for &w in &rev[v] {
                if provisional[w] == usize::MAX {
                    provisional[w] = id;
                    stack.push(w);
                }
            }
        }
        group.sort_unstable();
        groups.push(group);
    }

    let mut permutation: Vec<usize> = (0..groups.len()).collect();
    permutation.sort_by(|&a, &b| {
        groups[b]
            .len()
            .cmp(&groups[a].len())
            .then_with(|| groups[a][0].cmp(&groups[b][0]))
    });

    let mut remap = vec![0usize; groups.len()];
    let mut components = Vec::with_capacity(groups.len());
    for (new_id, &old_id) in permutation.iter().enumerate() {
        remap[old_id] = new_id;
        components.push(std::mem::take(&mut groups[old_id]));
    }

    let mut component_of = vec![0usize; n];
    for (id, component) in components.iter().enumerate() {
        for &v in component {
            component_of[v] = id;
        }
    }

    let mut cyclic: Vec<bool> = components
        .iter()
        .map(|component| component.len() >= 2)
        .collect();
    let mut internal_edges = vec![0usize; components.len()];
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
            if i < entry.1 {
                entry.1 = i;
            }
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

    Ok(Graph {
        components,
        component_of,
        cyclic,
        internal_edges,
        arcs,
    })
}
