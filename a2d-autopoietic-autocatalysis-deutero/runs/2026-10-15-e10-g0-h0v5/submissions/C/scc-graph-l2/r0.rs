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

    let mut adjacency = vec![Vec::<usize>::new(); n];
    let mut reverse = vec![Vec::<usize>::new(); n];
    for &(from, to) in edges {
        adjacency[from].push(to);
        reverse[to].push(from);
    }

    // Iterative Kosaraju pass to avoid recursion on long paths.
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);
    for start in 0..n {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((vertex, next_index)) = stack.last_mut() {
            if *next_index < adjacency[*vertex].len() {
                let next = adjacency[*vertex][*next_index];
                *next_index += 1;
                if !visited[next] {
                    visited[next] = true;
                    stack.push((next, 0));
                }
            } else {
                let (finished, _) = stack.pop().unwrap();
                order.push(finished);
            }
        }
    }

    let mut raw_component_of = vec![usize::MAX; n];
    let mut raw_components = Vec::<Vec<usize>>::new();
    for &start in order.iter().rev() {
        if raw_component_of[start] != usize::MAX {
            continue;
        }
        let id = raw_components.len();
        let mut members = Vec::new();
        let mut stack = vec![start];
        raw_component_of[start] = id;
        while let Some(vertex) = stack.pop() {
            members.push(vertex);
            for &next in &reverse[vertex] {
                if raw_component_of[next] == usize::MAX {
                    raw_component_of[next] = id;
                    stack.push(next);
                }
            }
        }
        members.sort_unstable();
        raw_components.push(members);
    }

    let mut component_order: Vec<usize> = (0..raw_components.len()).collect();
    component_order.sort_by(|&a, &b| {
        raw_components[b]
            .len()
            .cmp(&raw_components[a].len())
            .then_with(|| raw_components[a][0].cmp(&raw_components[b][0]))
    });

    let mut remap = vec![0usize; raw_components.len()];
    let mut components = Vec::with_capacity(raw_components.len());
    for (new_id, &old_id) in component_order.iter().enumerate() {
        remap[old_id] = new_id;
        components.push(raw_components[old_id].clone());
    }

    let component_of: Vec<usize> = raw_component_of
        .iter()
        .map(|&old_id| remap[old_id])
        .collect();

    let count = components.len();
    let mut cyclic: Vec<bool> = components.iter().map(|members| members.len() > 1).collect();
    let mut internal_edges = vec![0usize; count];
    let mut arc_data = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (edge_index, &(from, to)) in edges.iter().enumerate() {
        let source = component_of[from];
        let target = component_of[to];
        if source == target {
            internal_edges[source] += 1;
            if from == to {
                cyclic[source] = true;
            }
        } else {
            let entry = arc_data.entry((source, target)).or_insert((0, edge_index));
            entry.0 += 1;
            entry.1 = entry.1.min(edge_index);
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

fn main() {}