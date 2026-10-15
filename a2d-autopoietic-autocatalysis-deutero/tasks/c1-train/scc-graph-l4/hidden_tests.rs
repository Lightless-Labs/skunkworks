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

// R38,R39,R50,R52
#[test]
fn hidden_constructor_and_owned_snapshots() {
    let e=[(2,3),(3,2),(4,4),(2,3)];
    let mut state=Incremental::new(6,&e).unwrap();
    assert_eq!(state.snapshot(),analyze(6,&e).unwrap()); assert!(state.history().is_empty());
    let mut copy=state.snapshot(); copy.components.clear();
    assert_eq!(state.snapshot(),analyze(6,&e).unwrap());
    let mut clone=state.clone(); clone.insert(&[(3,5),(5,2)]).unwrap();
    assert_ne!(clone.snapshot(),state.snapshot()); assert!(state.history().is_empty());
    state.insert(&[(0,1)]).unwrap(); assert_ne!(clone.snapshot(),state.snapshot());
    assert!(matches!(Incremental::new(1,&[(1,0)]),Err(GraphError::BadEndpoint{edge:0,side:0,vertex:1})));
    assert!(matches!(Incremental::new(MAX_VERTICES+1,&[]),Err(GraphError::TooManyVertices{..})));
    let over=vec![(9,9);MAX_EDGES+1];
    assert!(matches!(Incremental::new(0,&over),Err(GraphError::TooManyEdges{..})));
}

// R40,R41,R43,R49
#[test]
fn hidden_atomic_batch_error_and_local_index() {
    let mut s=Incremental::new(5,&[(0,1),(1,2)]).unwrap();
    let snap=s.snapshot();
    assert_eq!(s.insert(&[(2,0),(3,4),(4,9)]),Err(GraphError::BadEndpoint{edge:2,side:1,vertex:9}));
    assert_eq!(s.snapshot(),snap); assert!(s.history().is_empty());
    assert_eq!(s.insert(&[(7,8)]),Err(GraphError::BadEndpoint{edge:0,side:0,vertex:7}));
    let b=s.insert(&[(2,0)]).unwrap();
    assert_eq!(b.steps[0].edge_index,2); assert_eq!(b.steps[0].status,EdgeStatus::Merge);
    let saved=s.history(); let snap=s.snapshot();
    assert!(s.insert(&[(3,4),(usize::MAX,0)]).is_err());
    assert_eq!(s.history(),saved);assert_eq!(s.snapshot(),snap);
}

// R40,R41,R43,R46,R51
#[test]
fn hidden_capacity_precedes_endpoints_and_empty_at_limit() {
    let e=vec![(0,0);MAX_EDGES-1]; let mut s=Incremental::new(2,&e).unwrap();
    assert_eq!(s.insert(&[(1,0),(9,9)]),Err(GraphError::TooManyEdges{limit:MAX_EDGES}));
    let b=s.insert(&[(0,0)]).unwrap();
    assert_eq!(b.steps[0].edge_index,MAX_EDGES-1); assert_eq!(b.steps[0].status,EdgeStatus::Duplicate);
    assert_eq!(b.graph.internal_edges,vec![MAX_EDGES,0]);
    let empty=s.insert(&[]).unwrap(); assert!(empty.steps.is_empty()&&empty.merges.is_empty());
    assert_eq!(empty.graph,s.snapshot());
    assert_eq!(s.insert(&[(9,9)]),Err(GraphError::TooManyEdges{limit:MAX_EDGES}));
    assert!(s.history().is_empty());
}

// R42-R46,R50
#[test]
fn hidden_duplicate_precedes_internal_and_reverse_pair() {
    let mut s=Incremental::new(4,&[(1,2),(2,1)]).unwrap();
    let b=s.insert(&[(1,2),(1,1),(1,1),(0,3),(3,0),(3,0)]).unwrap();
    assert_eq!(b.steps.iter().map(|x|x.status.clone()).collect::<Vec<_>>(),vec![EdgeStatus::Duplicate,EdgeStatus::Internal,EdgeStatus::Duplicate,EdgeStatus::Link,EdgeStatus::Merge,EdgeStatus::Duplicate]);
    assert_eq!(b.steps.iter().map(|x|x.edge_index).collect::<Vec<_>>(),vec![2,3,4,5,6,7]);
    assert_eq!(b.graph.components,vec![vec![0,3],vec![1,2]]);
    assert_eq!(b.graph.internal_edges,vec![3,5]);
    assert_eq!(b.merges.len(),1); assert_eq!(b.merges[0].edge_index,6);
    assert_eq!(b.graph,analyze(4,&[(1,2),(2,1),(1,2),(1,1),(1,1),(0,3),(3,0),(3,0)]).unwrap());
}

// R42-R44,R47,R48
#[test]
fn hidden_one_edge_merges_all_path_components() {
    let mut s=Incremental::new(7,&[(1,3),(3,5),(5,6),(0,2)]).unwrap();
    let b=s.insert(&[(6,1)]).unwrap();
    assert_eq!(b.steps,vec![Step{edge_index:4,status:EdgeStatus::Merge,before:(6,1),after:(0,0)}]);
    assert_eq!(b.merges,vec![Merge{edge_index:4,before:vec![1,3,5,6],after:0,members:vec![1,3,5,6]}]);
    assert_eq!(b.graph.components,vec![vec![1,3,5,6],vec![0],vec![2],vec![4]]);
}

// R42,R44,R47,R48
#[test]
fn hidden_merge_excludes_branches_and_renumbers() {
    let mut s=Incremental::new(9,&[(0,1),(1,2),(2,3),(1,4),(5,2),(6,7),(7,6)]).unwrap();
    let b=s.insert(&[(3,0),(5,8)]).unwrap();
    assert_eq!(b.merges,vec![Merge{edge_index:7,before:vec![1,2,3,4],after:0,members:vec![0,1,2,3]}]);
    assert_eq!(b.steps[0].before,(4,1)); assert_eq!(b.steps[0].after,(0,0));
    assert_eq!(b.steps[1],Step{edge_index:8,status:EdgeStatus::Link,before:(3,4),after:(3,4)});
    assert_eq!(b.graph.components,vec![vec![0,1,2,3],vec![6,7],vec![4],vec![5],vec![8]]);
}

// R39,R43,R44,R47-R50
#[test]
fn hidden_history_keeps_event_time_ids_and_members() {
    let mut s=Incremental::new(6,&[]).unwrap();
    let first=s.insert(&[(4,5),(5,4)]).unwrap();
    let saved=first.clone(); let old=s.history();
    let second=s.insert(&[(0,1),(1,0)]).unwrap();
    assert_eq!(second.merges,vec![Merge{edge_index:3,before:vec![1,2],after:0,members:vec![0,1]}]);
    let third=s.insert(&[(1,4),(5,0)]).unwrap();
    assert_eq!(third.merges,vec![Merge{edge_index:5,before:vec![0,1],after:0,members:vec![0,1,4,5]}]);
    assert_eq!(s.history()[0],Merge{edge_index:1,before:vec![4,5],after:0,members:vec![4,5]});
    assert_eq!(s.history().len(),3); assert_eq!(first,saved);assert_eq!(old,first.merges);
    let mut owned=s.history(); owned[0].members.clear(); assert!(!s.history()[0].members.is_empty());
    assert_eq!(first.steps[0].before,(4,5)); assert_eq!(first.steps[0].after,(4,5));
    assert_eq!(first.steps[1].before,(5,4)); assert_eq!(first.steps[1].after,(0,0));
}

// R19,R20,R28,R42,R45,R46
#[test]
fn hidden_self_loop_duplicate_updates_topology() {
    let mut s=Incremental::new(4,&[(3,3)]).unwrap();
    assert_eq!(s.snapshot().topo,vec![3,2,1,0]);
    let b=s.insert(&[(0,0),(0,0)]).unwrap();
    assert_eq!(b.steps[0].status,EdgeStatus::Internal);assert_eq!(b.steps[1].status,EdgeStatus::Duplicate);
    assert!(b.merges.is_empty()&&s.history().is_empty());
    assert_eq!(b.graph.topo,vec![0,3,2,1]);assert_eq!(b.graph.cyclic,vec![true,false,false,true]);
    assert_eq!(b.graph.internal_edges,vec![2,0,0,1]);
    assert_eq!(b.graph.query(0,0).unwrap(),Query{kind:Relation::Cycle,route:vec![0,0]});
}

// R24,R26,R39,R42,R43,R50,R51
#[test]
fn hidden_empty_batches_and_global_witnesses() {
    let mut s=Incremental::new(5,&[(2,2),(2,2)]).unwrap();
    let empty=s.insert(&[]).unwrap(); assert_eq!(empty.graph,s.snapshot());
    let b=s.insert(&[(4,1),(4,1),(4,0)]).unwrap();
    assert_eq!(b.steps.iter().map(|x|x.edge_index).collect::<Vec<_>>(),vec![2,3,4]);
    assert_eq!(b.graph.arcs,vec![Arc{from:4,to:1,multiplicity:2,first_edge:2},Arc{from:4,to:0,multiplicity:1,first_edge:4}]);
    let save=b.clone(); s.insert(&[]).unwrap();
    let next=s.insert(&[(1,4)]).unwrap(); assert_eq!(next.steps[0].edge_index,5);
    assert_eq!(b,save); assert!(empty.steps.is_empty()&&empty.merges.is_empty());
    assert!(s.snapshot().query(4,0).unwrap().kind==Relation::Forward);
    let mut zero=Incremental::new(0,&[]).unwrap();assert!(zero.insert(&[]).unwrap().graph.topo.is_empty());
    assert_eq!(zero.insert(&[(0,0)]),Err(GraphError::BadEndpoint{edge:0,side:0,vertex:0}));
}

// R38-R50,R52
#[test]
fn hidden_deterministic_incremental_replay() {
    let mut s=Incremental::new(8,&[]).unwrap();let mut all=Vec::new();let mut log=Vec::new();
    let batches:Vec<Vec<(usize,usize)>>=vec![vec![(6,2),(2,4)],vec![(4,6),(0,1),(1,0)],vec![(1,2),(4,7)],vec![(7,0),(6,2),(3,3),(3,5)],vec![(5,3),(5,0),(7,5)]];
    for batch in batches {
        let previous=s.snapshot();let b=s.insert(&batch).unwrap();
        for (offset,&e) in batch.iter().enumerate() {
            let before=analyze(8,&all).unwrap();let duplicate=all.contains(&e);all.push(e);let after=analyze(8,&all).unwrap();
            let (u,v)=e;let pair=(before.component_of[u],before.component_of[v]);let post=(after.component_of[u],after.component_of[v]);
            let status=if duplicate{EdgeStatus::Duplicate}else if pair.0==pair.1{EdgeStatus::Internal}else if post.0==post.1{EdgeStatus::Merge}else{EdgeStatus::Link};
            assert_eq!(b.steps[offset],Step{edge_index:all.len()-1,status:status.clone(),before:pair,after:post});
            if status==EdgeStatus::Merge {
                let members=after.components[post.0].clone();let mut ids:Vec<_>=members.iter().map(|&w|before.component_of[w]).collect();ids.sort();ids.dedup();
                log.push(Merge{edge_index:all.len()-1,before:ids,after:post.0,members});
            }
        }
        assert_eq!(b.graph,analyze(8,&all).unwrap());assert_eq!(s.snapshot(),b.graph);assert_eq!(s.history(),log);
        assert_eq!(previous.component_of.len(),8);
    }
    let clone=s.clone();assert_eq!(clone.snapshot(),s.snapshot());assert_eq!(clone.history(),s.history());
}
