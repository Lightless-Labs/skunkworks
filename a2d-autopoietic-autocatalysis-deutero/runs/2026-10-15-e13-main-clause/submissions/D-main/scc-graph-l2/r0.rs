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
    for (index, &(from, to)) in edges.iter().enumerate() {
        if from >= n {
            return Err(GraphError::BadEndpoint {
                edge: index,
                side: 0,
                vertex: from,
            });
        }
        if to >= n {
            return Err(GraphError::BadEndpoint {
                edge: index,
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
    let mut finish_order = Vec::with_capacity(n);
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
                finish_order.push(finished);
            }
        }
    }

    let mut raw_component_of = vec![usize::MAX; n];
    let mut raw_components = Vec::<Vec<usize>>::new();
    for &start in finish_order.iter().rev() {
        if raw_component_of[start] != usize::MAX {
            continue;
        }
        let raw_id = raw_components.len();
        let mut members = Vec::new();
        let mut stack = vec![start];
        raw_component_of[start] = raw_id;
        while let Some(vertex) = stack.pop() {
            members.push(vertex);
            for &next in &reverse[vertex] {
                if raw_component_of[next] == usize::MAX {
                    raw_component_of[next] = raw_id;
                    stack.push(next);
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

    let mut raw_to_canonical = vec![0usize; raw_components.len()];
    let mut components = Vec::with_capacity(raw_components.len());
    for (canonical_id, &raw_id) in order.iter().enumerate() {
        raw_to_canonical[raw_id] = canonical_id;
        components.push(raw_components[raw_id].clone());
    }

    let component_of: Vec<usize> = raw_component_of
        .into_iter()
        .map(|raw_id| raw_to_canonical[raw_id])
        .collect();

    let mut cyclic: Vec<bool> = components
        .iter()
        .map(|component| component.len() >= 2)
        .collect();
    let mut internal_edges = vec![0usize; components.len()];
    let mut arc_counts = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (index, &(from, to)) in edges.iter().enumerate() {
        let source_component = component_of[from];
        let target_component = component_of[to];
        if source_component == target_component {
            internal_edges[source_component] += 1;
            if from == to {
                cyclic[source_component] = true;
            }
        } else {
            let entry = arc_counts
                .entry((source_component, target_component))
                .or_insert((0, index));
            entry.0 += 1;
            entry.1 = entry.1.min(index);
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