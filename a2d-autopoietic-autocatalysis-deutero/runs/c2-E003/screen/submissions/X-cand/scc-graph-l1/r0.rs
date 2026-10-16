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
        return Err(GraphError::TooManyEdges {
            limit: MAX_EDGES,
        });
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
    let mut reverse = vec![Vec::new(); n];
    let mut self_loop = vec![false; n];

    for &(source, target) in edges {
        adjacency[source].push(target);
        reverse[target].push(source);
        if source == target {
            self_loop[source] = true;
        }
    }

    // Iterative Kosaraju pass one: compute finishing order.
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);
    for start in 0..n {
        if visited[start] {
            continue;
        }

        visited[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((vertex, next_edge)) = stack.pop() {
            if next_edge < adjacency[vertex].len() {
                stack.push((vertex, next_edge + 1));
                let next = adjacency[vertex][next_edge];
                if !visited[next] {
                    visited[next] = true;
                    stack.push((next, 0));
                }
            } else {
                order.push(vertex);
            }
        }
    }

    // Iterative Kosaraju pass two: collect components in reverse finishing order.
    let mut assigned = vec![false; n];
    let mut components = Vec::new();
    for &start in order.iter().rev() {
        if assigned[start] {
            continue;
        }

        assigned[start] = true;
        let mut stack = vec![start];
        let mut component = Vec::new();
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
        components.push(component);
    }

    components.sort_unstable_by(|a, b| {
        b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0]))
    });

    let mut component_of = vec![0usize; n];
    let mut cyclic = Vec::with_capacity(components.len());

    for (component_id, component) in components.iter().enumerate() {
        for &vertex in component {
            component_of[vertex] = component_id;
        }
        cyclic.push(
            component.len() >= 2 || component.iter().any(|&vertex| self_loop[vertex]),
        );
    }

    Ok(Graph {
        components,
        component_of,
        cyclic,
    })
}