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

    for (index, &(source, target)) in edges.iter().enumerate() {
        if source >= n {
            return Err(GraphError::BadEndpoint {
                edge: index,
                side: 0,
                vertex: source,
            });
        }
        if target >= n {
            return Err(GraphError::BadEndpoint {
                edge: index,
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

    // Iterative DFS finishing order avoids recursion limits on long paths.
    let mut visited = vec![false; n];
    let mut finishing_order = Vec::with_capacity(n);

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
                finishing_order.push(finished);
            }
        }
    }

    let mut assigned = vec![false; n];
    let mut components = Vec::new();
    let mut component_has_self_loop = Vec::new();

    for &start in finishing_order.iter().rev() {
        if assigned[start] {
            continue;
        }

        assigned[start] = true;
        let mut stack = vec![start];
        let mut component = Vec::new();
        let mut contains_self_loop = false;

        while let Some(vertex) = stack.pop() {
            component.push(vertex);
            contains_self_loop |= has_self_loop[vertex];

            for &neighbor in &reverse_adjacency[vertex] {
                if !assigned[neighbor] {
                    assigned[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }

        component.sort_unstable();
        components.push(component);
        component_has_self_loop.push(contains_self_loop);
    }

    let mut order: Vec<usize> = (0..components.len()).collect();
    order.sort_unstable_by(|&a, &b| {
        components[b]
            .len()
            .cmp(&components[a].len())
            .then_with(|| components[a][0].cmp(&components[b][0]))
    });

    let mut sorted_components = Vec::with_capacity(components.len());
    let mut cyclic = Vec::with_capacity(components.len());
    let mut component_of = vec![0; n];

    for (component_id, old_id) in order.into_iter().enumerate() {
        let component = std::mem::take(&mut components[old_id]);
        for &vertex in &component {
            component_of[vertex] = component_id;
        }

        cyclic.push(component.len() >= 2 || component_has_self_loop[old_id]);
        sorted_components.push(component);
    }

    Ok(Graph {
        components: sorted_components,
        component_of,
        cyclic,
    })
}