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
fn validate(n: usize, edges: &[(usize, usize)], prior: usize) -> Result<(), GraphError> {
    if n > MAX_VERTICES {
        return Err(GraphError::TooManyVertices { count: n, limit: MAX_VERTICES });
    }
    if edges.len() > MAX_EDGES - prior {
        return Err(GraphError::TooManyEdges { limit: MAX_EDGES });
    }
    for (i, &(u, v)) in edges.iter().enumerate() {
        if v >= n { return Err(GraphError::BadEndpoint { edge: i, side: 1, vertex: v }); }
        if u >= n { return Err(GraphError::BadEndpoint { edge: i, side: 0, vertex: u }); }
    }
    Ok(())
}

pub fn analyze(n: usize, edges: &[(usize, usize)]) -> Result<Graph, GraphError> {
    validate(n, edges, 0)?;
    Ok(build(n, edges))
}

fn build(n: usize, edges: &[(usize, usize)]) -> Graph {
    let mut adj = vec![Vec::new(); n];
    let mut rev = vec![Vec::new(); n];
    for &(u, v) in edges { adj[u].push(v); rev[v].push(u); }
    let mut seen = vec![false; n];
    let mut finish = Vec::new();
    for root in 0..n {
        if seen[root] { continue; }
        seen[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((v, next)) = stack.last_mut() {
            if *next < adj[*v].len() {
                let w = adj[*v][*next];
                *next += 1;
                if !seen[w] { seen[w] = true; stack.push((w, 0)); }
            } else {
                finish.push(*v);
                stack.pop();
            }
        }
    }
    seen.fill(false);
    let mut components = Vec::new();
    for &root in finish.iter().rev() {
        if seen[root] { continue; }
        seen[root] = true;
        let mut stack = vec![root];
        let mut members = Vec::new();
        while let Some(v) = stack.pop() {
            members.push(v);
            for &w in &rev[v] {
                if !seen[w] { seen[w] = true; stack.push(w); }
            }
        }
        members.sort_unstable();
        components.push(members);
    }
    components.sort_by(|a, b| b.len().cmp(&a.len()).then(a[0].cmp(&b[0])));
    let k = components.len();
    let mut component_of = vec![0; n];
    for (c, members) in components.iter().enumerate() {
        for &v in members { component_of[v] = c; }
    }
    let mut cyclic: Vec<bool> = components.iter().map(|c| c.len() > 1).collect();
    for &(u, v) in edges { if u == v { cyclic[component_of[u]] = true; } }

    Graph { components, component_of, cyclic  }
}
