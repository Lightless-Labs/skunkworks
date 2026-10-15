// R9-R13
#[test]
fn public_cycle_and_tail() {
    let g = analyze(4, &[(0,1),(1,0),(1,2)]).unwrap();
    assert_eq!(g.components, vec![vec![0,1],vec![2],vec![3]]);
    assert_eq!(g.component_of, vec![0,0,1,2]);
    assert_eq!(g.cyclic, vec![true,false,false]);
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

// R13,R16
#[test]
fn public_isolates_and_loop() {
    let g = analyze(10, &[(8,8)]).unwrap();
    assert_eq!(g.components, (0..10).map(|v|vec![v]).collect::<Vec<_>>());
    assert_eq!(g.cyclic, vec![false,false,false,false,false,false,false,false,true,false]);
}
