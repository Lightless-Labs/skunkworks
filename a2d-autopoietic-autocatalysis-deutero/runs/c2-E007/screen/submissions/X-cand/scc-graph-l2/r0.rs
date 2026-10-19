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
    for (edge, &(from, to)) in edges.iter().enumerate() {
        if from >= n {
            return Err(GraphError::BadEndpoint {
                edge,
                side: 0,
                vertex: from,
            });
        }
        if to >= n {
            return Err(GraphError::BadEndpoint {
                edge,
                side: 1,
                vertex: to,
            });
        }
    }

    // Iterative Kosaraju traversal avoids recursion on long paths.
    let mut adjacency = vec![Vec::<usize>::new(); n];
    let mut reverse = vec![Vec::<usize>::new(); n];
    for &(from, to) in edges {
        adjacency[from].push(to);
        reverse[to].push(from);
    }

    let mut seen = vec![false; n];
    let mut finish_order = Vec::with_capacity(n);
    for start in 0..n {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((vertex, next_index)) = stack.last_mut() {
            if *next_index < adjacency[*vertex].len() {
                let next = adjacency[*vertex][*next_index];
                *next_index += 1;
                if !seen[next] {
                    seen[next] = true;
                    stack.push((next, 0));
                }
            } else {
                let (finished, _) = stack.pop().unwrap();
                finish_order.push(finished);
            }
        }
    }

    let mut raw_components = Vec::<Vec<usize>>::new();
    let mut assigned = vec![false; n];
    for &start in finish_order.iter().rev() {
        if assigned[start] {
            continue;
        }
        assigned[start] = true;
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(vertex) = stack.pop() {
            component.push(vertex);
            for &next in &reverse[vertex] {
                if !assigned[next] {
                    assigned[next] = true;
                    stack.push(next);
                }
            }
        }
        component.sort_unstable();
        raw_components.push(component);
    }

    raw_components.sort_by(|a, b| {
        b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0]))
    });

    let mut component_of = vec![0usize; n];
    for (component_id, component) in raw_components.iter().enumerate() {
        for &vertex in component {
            component_of[vertex] = component_id;
        }
    }

    let component_count = raw_components.len();
    let mut cyclic: Vec<bool> = raw_components
        .iter()
        .map(|component| component.len() >= 2)
        .collect();
    let mut internal_edges = vec![0usize; component_count];
    let mut arc_counts = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (edge_index, &(from_vertex, to_vertex)) in edges.iter().enumerate() {
        let from = component_of[from_vertex];
        let to = component_of[to_vertex];
        if from == to {
            internal_edges[from] += 1;
            if from_vertex == to_vertex {
                cyclic[from] = true;
            }
        } else {
            let entry = arc_counts.entry((from, to)).or_insert((0, edge_index));
            entry.0 += 1;
            if edge_index < entry.1 {
                entry.1 = edge_index;
            }
        }
    }

    let mut arcs: Vec<Arc> = arc_counts
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
        components: raw_components,
        component_of,
        cyclic,
        internal_edges,
        arcs,
    })
}