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
    let mut reverse = vec![Vec::new(); n];
    for &(from, to) in edges {
        adjacency[from].push(to);
        reverse[to].push(from);
    }

    let mut visited = vec![false; n];
    let mut finish_order = Vec::with_capacity(n);

    for start in 0..n {
        if visited[start] {
            continue;
        }

        visited[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((vertex, next_index)) = stack.last_mut() {
            if *next_index < adjacency[*vertex].len() {
                let neighbor = adjacency[*vertex][*next_index];
                *next_index += 1;
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

    let mut raw_components = Vec::new();
    let mut raw_component_of = vec![usize::MAX; n];

    for &start in finish_order.iter().rev() {
        if raw_component_of[start] != usize::MAX {
            continue;
        }

        let component_id = raw_components.len();
        let mut members = Vec::new();
        let mut stack = vec![start];
        raw_component_of[start] = component_id;

        while let Some(vertex) = stack.pop() {
            members.push(vertex);
            for &neighbor in &reverse[vertex] {
                if raw_component_of[neighbor] == usize::MAX {
                    raw_component_of[neighbor] = component_id;
                    stack.push(neighbor);
                }
            }
        }

        members.sort_unstable();
        raw_components.push(members);
    }

    let mut order: Vec<usize> = (0..raw_components.len()).collect();
    order.sort_unstable_by(|&a, &b| {
        raw_components[b]
            .len()
            .cmp(&raw_components[a].len())
            .then_with(|| raw_components[a][0].cmp(&raw_components[b][0]))
    });

    let mut remap = vec![0usize; raw_components.len()];
    let mut components = Vec::with_capacity(raw_components.len());
    for (new_id, &old_id) in order.iter().enumerate() {
        remap[old_id] = new_id;
        components.push(raw_components[old_id].clone());
    }

    let component_of: Vec<usize> = raw_component_of
        .into_iter()
        .map(|old_id| remap[old_id])
        .collect();

    let mut cyclic: Vec<bool> = components
        .iter()
        .map(|members| members.len() >= 2)
        .collect();
    let mut internal_edges = vec![0usize; components.len()];
    let mut arc_positions: Vec<Vec<Option<usize>>> =
        vec![vec![None; components.len()]; components.len()];
    let mut arcs: Vec<Arc> = Vec::new();

    for (edge_index, &(from_vertex, to_vertex)) in edges.iter().enumerate() {
        let from = component_of[from_vertex];
        let to = component_of[to_vertex];

        if from == to {
            internal_edges[from] += 1;
            if from_vertex == to_vertex {
                cyclic[from] = true;
            }
        } else if let Some(position) = arc_positions[from][to] {
            arcs[position].multiplicity += 1;
        } else {
            let position = arcs.len();
            arc_positions[from][to] = Some(position);
            arcs.push(Arc {
                from,
                to,
                multiplicity: 1,
                first_edge: edge_index,
            });
        }
    }

    arcs.sort_unstable_by(|a, b| a.from.cmp(&b.from).then_with(|| b.to.cmp(&a.to)));

    Ok(Graph {
        components,
        component_of,
        cyclic,
        internal_edges,
        arcs,
    })
}

fn main() {}