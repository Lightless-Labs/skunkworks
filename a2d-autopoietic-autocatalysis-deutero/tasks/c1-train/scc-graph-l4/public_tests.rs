// R38,R39,R42-R45
#[test]
fn public_incremental_link() {
    let mut s=Incremental::new(10,&[]).unwrap();assert!(s.history().is_empty());
    let b=s.insert(&[(0,1)]).unwrap();
    assert_eq!(b.steps,vec![Step{edge_index:0,status:EdgeStatus::Link,before:(0,1),after:(0,1)}]);
    assert!(b.merges.is_empty());assert_eq!(b.graph,s.snapshot());
    assert_eq!(b.graph,analyze(10,&[(0,1)]).unwrap());
}

// R44,R47-R50
#[test]
fn public_incremental_two_component_merge() {
    let mut s=Incremental::new(10,&[(0,1)]).unwrap();let b=s.insert(&[(1,0)]).unwrap();
    assert_eq!(b.steps[0],Step{edge_index:1,status:EdgeStatus::Merge,before:(1,0),after:(0,0)});
    assert_eq!(b.merges,vec![Merge{edge_index:1,before:vec![0,1],after:0,members:vec![0,1]}]);
    assert_eq!(s.history(),b.merges);assert_eq!(s.snapshot().query(0,1).unwrap().kind,Relation::Cycle);
}

// R43,R45,R50,R51
#[test]
fn public_incremental_duplicate_link() {
    let mut s=Incremental::new(10,&[(0,1)]).unwrap();let b=s.insert(&[(0,1)]).unwrap();
    assert_eq!(b.steps[0].status,EdgeStatus::Duplicate);assert_eq!(b.steps[0].edge_index,1);
    assert_eq!(b.graph.arcs[0].multiplicity,2);
    let empty=s.insert(&[]).unwrap();assert!(empty.steps.is_empty());assert_eq!(empty.graph,b.graph);
}

// R38,R45,R46
#[test]
fn public_incremental_internal_self_loop() {
    let mut s=Incremental::new(2,&[]).unwrap();let b=s.insert(&[(1,1)]).unwrap();
    assert_eq!(b.steps[0].status,EdgeStatus::Internal);assert!(b.merges.is_empty());
    assert_eq!(b.graph.cyclic,vec![false,true]);assert_eq!(b.graph.internal_edges,vec![0,1]);
    assert_eq!(b.graph.topo,vec![1,0]);assert!(s.history().is_empty());
}
