// R9-R13
#[test]
fn public_cycle_and_tail() {
    let g = analyze(4, &[(0,1),(1,0),(1,2)]).unwrap();
    assert_eq!(g.components, vec![vec![0,1],vec![2],vec![3]]);
    assert_eq!(g.component_of, vec![0,0,1,2]);
    assert_eq!(g.cyclic, vec![true,false,false]);
}

// R19-R26
#[test]
fn public_condensation_counts() {
    let g=analyze(4,&[(0,1),(1,0),(0,2),(1,2),(0,2)]).unwrap();
    assert_eq!(g.internal_edges,vec![2,0,0]);
    assert_eq!(g.arcs,vec![Arc{from:0,to:1,multiplicity:3,first_edge:2}]);
}

// R16
#[test]
fn public_two_isolates() {
    let g = analyze(2, &[]).unwrap();
    assert_eq!(g.components, vec![vec![0],vec![1]]);
    assert_eq!(g.component_of, vec![0,1]);
    assert_eq!(g.cyclic, vec![false,false]);
}

// R6
#[test]
fn public_invalid_target() {
    assert_eq!(analyze(10, &[(2,10)]), Err(GraphError::BadEndpoint { edge:0, side:1, vertex:10 }));
}
