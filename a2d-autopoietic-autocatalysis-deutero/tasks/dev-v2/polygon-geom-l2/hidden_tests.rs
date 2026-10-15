fn p(x: i64, y: i64) -> Point { Point { x, y } }
// R1-R5, R16
#[test]
fn hidden_raw_cap_precedence() {
let mut v=vec![p(0,0);4097]; v[0]=p(i64::MIN,0);
assert_eq!(measure(&v),Err(GeomError::TooManyVertices { len:4097 }));
}

// R3, R5, R7, R16
#[test]
fn hidden_range_before_minimum() {
assert_eq!(measure(&[p(0,0),p(i64::MAX,0)]),Err(GeomError::OutOfRange { index:1 }));
assert_eq!(measure(&[p(0,i64::MIN)]),Err(GeomError::OutOfRange { index:0 }));
}

// R3, R5
#[test]
fn hidden_first_invalid_raw_index() {
assert_eq!(measure(&[p(0,0),p(0,1_000_000_001),p(-1_000_000_001,0)]),Err(GeomError::OutOfRange { index:1 }));
}

// R6-R7
#[test]
fn hidden_minimum_lengths() {
for (v,n) in [(vec![],0),(vec![p(1,1)],1),(vec![p(0,0),p(1,0)],2),(vec![p(2,2),p(2,2)],1)] {
assert_eq!(measure(&v),Err(GeomError::TooFewVertices { len:n })); }
}

// R6-R8, R13
#[test]
fn hidden_single_closure_removal() {
let v=[p(1,1),p(4,1),p(4,5),p(1,1)]; let m=measure(&v).unwrap();
assert_eq!(m.edge_milli,vec![3000,4000,5000]); assert_eq!(m.signed_area2,12);
}

// R6, R8-R9, R12
#[test]
fn hidden_exactly_one_closure() {
let v=[p(0,0),p(3,0),p(0,4),p(0,0),p(0,0)];
assert_eq!(measure(&v).unwrap().edge_milli,vec![3000,5000,4000,0]);
assert_eq!(measure(&[p(2,2);4]).unwrap().edge_milli,vec![0,0,0]);
}

// R10-R11, R15
#[test]
fn hidden_signed_and_reversed() {
let a=measure(&[p(-2,-1),p(5,-1),p(-2,2)]).unwrap();
let b=measure(&[p(-2,2),p(5,-1),p(-2,-1)]).unwrap();
assert_eq!(a.signed_area2,21); assert_eq!(b.signed_area2,-21);
assert_eq!(a.orientation,Orientation::CounterClockwise); assert_eq!(b.orientation,Orientation::Clockwise);
assert_eq!(b.area2,21); assert_eq!(a.perimeter_milli,b.perimeter_milli);
}

// R9-R11
#[test]
fn hidden_cancelled_area() {
let m=measure(&[p(0,0),p(6,4),p(0,4),p(6,0)]).unwrap();
assert_eq!(m.signed_area2,0); assert_eq!(m.area2,0); assert_eq!(m.orientation,Orientation::Degenerate);
}

// R8, R12-R13
#[test]
fn hidden_per_edge_rounding() {
let m=measure(&[p(0,0),p(1,1),p(2,0)]).unwrap();
assert_eq!(m.edge_milli,vec![1414,1414,2000]); assert_eq!(m.perimeter_milli,4828);
}

// R12-R13
#[test]
fn hidden_round_up_and_down() {
let m=measure(&[p(0,0),p(1,3),p(3,0)]).unwrap();
assert_eq!(m.edge_milli,vec![3162,3606,3000]); assert_eq!(m.perimeter_milli,9768);
}

// R14
#[test]
fn hidden_longest_endpoint_tie() {
let m=measure(&[p(5,5),p(0,5),p(0,0),p(5,0)]).unwrap();
assert_eq!(m.longest_edge,Some(1));
}

// R12, R14
#[test]
fn hidden_rounded_not_exact_longest() {
let m=measure(&[p(0,0),p(1_000_000_000,0),p(0,1)]).unwrap();
assert_eq!(m.edge_milli,vec![1_000_000_000_000,1_000_000_000_000,1000]);
assert_eq!(m.longest_edge,Some(0));
}

// R9, R14
#[test]
fn hidden_duplicate_edge_tie() {
let m=measure(&[p(0,0),p(2,0),p(0,0),p(2,0)]).unwrap(); assert_eq!(m.longest_edge,Some(0));
}

// R9, R11-R14
#[test]
fn hidden_zero_edges() {
let m=measure(&[p(7,-3);5]).unwrap(); assert_eq!(m.longest_edge,Some(0));
assert_eq!(m.edge_milli,vec![0;4]); assert_eq!(m.perimeter_milli,0); assert_eq!(m.orientation,Orientation::Degenerate);
}

// R3, R10, R12-R13
#[test]
fn hidden_extreme_exact_area() {
let b=1_000_000_000; let m=measure(&[p(-b,-b),p(b,-b),p(b,b),p(-b,b)]).unwrap();
assert_eq!(m.signed_area2,8_000_000_000_000_000_000i128); assert_eq!(m.area2,8_000_000_000_000_000_000u128);
assert_eq!(m.perimeter_milli,8_000_000_000_000u64);
}

// R9-R10
#[test]
fn hidden_concave_area() {
let m=measure(&[p(0,0),p(7,0),p(7,5),p(3,2),p(0,5)]).unwrap(); assert_eq!(m.area2,49);
}

// R10, R12, R15-R16
#[test]
fn hidden_translated_exactly() {
let v=[p(0,0),p(4,1),p(1,5)]; let copy=v;
let a=measure(&v).unwrap(); let w:Vec<_>=v.iter().map(|q|p(q.x+999_999_990,q.y-999_999_990)).collect();
let b=measure(&w).unwrap(); assert_eq!(a,b); assert_eq!(v,copy); assert_eq!(a.area2,19);
}

// R8-R13
#[test]
fn hidden_collinear_backtracking() {
let m=measure(&[p(8,0),p(-2,0),p(3,0),p(5,0)]).unwrap();
assert_eq!(m.edge_milli,vec![10000,5000,2000,3000]); assert_eq!(m.area2,0);
}

// R4, R6, R9
#[test]
fn hidden_raw_cap_inclusive() {
let v=vec![p(0,0);4096]; let m=measure(&v).unwrap(); assert_eq!(m.edge_milli.len(),4095); assert_eq!(m.perimeter_milli,0);
}

// R10-R13
#[test]
fn hidden_negative_sloped_edges() {
let m=measure(&[p(-4,-3),p(-1,-2),p(-2,2)]).unwrap(); assert_eq!(m.signed_area2,13); assert_eq!(m.edge_milli,vec![3162,4123,5385]);
}

// R17-R19, R16
#[test]
fn hidden_query_error_order() {
let v=[p(0,0),p(5,0),p(0,5)]; let bad=vec![p(i64::MAX,0);4097];
assert_eq!(classify(&[],&bad),Err(GeomError::TooFewVertices { len:0 }));
assert_eq!(classify(&v,&bad),Err(GeomError::TooManyQueries { len:4097 }));
assert_eq!(classify(&v,&[p(1,1),p(0,i64::MIN),p(i64::MAX,0)]),Err(GeomError::QueryOutOfRange { index:1 }));
assert_eq!(classify(&v,&[]),Ok(vec![])); assert_eq!(classify(&[],&[]),Err(GeomError::TooFewVertices { len:0 }));
}

// R20, R24, R6, R8
#[test]
fn hidden_boundary_ownership() {
let v=[p(0,0),p(8,0),p(8,6),p(0,6),p(0,0)];
assert_eq!(classify(&v,&[p(0,0),p(8,0),p(8,6),p(0,6),p(0,3),p(4,6)]).unwrap(),vec![Location::Boundary{edge:0},Location::Boundary{edge:0},Location::Boundary{edge:1},Location::Boundary{edge:2},Location::Boundary{edge:3},Location::Boundary{edge:2}]);
}

// R19, R21-R22, R24
#[test]
fn hidden_halfopen_concavity() {
let v=[p(0,0),p(8,0),p(8,8),p(4,4),p(0,8)];
assert_eq!(classify(&v,&[p(-1,4),p(1,4),p(7,4),p(4,6),p(4,3),p(9,0),p(1,4)]).unwrap(),vec![Location::Outside,Location::Inside,Location::Inside,Location::Outside,Location::Inside,Location::Outside,Location::Inside]);
let w:Vec<_>=v.iter().copied().rev().collect(); assert_eq!(classify(&w,&[p(1,4),p(4,6)]).unwrap(),vec![Location::Inside,Location::Outside]);
}

// R9, R20, R23-R24
#[test]
fn hidden_degenerate_overlap_query() {
let v=[p(0,0),p(8,0),p(3,0),p(3,0)];
assert_eq!(classify(&v,&[p(3,0),p(7,0),p(3,1),p(9,0)]).unwrap(),vec![Location::Boundary{edge:0},Location::Boundary{edge:0},Location::Outside,Location::Outside]);
assert_eq!(classify(&[p(2,2);4],&[p(2,2),p(2,3)]).unwrap(),vec![Location::Boundary{edge:0},Location::Outside]);
}

// R21-R24
#[test]
fn hidden_self_crossing_even_odd() {
let v=[p(0,0),p(8,8),p(0,8),p(8,0)];
assert_eq!(classify(&v,&[p(4,4),p(4,1),p(4,7),p(1,4)]).unwrap(),vec![Location::Boundary{edge:0},Location::Inside,Location::Inside,Location::Outside]);
let twice=[p(0,0),p(8,0),p(8,8),p(0,8),p(0,0),p(8,0),p(8,8),p(0,8)];
assert_eq!(classify(&twice,&[p(4,4)]).unwrap(),vec![Location::Outside]);
}

// R3, R18, R21-R22
#[test]
fn hidden_query_exact_at_extremes() {
let b=1_000_000_000; let v=[p(-b,-b),p(b,-b),p(-b,b)];
assert_eq!(classify(&v,&[p(0,-1),p(0,0),p(0,1),p(b,b)]).unwrap(),vec![Location::Inside,Location::Boundary{edge:1},Location::Outside,Location::Outside]);
let queries=vec![p(0,-1);4096]; assert_eq!(classify(&v,&queries).unwrap(),vec![Location::Inside;4096]);
}

// R20-R22, R24
#[test]
fn hidden_vertex_ray_and_horizontal() {
let v=[p(0,3),p(3,0),p(6,3),p(3,6)];
assert_eq!(classify(&v,&[p(-1,3),p(1,3),p(5,3),p(7,3),p(3,0),p(3,6)]).unwrap(),vec![Location::Outside,Location::Inside,Location::Inside,Location::Outside,Location::Boundary{edge:0},Location::Boundary{edge:2}]);
}
