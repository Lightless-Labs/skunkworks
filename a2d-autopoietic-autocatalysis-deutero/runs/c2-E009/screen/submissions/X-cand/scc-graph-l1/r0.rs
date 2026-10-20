pub const MAX_VERTICES: usize = 4096;
pub const MAX_EDGES: usize = 32768;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    TooManyVertices { count: usize, limit: usize },
    TooManyEdges { limit: usize },
    BadEndpoint { edge: usize, side: u8, vertex: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Graph {
    pub components: Vec<Vec<usize>>,
    pub component_of: Vec<usize>,
    pub cyclic: Vec<bool>,
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

    for (edge_index, &(source, target)) in edges.iter().enumerate() {
        if source >= n {
            return Err(GraphError::BadEndpoint {
                edge: edge_index,
                side: 0,
                vertex: source,
            });
        }
        if target >= n {
            return Err(GraphError::BadEndpoint {
                edge: edge_index,
                side: 1,
                vertex: target,
            });
        }
    }

    let mut adjacency = vec![Vec::new(); n];
    let mut reverse_adjacency = vec![Vec::new(); n];
    let mut has_self_loop = vec![false; n];

    for &(source, target) in edges {
        adjacency[source].push(target);
        reverse_adjacency[target].push(source);
        if source == target {
            has_self_loop[source] = true;
        }
    }

    // Iterative first pass of Kosaraju's algorithm.
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

    // Second pass assigns each vertex to a strongly connected component.
    visited.fill(false);
    let mut raw_components = Vec::new();

    for &start in finish_order.iter().rev() {
        if visited[start] {
            continue;
        }

        let mut component = Vec::new();
        let mut stack = vec![start];
        visited[start] = true;

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

    raw_components.sort_by(|left, right| {
        right
            .len()
            .cmp(&left.len())
            .then_with(|| left[0].cmp(&right[0]))
    });

    let mut component_of = vec![0; n];
    let mut cyclic = Vec::new();

    for (component_id, component) in raw_components.iter().enumerate() {
        for &vertex in component {
            component_of[vertex] = component_id;
        }
        cyclic.push(component.len() >= 2 || has_self_loop[component[0]]);
    }

    Ok(Graph {
        components: raw_components,
        component_of,
        cyclic,
    })
}