// R19-R30
#[test]
fn public_topology_chain() {
    let edges:Vec<_>=(1..10).map(|v|(v-1,v)).collect();
    let g=analyze(10,&edges).unwrap();
    assert_eq!(g.components,(0..10).map(|v|vec![v]).collect::<Vec<_>>());
    assert_eq!(g.topo,(0..10).collect::<Vec<_>>());assert_eq!(g.depth,(0..10).collect::<Vec<_>>());
    assert_eq!(g.arcs.len(),9);assert_eq!(g.arcs[0],Arc{from:0,to:1,multiplicity:1,first_edge:0});
}

// R33,R34
#[test]
fn public_query_directions() {
    let edges:Vec<_>=(1..10).map(|v|(v-1,v)).collect();
    let g=analyze(10,&edges).unwrap();
    assert_eq!(g.query(0,2).unwrap(),Query{kind:Relation::Forward,route:vec![0,1,2]});
    assert_eq!(g.query(2,0).unwrap(),Query{kind:Relation::Backward,route:vec![2,1,0]});
}

// R32,R35
#[test]
fn public_cycle_and_separate_query() {
    let g=analyze(4,&[(0,1),(1,0)]).unwrap();
    assert_eq!(g.query(0,1).unwrap(),Query{kind:Relation::Cycle,route:vec![0,0]});
    assert_eq!(g.query(2,2).unwrap(),Query{kind:Relation::Trivial,route:vec![1]});
    assert_eq!(g.query(0,3).unwrap(),Query{kind:Relation::Separate,route:vec![]});
}

// R6,R31
#[test]
fn public_query_error() {
    assert_eq!(analyze(1,&[(0,1)]),Err(GraphError::BadEndpoint{edge:0,side:1,vertex:1}));
    assert_eq!(analyze(2,&[]).unwrap().query(0,2),Err(GraphError::BadQuery{side:1,vertex:2}));
}
