pub const MAX_VERTICES: usize = 4096;
pub const MAX_EDGES: usize = 32768;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    TooManyVertices { count: usize, limit: usize },
    TooManyEdges { limit: usize },
    BadEndpoint { edge: usize, side: u8, vertex: usize },
    BadQuery { side: u8, vertex: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arc {
    pub from: usize,
    pub to: usize,
    pub multiplicity: usize,
    pub first_edge: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Relation {
    Cycle,
    Trivial,
    Forward,
    Backward,
    Separate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub kind: Relation,
    pub route: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Graph {
    pub components: Vec<Vec<usize>>,
    pub component_of: Vec<usize>,
    pub cyclic: Vec<bool>,
    pub internal_edges: Vec<usize>,
    pub arcs: Vec<Arc>,
    pub topo: Vec<usize>,
    pub depth: Vec<usize>,
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
    for (i, &(from, to)) in edges.iter().enumerate() {
        if from >= n {
            return Err(GraphError::BadEndpoint {
                edge: i,
                side: 0,
                vertex: from,
            });
        }
        if to >= n {
            return Err(GraphError::BadEndpoint {
                edge: i,
                side: 1,
                vertex: to,
            });
        }
    }

    let mut outgoing = vec![Vec::<usize>::new(); n];
    let mut incoming = vec![Vec::<usize>::new(); n];
    for &(from, to) in edges {
        outgoing[from].push(to);
        incoming[to].push(from);
    }

    // Iterative Kosaraju traversal avoids recursion on long paths.
    let mut visited = vec![false; n];
    let mut finish_order = Vec::with_capacity(n);
    for start in 0..n {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((vertex, next_index)) = stack.last_mut() {
            if *next_index < outgoing[*vertex].len() {
                let next = outgoing[*vertex][*next_index];
                *next_index += 1;
                if !visited[next] {
                    visited[next] = true;
                    stack.push((next, 0));
                }
            } else {
                let (done, _) = stack.pop().unwrap();
                finish_order.push(done);
            }
        }
    }

    let mut assigned = vec![false; n];
    let mut components = Vec::<Vec<usize>>::new();
    for &start in finish_order.iter().rev() {
        if assigned[start] {
            continue;
        }
        assigned[start] = true;
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(vertex) = stack.pop() {
            component.push(vertex);
            for &prev in &incoming[vertex] {
                if !assigned[prev] {
                    assigned[prev] = true;
                    stack.push(prev);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }

    components.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    let component_count = components.len();
    let mut component_of = vec![0usize; n];
    for (id, component) in components.iter().enumerate() {
        for &vertex in component {
            component_of[vertex] = id;
        }
    }

    let mut cyclic = components
        .iter()
        .map(|component| component.len() >= 2)
        .collect::<Vec<_>>();
    let mut internal_edges = vec![0usize; component_count];
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

    let mut arcs = arc_data
        .into_iter()
        .map(|((from, to), (multiplicity, first_edge))| Arc {
            from,
            to,
            multiplicity,
            first_edge,
        })
        .collect::<Vec<_>>();
    arcs.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| b.to.cmp(&a.to)));

    let mut dag_out = vec![Vec::<usize>::new(); component_count];
    let mut dag_in = vec![Vec::<usize>::new(); component_count];
    let mut indegree = vec![0usize; component_count];
    for arc in &arcs {
        dag_out[arc.from].push(arc.to);
        dag_in[arc.to].push(arc.from);
        indegree[arc.to] += 1;
    }

    let mut available = std::collections::BinaryHeap::<(usize, usize, usize)>::new();
    for id in 0..component_count {
        if indegree[id] == 0 {
            available.push((internal_edges[id], components[id][0], id));
        }
    }

    let mut topo = Vec::with_capacity(component_count);
    let mut depth = vec![0usize; component_count];
    while let Some((_, _, id)) = available.pop() {
        topo.push(id);
        for &next in &dag_out[id] {
            depth[next] = depth[next].max(depth[id] + 1);
            indegree[next] -= 1;
            if indegree[next] == 0 {
                available.push((internal_edges[next], components[next][0], next));
            }
        }
    }

    Ok(Graph {
        components,
        component_of,
        cyclic,
        internal_edges,
        arcs,
        topo,
        depth,
    })
}

impl Graph {
    pub fn query(&self, from: usize, to: usize) -> Result<Query, GraphError> {
        if from >= self.component_of.len() {
            return Err(GraphError::BadQuery {
                side: 0,
                vertex: from,
            });
        }
        if to >= self.component_of.len() {
            return Err(GraphError::BadQuery {
                side: 1,
                vertex: to,
            });
        }

        let a = self.component_of[from];
        let b = self.component_of[to];
        if a == b {
            if self.cyclic[a] {
                return Ok(Query {
                    kind: Relation::Cycle,
                    route: vec![a, a],
                });
            }
            return Ok(Query {
                kind: Relation::Trivial,
                route: vec![a],
            });
        }

        if let Some(route) = self.shortest_route(a, b) {
            return Ok(Query {
                kind: Relation::Forward,
                route,
            });
        }
        if let Some(mut route) = self.shortest_route(b, a) {
            route.reverse();
            return Ok(Query {
                kind: Relation::Backward,
                route,
            });
        }
        Ok(Query {
            kind: Relation::Separate,
            route: Vec::new(),
        })
    }

    fn shortest_route(&self, start: usize, goal: usize) -> Option<Vec<usize>> {
        let count = self.components.len();
        let mut adjacency = vec![Vec::<usize>::new(); count];
        for arc in &self.arcs {
            adjacency[arc.from].push(arc.to);
        }
        for neighbors in &mut adjacency {
            neighbors.sort_unstable();
            neighbors.dedup();
        }

        let mut distance = vec![usize::MAX; count];
        let mut queue = std::collections::VecDeque::new();
        distance[goal] = 0;
        queue.push_back(goal);

        // Reverse BFS computes shortest distances to the goal.
        let mut reverse = vec![Vec::<usize>::new(); count];
        for (source, neighbors) in adjacency.iter().enumerate() {
            for &target in neighbors {
                reverse[target].push(source);
            }
        }
        while let Some(vertex) = queue.pop_front() {
            for &prev in &reverse[vertex] {
                if distance[prev] == usize::MAX {
                    distance[prev] = distance[vertex] + 1;
                    queue.push_back(prev);
                }
            }
        }
        if distance[start] == usize::MAX {
            return None;
        }

        let mut route = vec![start];
        let mut current = start;
        while current != goal {
            let next = adjacency[current]
                .iter()
                .copied()
                .filter(|&candidate| distance[candidate] + 1 == distance[current])
                .min()?;
            route.push(next);
            current = next;
        }
        Some(route)
    }
}

fn main() {}