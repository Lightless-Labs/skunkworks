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
        return Err(GraphError::TooManyEdges { limit: MAX_EDGES });
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
    let mut reverse = vec![Vec::new(); n];
    for &(from, to) in edges {
        adjacency[from].push(to);
        reverse[to].push(from);
    }

    // Iterative first pass of Kosaraju's algorithm.
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);
    for start in 0..n {
        if visited[start] {
            continue;
        }

        visited[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((vertex, next_index)) = stack.pop() {
            if next_index < adjacency[vertex].len() {
                stack.push((vertex, next_index + 1));
                let next = adjacency[vertex][next_index];
                if !visited[next] {
                    visited[next] = true;
                    stack.push((next, 0));
                }
            } else {
                order.push(vertex);
            }
        }
    }

    // Second pass assigns temporary SCC identifiers.
    let mut temporary_component_of = vec![usize::MAX; n];
    let mut temporary_components: Vec<Vec<usize>> = Vec::new();
    for &start in order.iter().rev() {
        if temporary_component_of[start] != usize::MAX {
            continue;
        }

        let id = temporary_components.len();
        let mut members = Vec::new();
        let mut stack = vec![start];
        temporary_component_of[start] = id;

        while let Some(vertex) = stack.pop() {
            members.push(vertex);
            for &next in &reverse[vertex] {
                if temporary_component_of[next] == usize::MAX {
                    temporary_component_of[next] = id;
                    stack.push(next);
                }
            }
        }
        members.sort_unstable();
        temporary_components.push(members);
    }

    let mut old_ids: Vec<usize> = (0..temporary_components.len()).collect();
    old_ids.sort_unstable_by(|&a, &b| {
        temporary_components[b]
            .len()
            .cmp(&temporary_components[a].len())
            .then_with(|| temporary_components[a][0].cmp(&temporary_components[b][0]))
    });

    let mut old_to_new = vec![0usize; temporary_components.len()];
    let mut components = Vec::with_capacity(temporary_components.len());
    for (new_id, old_id) in old_ids.into_iter().enumerate() {
        old_to_new[old_id] = new_id;
        components.push(std::mem::take(&mut temporary_components[old_id]));
    }

    let component_of: Vec<usize> = temporary_component_of
        .iter()
        .map(|&old_id| old_to_new[old_id])
        .collect();

    let component_count = components.len();
    let mut cyclic: Vec<bool> = components.iter().map(|c| c.len() >= 2).collect();
    let mut internal_edges = vec![0usize; component_count];

    // Aggregate condensation arcs by ordered component pair.
    let mut arc_index = std::collections::HashMap::<(usize, usize), usize>::new();
    let mut arcs: Vec<Arc> = Vec::new();

    for (edge_index, &(from, to)) in edges.iter().enumerate() {
        let from_component = component_of[from];
        let to_component = component_of[to];

        if from_component == to_component {
            internal_edges[from_component] += 1;
            if from == to {
                cyclic[from_component] = true;
            }
        } else if let Some(&index) = arc_index.get(&(from_component, to_component)) {
            arcs[index].multiplicity += 1;
        } else {
            let index = arcs.len();
            arcs.push(Arc {
                from: from_component,
                to: to_component,
                multiplicity: 1,
                first_edge: edge_index,
            });
            arc_index.insert((from_component, to_component), index);
        }
    }

    arcs.sort_unstable_by(|a, b| {
        a.from
            .cmp(&b.from)
            .then_with(|| b.to.cmp(&a.to))
    });

    Ok(Graph {
        components,
        component_of,
        cyclic,
        internal_edges,
        arcs,
    })
}