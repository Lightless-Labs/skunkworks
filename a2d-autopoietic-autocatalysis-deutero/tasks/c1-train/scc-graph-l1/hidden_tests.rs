// R1-R3,R15
#[test]
fn hidden_zero_contract() {
    let g = analyze(0, &[]).unwrap();
    assert_eq!(g.components, Vec::<Vec<usize>>::new());
    assert_eq!(g.component_of, Vec::<usize>::new());
    assert_eq!(g.cyclic, Vec::<bool>::new());
    assert_eq!(g.clone(), g);
}

// R9-R12,R16
#[test]
fn hidden_isolated_seven() {
    let g = analyze(7, &[]).unwrap();
    assert_eq!(g.components, (0..7).map(|v|vec![v]).collect::<Vec<_>>());
    assert_eq!(g.component_of, (0..7).collect::<Vec<_>>());
    assert_eq!(g.cyclic, vec![false;7]);
}

// R9-R12
#[test]
fn hidden_size_before_minimum() {
    let g = analyze(8, &[(4,6),(6,7),(7,4),(1,3),(3,1),(0,0)]).unwrap();
    assert_eq!(g.components, vec![vec![4,6,7],vec![1,3],vec![0],vec![2],vec![5]]);
    assert_eq!(g.component_of, vec![2,1,3,1,0,4,0,0]);
    assert_eq!(g.cyclic, vec![true,true,true,false,false]);
}

// R10-R12
#[test]
fn hidden_equal_size_tie() {
    let g = analyze(7, &[(5,3),(3,5),(6,0),(0,6),(4,2),(2,4)]).unwrap();
    assert_eq!(g.components, vec![vec![0,6],vec![2,4],vec![3,5],vec![1]]);
    assert_eq!(g.component_of, vec![0,3,1,2,1,2,0]);
}

// R8,R9,R13
#[test]
fn hidden_one_way_diamond() {
    let g = analyze(5, &[(4,2),(4,1),(2,0),(1,0)]).unwrap();
    assert_eq!(g.components, vec![vec![0],vec![1],vec![2],vec![3],vec![4]]);
    assert_eq!(g.cyclic, vec![false;5]);
}

// R13,R14
#[test]
fn hidden_singleton_loops_duplicates() {
    let g = analyze(5, &[(3,3),(3,3),(4,1),(4,1),(0,0)]).unwrap();
    assert_eq!(g.cyclic, vec![true,false,false,true,false]);
    assert_eq!(g.component_of, vec![0,1,2,3,4]);
}

// R4,R5
#[test]
fn hidden_vertex_limit_precedes_everything() {
    let e = vec![(usize::MAX,usize::MAX);MAX_EDGES+1];
    assert_eq!(analyze(MAX_VERTICES+1,&e),Err(GraphError::TooManyVertices{count:MAX_VERTICES+1,limit:MAX_VERTICES}));
    assert_eq!(analyze(usize::MAX,&[]),Err(GraphError::TooManyVertices{count:usize::MAX,limit:MAX_VERTICES}));
}

// R5,R14
#[test]
fn hidden_edge_limit_precedes_endpoints() {
    let e = vec![(9,10);MAX_EDGES+1];
    assert_eq!(analyze(0,&e),Err(GraphError::TooManyEdges{limit:MAX_EDGES}));
}

// R6,R7
#[test]
fn hidden_first_edge_and_source_precedence() {
    assert_eq!(analyze(3,&[(1,1),(1,2),(8,9),(9,1)]),Err(GraphError::BadEndpoint{edge:2,side:0,vertex:8}));
    assert_eq!(analyze(3,&[(2,7),(9,1)]),Err(GraphError::BadEndpoint{edge:0,side:1,vertex:7}));
}

// R6,R7
#[test]
fn hidden_error_offset_retains_duplicates() {
    assert_eq!(analyze(2,&[(1,1),(1,1),(1,0),(0,1),(1,usize::MAX)]),Err(GraphError::BadEndpoint{edge:4,side:1,vertex:usize::MAX}));
    assert_eq!(analyze(2,&[(usize::MAX,0)]),Err(GraphError::BadEndpoint{edge:0,side:0,vertex:usize::MAX}));
}

// R6,R15
#[test]
fn hidden_zero_with_edge() {
    assert_eq!(analyze(0,&[(0,0)]),Err(GraphError::BadEndpoint{edge:0,side:0,vertex:0}));
}

// R4,R5,R14,R18
#[test]
fn hidden_exact_limits() {
    let e = vec![(MAX_VERTICES-1,MAX_VERTICES-1);MAX_EDGES];
    let g = analyze(MAX_VERTICES,&e).unwrap();
    assert_eq!(g.components.len(),MAX_VERTICES);
    assert_eq!(g.component_of[MAX_VERTICES-1],MAX_VERTICES-1);
    assert!(g.cyclic[MAX_VERTICES-1]);
    assert!(!g.cyclic[0]);
}

// R10-R14,R17
#[test]
fn hidden_edge_permutation() {
    let e = vec![(6,2),(2,4),(4,6),(0,3),(3,0),(3,6),(5,5),(5,5)];
    let mut r = e.clone(); r.reverse();
    let (a,b) = (analyze(8,&e).unwrap(),analyze(8,&r).unwrap());
    assert_eq!(a.components,b.components);
    assert_eq!(a.component_of,b.component_of);
    assert_eq!(a.cyclic,b.cyclic);
}

// R8,R9,R13,R18
#[test]
fn hidden_long_path() {
    let e: Vec<_> = (1..MAX_VERTICES).map(|i|(i,i-1)).collect();
    let g = analyze(MAX_VERTICES,&e).unwrap();
    assert_eq!(g.components.len(),MAX_VERTICES);
    assert_eq!(g.component_of,(0..MAX_VERTICES).collect::<Vec<_>>());
    assert!(g.cyclic.iter().all(|&x|!x));
}

// R8-R14,R18
#[test]
fn hidden_long_ring() {
    let mut e: Vec<_> = (1..MAX_VERTICES).map(|i|(i-1,i)).collect();
    e.push((MAX_VERTICES-1,0));
    let g = analyze(MAX_VERTICES,&e).unwrap();
    assert_eq!(g.components,vec![(0..MAX_VERTICES).collect::<Vec<_>>()]);
    assert_eq!(g.component_of,vec![0;MAX_VERTICES]);
    assert_eq!(g.cyclic,vec![true]);
}

// R8-R14,R17,R18
#[test]
fn hidden_all_three_vertex_digraphs() {
    for mask in 0u32..512 {
        let mut e = Vec::new(); let mut reach = [[false;3];3];
        for u in 0..3 { reach[u][u] = true; for v in 0..3 {
            if mask & (1 << (u*3+v)) != 0 { e.push((u,v)); reach[u][v]=true; }
        }}
        for k in 0..3 { for u in 0..3 { for v in 0..3 { reach[u][v] |= reach[u][k] && reach[k][v]; }}}
        let mut expected = Vec::new(); let mut used = [false;3];
        for u in 0..3 { if !used[u] { let mut c=Vec::new();
            for v in 0..3 { if reach[u][v] && reach[v][u] { used[v]=true; c.push(v); }}
            expected.push(c);
        }}
        expected.sort_by_key(|c|(std::cmp::Reverse(c.len()),c[0]));
        let g=analyze(3,&e).unwrap(); assert_eq!(g.components,expected,"mask {}",mask);
        for (id,c) in expected.iter().enumerate() { for &v in c { assert_eq!(g.component_of[v],id); }
            assert_eq!(g.cyclic[id],c.len()>1 || e.contains(&(c[0],c[0])));
        }
    }
}

// R8,R9
#[test]
fn hidden_bridge_does_not_unite() {
    let g=analyze(6,&[(0,2),(2,0),(3,5),(5,3),(2,3)]).unwrap();
    assert_eq!(g.components,vec![vec![0,2],vec![3,5],vec![1],vec![4]]);
}

// R8-R12
#[test]
fn hidden_return_path_unites_intermediate() {
    let g=analyze(6,&[(0,2),(2,4),(4,5),(5,0),(1,3)]).unwrap();
    assert_eq!(g.components,vec![vec![0,2,4,5],vec![1],vec![3]]);
}

// R8,R9,R13
#[test]
fn hidden_chord_and_dead_branch() {
    let g=analyze(7,&[(1,3),(3,5),(5,1),(1,5),(3,6),(6,4)]).unwrap();
    assert_eq!(g.components,vec![vec![1,3,5],vec![0],vec![2],vec![4],vec![6]]);
    assert_eq!(g.cyclic,vec![true,false,false,false,false]);
}

// R9-R14
#[test]
fn hidden_only_self_edges() {
    let g=analyze(6,&[(5,5),(2,2),(5,5),(4,4)]).unwrap();
    assert_eq!(g.components,(0..6).map(|i|vec![i]).collect::<Vec<_>>());
    assert_eq!(g.cyclic,vec![false,false,true,false,true,true]);
}

// R9-R14
#[test]
fn hidden_duplicate_bidirectional() {
    let g=analyze(5,&[(4,2),(4,2),(2,4),(2,4),(0,1),(0,1)]).unwrap();
    assert_eq!(g.components,vec![vec![2,4],vec![0],vec![1],vec![3]]);
    assert_eq!(g.component_of,vec![1,2,0,3,0]);
}

// R8,R13,R14,R16
#[test]
fn hidden_single_vertex_both_states() {
    assert_eq!(analyze(1,&[]).unwrap().cyclic,vec![false]);
    assert_eq!(analyze(1,&[(0,0),(0,0)]).unwrap().cyclic,vec![true]);
}

// R4,R6,R7
#[test]
fn hidden_last_endpoint_at_boundary() {
    let g=analyze(MAX_VERTICES,&[(MAX_VERTICES-1,0)]).unwrap();
    assert_eq!(g.component_of[MAX_VERTICES-1],MAX_VERTICES-1);
    assert_eq!(analyze(MAX_VERTICES,&[(0,MAX_VERTICES)]),Err(GraphError::BadEndpoint{edge:0,side:1,vertex:MAX_VERTICES}));
}

// R8-R12,R18
#[test]
fn hidden_nested_dfs_cross_edges() {
    let g=analyze(9,&[(8,6),(6,4),(4,6),(8,7),(7,5),(5,7),(4,5),(5,3),(3,1),(1,3),(0,2)]).unwrap();
    assert_eq!(g.components,vec![vec![1,3],vec![4,6],vec![5,7],vec![0],vec![2],vec![8]]);
}
