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
    for (i, &(s, t)) in edges.iter().enumerate() {
        if s >= n {
            return Err(GraphError::BadEndpoint {
                edge: i,
                side: 0,
                vertex: s,
            });
        }
        if t >= n {
            return Err(GraphError::BadEndpoint {
                edge: i,
                side: 1,
                vertex: t,
            });
        }
    }

    let mut adj = vec![Vec::new(); n];
    let mut rev = vec![Vec::new(); n];
    for &(s, t) in edges {
        adj[s].push(t);
        rev[t].push(s);
    }

    let mut seen = vec![false; n];
    let mut finish = Vec::with_capacity(n);
    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0usize)];
        while let Some((v, next_edge)) = stack.last_mut() {
            if *next_edge < adj[*v].len() {
                let w = adj[*v][*next_edge];
                *next_edge += 1;
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                finish.push(*v);
                stack.pop();
            }
        }
    }

    seen.fill(false);
    let mut components = Vec::new();
    for &root in finish.iter().rev() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut component = Vec::new();
        let mut stack = vec![root];
        while let Some(v) = stack.pop() {
            component.push(v);
            for &w in &rev[v] {
                if !seen[w] {
                    seen[w] = true;
                    stack.push(w);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }

    components.sort_by(|a, b| {
        b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0]))
    });

    let mut component_of = vec![0usize; n];
    let mut cyclic = Vec::with_capacity(components.len());
    for (id, component) in components.iter().enumerate() {
        for &v in component {
            component_of[v] = id;
        }
        cyclic.push(component.len() > 1);
    }
    for &(s, t) in edges {
        if s == t {
            cyclic[component_of[s]] = true;
        }
    }

    Ok(Graph {
        components,
        component_of,
        cyclic,
    })
}
