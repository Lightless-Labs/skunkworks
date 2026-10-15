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

// R19,R20,R26
#[test]
fn hidden_internal_occurrence_accounting() {
    let e=[(4,4),(4,4),(1,3),(3,1),(1,3),(0,2)];
    let g=analyze(5,&e).unwrap();
    assert_eq!(g.components,vec![vec![1,3],vec![0],vec![2],vec![4]]);
    assert_eq!(g.internal_edges,vec![3,0,0,2]);
    assert_eq!(g.arcs,vec![Arc{from:1,to:2,multiplicity:1,first_edge:5}]);
}

// R21-R24,R26
#[test]
fn hidden_aggregates_different_vertex_pairs() {
    let g=analyze(6,&[(0,2),(2,0),(1,3),(3,1),(0,1),(2,3),(0,1),(2,1)]).unwrap();
    assert_eq!(g.arcs,vec![Arc{from:0,to:1,multiplicity:4,first_edge:4}]);
    assert_eq!(g.internal_edges,vec![2,2,0,0]);
}

// R21-R25
#[test]
fn hidden_arc_order_descending_target() {
    let g=analyze(5,&[(0,1),(3,4),(0,4),(0,2),(3,1),(0,1)]).unwrap();
    assert_eq!(g.arcs,vec![
        Arc{from:0,to:4,multiplicity:1,first_edge:2},
        Arc{from:0,to:2,multiplicity:1,first_edge:3},
        Arc{from:0,to:1,multiplicity:2,first_edge:0},
        Arc{from:3,to:4,multiplicity:1,first_edge:1},
        Arc{from:3,to:1,multiplicity:1,first_edge:4}]);
}

// R19-R26
#[test]
fn hidden_witness_uses_original_positions() {
    let g=analyze(5,&[(3,3),(3,3),(1,2),(2,1),(1,2),(3,1),(3,2),(0,4)]).unwrap();
    assert_eq!(g.arcs,vec![Arc{from:1,to:3,multiplicity:1,first_edge:7},Arc{from:2,to:0,multiplicity:2,first_edge:5}]);
    assert_eq!(g.internal_edges,vec![3,0,2,0]);
}

// R15,R16,R19,R21,R27,R30
#[test]
fn hidden_condensation_empty_and_isolated() {
    let g=analyze(0,&[]).unwrap();
    assert!(g.internal_edges.is_empty() && g.arcs.is_empty());
    let g=analyze(9,&[]).unwrap();
    assert_eq!(g.internal_edges,vec![0;9]); assert!(g.arcs.is_empty());
}

// R19-R23,R26
#[test]
fn hidden_condensation_all_internal() {
    let g=analyze(4,&[(0,1),(1,2),(2,3),(3,0),(1,1),(0,1)]).unwrap();
    assert_eq!(g.internal_edges,vec![6]); assert!(g.arcs.is_empty());
}

// R17,R22-R26
#[test]
fn hidden_condensation_permutation_witnesses() {
    let e=[(2,2),(3,1),(3,0),(3,1),(0,0)];
    let mut r=e; r.reverse();
    let a=analyze(4,&e).unwrap(); let b=analyze(4,&r).unwrap();
    assert_eq!(a.internal_edges,b.internal_edges);
    assert_eq!(a.arcs.iter().map(|x|(x.from,x.to,x.multiplicity)).collect::<Vec<_>>(),b.arcs.iter().map(|x|(x.from,x.to,x.multiplicity)).collect::<Vec<_>>());
    assert_eq!(a.arcs[0].first_edge,1); assert_eq!(b.arcs[0].first_edge,1);
    assert_eq!(a.arcs[1].first_edge,2); assert_eq!(b.arcs[1].first_edge,2);
}

// R19-R26
#[test]
fn hidden_all_small_condensations() {
    for mask in 0u32..512 {
        let e:Vec<_>=(0..9).filter(|&b|mask&(1<<b)!=0).map(|b|(b/3,b%3)).collect();
        let g=analyze(3,&e).unwrap();
        let k=g.components.len(); let mut counts=vec![vec![0;k];k]; let mut first=vec![vec![usize::MAX;k];k];
        for (i,&(u,v)) in e.iter().enumerate() { let (a,b)=(g.component_of[u],g.component_of[v]); counts[a][b]+=1; first[a][b]=first[a][b].min(i); }
        let mut expected=Vec::new();
        for a in 0..k { assert_eq!(g.internal_edges[a],counts[a][a]);
            for b in (0..k).rev() { if a!=b && counts[a][b]>0 { expected.push(Arc{from:a,to:b,multiplicity:counts[a][b],first_edge:first[a][b]}); }}
        }
        assert_eq!(g.arcs,expected);
    }
}

// R27-R30
#[test]
fn hidden_priority_and_immediate_readiness() {
    let g=analyze(6,&[(4,4),(4,4),(3,3),(1,1),(4,3),(3,1),(0,2)]).unwrap();
    assert_eq!(g.topo,vec![4,3,1,5,0,2]);
    assert_eq!(g.depth,vec![0,2,1,1,0,0]);
    let h=analyze(4,&[(3,1)]).unwrap();
    assert_eq!(h.topo,vec![3,2,1,0]);
    assert_eq!(h.depth,vec![0,1,0,0]);
}

// R11,R27-R29
#[test]
fn hidden_topology_uses_vertex_not_component_id() {
    let g=analyze(7,&[(0,2),(2,0),(4,5),(5,6),(6,4)]).unwrap();
    assert_eq!(g.components,vec![vec![4,5,6],vec![0,2],vec![1],vec![3]]);
    assert_eq!(g.topo,vec![0,1,3,2]);
    assert_eq!(g.depth,vec![0,0,0,0]);
}

// R31,R37
#[test]
fn hidden_query_validation_and_empty() {
    let g=analyze(4,&[]).unwrap();
    assert_eq!(g.query(usize::MAX,usize::MAX),Err(GraphError::BadQuery{side:0,vertex:usize::MAX}));
    assert_eq!(g.query(3,4),Err(GraphError::BadQuery{side:1,vertex:4}));
    assert_eq!(g.query(4,4),Err(GraphError::BadQuery{side:0,vertex:4}));
    assert_eq!(analyze(0,&[]).unwrap().query(0,0),Err(GraphError::BadQuery{side:0,vertex:0}));
}

// R31,R32,R35
#[test]
fn hidden_cycle_trivial_and_separate_encodings() {
    let g=analyze(7,&[(4,6),(6,4),(2,2)]).unwrap(); let saved=g.clone();
    assert_eq!(g.query(6,4).unwrap(),Query{kind:Relation::Cycle,route:vec![0,0]});
    assert_eq!(g.query(4,4).unwrap(),Query{kind:Relation::Cycle,route:vec![0,0]});
    assert_eq!(g.query(2,2).unwrap(),Query{kind:Relation::Cycle,route:vec![3,3]});
    assert_eq!(g.query(5,5).unwrap(),Query{kind:Relation::Trivial,route:vec![5]});
    assert_eq!(g.query(2,6).unwrap(),Query{kind:Relation::Separate,route:vec![]});
    assert_eq!(g,saved);
}

// R33-R36
#[test]
fn hidden_shortest_routes_and_backward_reversal() {
    let g=analyze(8,&[(0,2),(0,1),(1,4),(2,4),(4,7),(0,3),(3,5),(5,6),(6,7)]).unwrap();
    assert_eq!(g.query(0,7).unwrap(),Query{kind:Relation::Forward,route:vec![0,1,4,7]});
    assert_eq!(g.query(7,0).unwrap(),Query{kind:Relation::Backward,route:vec![7,4,1,0]});
    assert_eq!(g.query(0,4).unwrap().route,vec![0,1,4]);
}

// R27,R30,R33,R36
#[test]
fn hidden_depth_is_longest_not_shortest() {
    let g=analyze(6,&[(0,5),(0,1),(1,2),(2,3),(3,5),(4,5),(0,5)]).unwrap();
    assert_eq!(g.depth,vec![0,1,2,3,0,4]);
    assert_eq!(g.topo,vec![4,0,1,2,3,5]);
    assert_eq!(g.query(0,5).unwrap().route,vec![0,5]);
}

// R33,R34,R36
#[test]
fn hidden_lex_tie_before_backward_reverse() {
    let g=analyze(8,&[(0,1),(1,6),(6,7),(0,2),(2,3),(3,7)]).unwrap();
    assert_eq!(g.query(0,7).unwrap().route,vec![0,1,6,7]);
    assert_eq!(g.query(7,0).unwrap().route,vec![7,6,1,0]);
}

// R11,R12,R32-R36
#[test]
fn hidden_query_original_vertices_and_component_ids() {
    let g=analyze(8,&[(5,6),(6,7),(7,5),(2,4),(4,2),(0,2),(4,5),(5,1)]).unwrap();
    assert_eq!(g.query(0,7).unwrap(),Query{kind:Relation::Forward,route:vec![2,1,0]});
    assert_eq!(g.query(1,2).unwrap(),Query{kind:Relation::Backward,route:vec![3,0,1]});
    assert_eq!(g.query(6,5).unwrap().route,vec![0,0]);
}

// R20,R27-R29,R36
#[test]
fn hidden_internal_duplicate_changes_topo_priority() {
    let a=analyze(4,&[(0,0),(3,3)]).unwrap();
    let b=analyze(4,&[(0,0),(3,3),(0,0)]).unwrap();
    assert_eq!(a.topo,vec![3,0,2,1]); assert_eq!(b.topo,vec![0,3,2,1]);
    assert_eq!(b.query(0,3).unwrap().kind,Relation::Separate);
    assert!(analyze(0,&[]).unwrap().topo.is_empty());
    assert!(analyze(0,&[]).unwrap().depth.is_empty());
}

// R27-R37
#[test]
fn hidden_all_small_query_pairs() {
    for mask in 0u32..512 {
        let e:Vec<_>=(0..9).filter(|&b|mask&(1<<b)!=0).map(|b|(b/3,b%3)).collect();
        let g=analyze(3,&e).unwrap(); let k=g.components.len();
        let mut reach=vec![vec![false;k];k]; for c in 0..k {reach[c][c]=true;}
        for a in &g.arcs {reach[a.from][a.to]=true;}
        for x in 0..k {for a in 0..k {for b in 0..k {reach[a][b]|=reach[a][x]&&reach[x][b];}}}
        let mut done=vec![false;k]; let mut dep=vec![0;k];
        for &c in &g.topo {
            let mut ready:Vec<_>=(0..k).filter(|&v|!done[v]&&g.arcs.iter().filter(|a|a.to==v).all(|a|done[a.from])).collect();
            ready.sort_by_key(|&v|(g.internal_edges[v],g.components[v][0]));
            assert_eq!(Some(c),ready.pop());
            for a in g.arcs.iter().filter(|a|a.to==c) {dep[c]=dep[c].max(dep[a.from]+1);}
            done[c]=true;
        }
        assert!(done.iter().all(|&d|d)); assert_eq!(g.depth,dep);
        for u in 0..3 {for v in 0..3 {
            let (a,b)=(g.component_of[u],g.component_of[v]);let q=g.query(u,v).unwrap();
            let expected=if a==b {if g.cyclic[a]{Relation::Cycle}else{Relation::Trivial}}else if reach[a][b]{Relation::Forward}else if reach[b][a]{Relation::Backward}else{Relation::Separate};
            assert_eq!(q.kind,expected);
            if a!=b && (reach[a][b]||reach[b][a]) {assert_eq!(q.route.first(),Some(&a));assert_eq!(q.route.last(),Some(&b));}
        }}
    }
}
