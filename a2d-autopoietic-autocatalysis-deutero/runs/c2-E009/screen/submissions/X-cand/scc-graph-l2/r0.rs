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

    let mut adjacency = vec![Vec::new(); n];
    let mut reverse_adjacency = vec![Vec::new(); n];

    for &(from, to) in edges {
        adjacency[from].push(to);
        reverse_adjacency[to].push(from);
    }

    let mut visited = vec![false; n];
    let mut finish_order = Vec::new();

    for start in 0..n {
        if visited[start] {
            continue;
        }

        visited[start] = true;
        let mut stack = vec![(start, 0usize)];

        while let Some((vertex, next_edge)) = stack.last_mut() {
            if *next_edge < adjacency[*vertex].len() {
                let neighbor = adjacency[*vertex][*next_edge];
                *next_edge += 1;

                if !visited[neighbor] {
                    visited[neighbor] = true;
                    stack.push((neighbor, 0));
                }
            } else {
                let (finished, _) = stack.pop().unwrap();
                finish_order.push(finished);
            }
        }
    }

    visited.fill(false);
    let mut raw_components = Vec::new();

    for &start in finish_order.iter().rev() {
        if visited[start] {
            continue;
        }

        visited[start] = true;
        let mut component = Vec::new();
        let mut stack = vec![start];

        while let Some(vertex) = stack.pop() {
            component.push(vertex);

            for &neighbor in &reverse_adjacency[vertex] {
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }

        component.sort_unstable();
        raw_components.push(component);
    }

    raw_components.sort_by(|a, b| {
        b.len()
            .cmp(&a.len())
            .then_with(|| a[0].cmp(&b[0]))
    });

    let mut component_of = vec![0; n];
    for (component_id, component) in raw_components.iter().enumerate() {
        for &vertex in component {
            component_of[vertex] = component_id;
        }
    }

    let mut cyclic = Vec::with_capacity(raw_components.len());
    let mut internal_edges = vec![0; raw_components.len()];
    for component in &raw_components {
        cyclic.push(component.len() >= 2);
    }

    let mut arc_counts: std::collections::BTreeMap<(usize, usize), (usize, usize)> =
        std::collections::BTreeMap::new();

    for (edge_index, &(from, to)) in edges.iter().enumerate() {
        let from_component = component_of[from];
        let to_component = component_of[to];

        if from_component == to_component {
            internal_edges[from_component] += 1;
            if from == to {
                cyclic[from_component] = true;
            }
        } else {
            let entry = arc_counts
                .entry((from_component, to_component))
                .or_insert((0, edge_index));
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