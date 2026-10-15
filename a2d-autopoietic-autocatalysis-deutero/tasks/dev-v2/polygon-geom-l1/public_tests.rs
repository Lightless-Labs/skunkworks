fn p(x: i64, y: i64) -> Point { Point { x, y } }
// R8-R14
#[test]
fn public_triangle() {
let m=measure(&[p(0,0),p(4,0),p(0,3)]).unwrap(); assert_eq!(m.signed_area2,12); assert_eq!(m.area2,12); assert_eq!(m.orientation,Orientation::CounterClockwise); assert_eq!(m.edge_milli,vec![4000,5000,3000]); assert_eq!(m.perimeter_milli,12000); assert_eq!(m.longest_edge,Some(1));
}

// R6, R10-R11
#[test]
fn public_clockwise_closed() {
let m=measure(&[p(0,0),p(0,2),p(2,0),p(0,0)]).unwrap(); assert_eq!(m.signed_area2,-4); assert_eq!(m.orientation,Orientation::Clockwise); assert_eq!(m.edge_milli.len(),3);
}

// R7
#[test]
fn public_short_error() {
assert_eq!(measure(&[p(9,9),p(10,10)]),Err(GeomError::TooFewVertices{len:2}));
}

// R9-R11
#[test]
fn public_collinear() {
let m=measure(&[p(0,0),p(3,0),p(7,0)]).unwrap(); assert_eq!(m.area2,0); assert_eq!(m.orientation,Orientation::Degenerate); assert_eq!(m.perimeter_milli,14000);
}
