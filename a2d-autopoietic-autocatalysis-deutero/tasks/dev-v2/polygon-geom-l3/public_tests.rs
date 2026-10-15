fn p(x: i64, y: i64) -> Point { Point { x, y } }
// R8-R14
#[test]
fn public_triangle() {
let m=measure(&[p(0,0),p(4,0),p(0,3)]).unwrap(); assert_eq!(m.signed_area2,12); assert_eq!(m.area2,12); assert_eq!(m.orientation,Orientation::CounterClockwise); assert_eq!(m.edge_milli,vec![4000,5000,3000]); assert_eq!(m.perimeter_milli,12000); assert_eq!(m.longest_edge,Some(1));
}

// R19-R22
#[test]
fn public_classification() {
assert_eq!(classify(&[p(0,0),p(10,0),p(10,10),p(0,10)],&[p(5,5),p(11,5),p(10,5)]).unwrap(),vec![Location::Inside,Location::Outside,Location::Boundary{edge:1}]);
}

// R26-R30
#[test]
fn public_hull() {
let v=[p(5,5),p(0,0),p(5,0),p(0,5),p(2,2)]; for mode in [Collinear::Include,Collinear::Exclude] { let h=convex_hull(&v,mode).unwrap(); assert_eq!(h.vertices,vec![p(0,0),p(5,0),p(5,5),p(0,5)]); assert_eq!(h.source_indices,vec![1,2,0,3]); assert_eq!(h.area2,50); assert_eq!(h.perimeter_milli,20000); }
}

// R7
#[test]
fn public_short_error() {
assert_eq!(measure(&[p(9,9),p(10,10)]),Err(GeomError::TooFewVertices{len:2}));
}
