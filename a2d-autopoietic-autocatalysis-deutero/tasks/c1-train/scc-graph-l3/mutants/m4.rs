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
    pub from: usize, pub to: usize,
    pub multiplicity: usize, pub first_edge: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Relation { Cycle, Trivial, Forward, Backward, Separate }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query { pub kind: Relation, pub route: Vec<usize> }
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
fn validate(n: usize, edges: &[(usize, usize)], prior: usize) -> Result<(), GraphError> {
    if n > MAX_VERTICES {
        return Err(GraphError::TooManyVertices { count: n, limit: MAX_VERTICES });
    }
    if edges.len() > MAX_EDGES - prior {
        return Err(GraphError::TooManyEdges { limit: MAX_EDGES });
    }
    for (i, &(u, v)) in edges.iter().enumerate() {
        if u >= n { return Err(GraphError::BadEndpoint { edge: i, side: 0, vertex: u }); }
        if v >= n { return Err(GraphError::BadEndpoint { edge: i, side: 1, vertex: v }); }
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
    let mut internal_edges = vec![0; k];
    let mut table = std::collections::BTreeMap::<(usize, usize), (usize, usize)>::new();
    for (i, &(u, v)) in edges.iter().enumerate() {
        let (a, b) = (component_of[u], component_of[v]);
        if a == b { internal_edges[a] += 1; }
        else {
            let value = table.entry((a, b)).or_insert((0, i));
            value.0 += 1;
        }
    }
    let mut arcs: Vec<Arc> = table.into_iter().map(|((from, to), (multiplicity, first_edge))|
        Arc { from, to, multiplicity, first_edge }).collect();
    arcs.sort_by(|a, b| a.from.cmp(&b.from).then(b.to.cmp(&a.to)));
    let mut indegree = vec![0usize; k];
    let mut outgoing = vec![Vec::new(); k];
    for arc in &arcs { indegree[arc.to] += 1; outgoing[arc.from].push(arc.to); }
    let mut ready = std::collections::BTreeSet::new();
    for c in 0..k { if indegree[c] == 0 { ready.insert((internal_edges[c], components[c][0], c)); } }
    let mut topo = Vec::new();
    let mut depth = vec![0; k];
    while let Some(key) = ready.pop_last() {
        let c = key.2;
        topo.push(c);
        for &d in &outgoing[c] {
            depth[d] = depth[d].max(depth[c] + 1);
            indegree[d] -= 1;
            if indegree[d] == 0 { ready.insert((internal_edges[d], components[d][0], d)); }
        }
    }

    Graph { components, component_of, cyclic , internal_edges, arcs, topo, depth }
}
impl Graph {
    pub fn query(&self, from: usize, to: usize) -> Result<Query, GraphError> {
        let n = self.component_of.len();
        if from >= n { return Err(GraphError::BadQuery { side: 0, vertex: from }); }
        if to >= n { return Err(GraphError::BadQuery { side: 1, vertex: to }); }
        let (a, b) = (self.component_of[from], self.component_of[to]);
        if a == b {
            return Ok(if self.cyclic[a] && from != to {
                Query { kind: Relation::Cycle, route: vec![a, a] }
            } else { Query { kind: Relation::Trivial, route: vec![a] } });
        }
        if let Some(route) = self.shortest(a, b) { return Ok(Query { kind: Relation::Forward, route }); }
        if let Some(mut route) = self.shortest(b, a) {
            route.reverse();
            return Ok(Query { kind: Relation::Backward, route });
        }
        Ok(Query { kind: Relation::Separate, route: Vec::new() })
    }

    fn shortest(&self, start: usize, end: usize) -> Option<Vec<usize>> {
        let k = self.components.len();
        let mut adj = vec![Vec::new(); k];
        for arc in &self.arcs { adj[arc.from].push(arc.to); }
        for row in &mut adj { row.sort_unstable(); }
        let mut parent = vec![usize::MAX; k];
        parent[start] = start;
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(v) = queue.pop_front() {
            if v == end {
                let mut route = vec![end];
                let mut cursor = end;
                while cursor != start { cursor = parent[cursor]; route.push(cursor); }
                route.reverse();
                return Some(route);
            }
            for &w in &adj[v] {
                if parent[w] == usize::MAX { parent[w] = v; queue.push_back(w); }
            }
        }
        None
    }
}
