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

    // Build a compact directed graph, then use iterative Tarjan traversal.
    let mut adjacency = vec![Vec::<usize>::new(); n];
    for &(from, to) in edges {
        adjacency[from].push(to);
    }

    let mut discovery = vec![usize::MAX; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut tarjan_stack = Vec::with_capacity(n);
    let mut raw_components = Vec::<Vec<usize>>::new();
    let mut next_discovery = 0usize;

    for root in 0..n {
        if discovery[root] != usize::MAX {
            continue;
        }

        discovery[root] = next_discovery;
        low[root] = next_discovery;
        next_discovery += 1;
        tarjan_stack.push(root);
        on_stack[root] = true;

        let mut dfs = vec![(root, 0usize)];
        while let Some((vertex, next_neighbor)) = dfs.last_mut() {
            if *next_neighbor < adjacency[*vertex].len() {
                let neighbor = adjacency[*vertex][*next_neighbor];
                *next_neighbor += 1;

                if discovery[neighbor] == usize::MAX {
                    discovery[neighbor] = next_discovery;
                    low[neighbor] = next_discovery;
                    next_discovery += 1;
                    tarjan_stack.push(neighbor);
                    on_stack[neighbor] = true;
                    dfs.push((neighbor, 0));
                } else if on_stack[neighbor] {
                    low[*vertex] = low[*vertex].min(discovery[neighbor]);
                }
                continue;
            }

            let (finished, _) = dfs.pop().unwrap();
            if let Some(&(parent, _)) = dfs.last() {
                low[parent] = low[parent].min(low[finished]);
            }

            if low[finished] == discovery[finished] {
                let mut component = Vec::new();
                loop {
                    let member = tarjan_stack.pop().unwrap();
                    on_stack[member] = false;
                    component.push(member);
                    if member == finished {
                        break;
                    }
                }
                component.sort_unstable();
                raw_components.push(component);
            }
        }
    }

    raw_components.sort_by(|a, b| {
        b.len()
            .cmp(&a.len())
            .then_with(|| a[0].cmp(&b[0]))
    });

    let mut component_of = vec![0usize; n];
    for (id, component) in raw_components.iter().enumerate() {
        for &vertex in component {
            component_of[vertex] = id;
        }
    }

    let mut cyclic: Vec<bool> = raw_components.iter().map(|c| c.len() >= 2).collect();
    let mut internal_edges = vec![0usize; raw_components.len()];
    let mut arc_counts = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();

    for (edge_index, &(from, to)) in edges.iter().enumerate() {
        let from_id = component_of[from];
        let to_id = component_of[to];
        if from_id == to_id {
            internal_edges[from_id] += 1;
            if from == to {
                cyclic[from_id] = true;
            }
        } else {
            let entry = arc_counts.entry((from_id, to_id)).or_insert((0, edge_index));
            entry.0 += 1;
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

fn main() {}