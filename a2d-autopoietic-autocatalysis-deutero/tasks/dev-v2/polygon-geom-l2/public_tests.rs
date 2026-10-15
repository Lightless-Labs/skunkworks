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

// R19-R22
#[test]
fn public_classification() {
assert_eq!(classify(&[p(0,0),p(10,0),p(10,10),p(0,10)],&[p(5,5),p(11,5),p(10,5)]).unwrap(),vec![Location::Inside,Location::Outside,Location::Boundary{edge:1}]);
}
