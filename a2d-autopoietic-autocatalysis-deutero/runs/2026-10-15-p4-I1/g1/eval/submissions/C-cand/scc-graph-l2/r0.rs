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

    // Iterative Kosaraju pass, recording vertices in finishing order.
    let mut visited = vec![false; n];
    let mut finish_order = Vec::with_capacity(n);
    for start in 0..n {
        if visited[start] {
            continue;
        }

        visited[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((vertex, next_neighbor)) = stack.last_mut() {
            if *next_neighbor < adjacency[*vertex].len() {
                let neighbor = adjacency[*vertex][*next_neighbor];
                *next_neighbor += 1;
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

    let mut assigned = vec![false; n];
    let mut components = Vec::new();
    for &start in finish_order.iter().rev() {
        if assigned[start] {
            continue;
        }

        assigned[start] = true;
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(vertex) = stack.pop() {
            component.push(vertex);
            for &neighbor in &reverse_adjacency[vertex] {
                if !assigned[neighbor] {
                    assigned[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }

    components.sort_by(|a, b| {
        b.len()
            .cmp(&a.len())
            .then_with(|| a[0].cmp(&b[0]))
    });

    let mut component_of = vec![0usize; n];
    let mut cyclic = Vec::with_capacity(components.len());
    for (component_id, component) in components.iter().enumerate() {
        for &vertex in component {
            component_of[vertex] = component_id;
        }
        cyclic.push(component.len() > 1);
    }

    let mut internal_edges = vec![0usize; components.len()];
    let mut arc_data: Vec<Vec<(usize, usize, usize)>> =
        (0..components.len()).map(|_| Vec::new()).collect();

    for (edge_index, &(from, to)) in edges.iter().enumerate() {
        let from_component = component_of[from];
        let to_component = component_of[to];
        if from_component == to_component {
            internal_edges[from_component] += 1;
            if from == to {
                cyclic[from_component] = true;
            }
        } else {
            let outgoing = &mut arc_data[from_component];
            if let Some((_, multiplicity, _)) =
                outgoing.iter_mut().find(|(target, _, _)| *target == to_component)
            {
                *multiplicity += 1;
            } else {
                outgoing.push((to_component, 1, edge_index));
            }
        }
    }

    let mut arcs = Vec::new();
    for (from, outgoing) in arc_data.iter_mut().enumerate() {
        for &(to, multiplicity, first_edge) in outgoing.iter() {
            arcs.push(Arc {
                from,
                to,
                multiplicity,
                first_edge,
            });
        }
    }
    arcs.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| b.to.cmp(&a.to)));

    Ok(Graph {
        components,
        component_of,
        cyclic,
        internal_edges,
        arcs,
    })
}

fn main() {}