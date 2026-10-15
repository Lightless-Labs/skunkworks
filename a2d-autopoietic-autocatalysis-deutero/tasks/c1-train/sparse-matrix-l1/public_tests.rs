fn make(r: usize, c: usize, items: &[(usize,usize,f64)]) -> Sparse {
    let es: Vec<_> = items.iter().map(|&(row,col,value)| Entry{row,col,value}).collect();
    Sparse::from_coo(r,c,&es).unwrap()
}
#[test]
fn public_construct() {
let s=make(2,3,&[(1,2,4.0),(0,1,2.0),(1,2,3.0)]); assert_eq!(s.shape(),(2,3)); assert_eq!(s.entries(),&[Entry{row:0,col:1,value:2.0},Entry{row:1,col:2,value:7.0}]);
}

#[test]
fn public_cancel() {
let s=make(2,2,&[(0,0,5.0),(0,0,-5.0),(1,1,6.0)]); assert_eq!(s.entries(),&[Entry{row:1,col:1,value:6.0}]);
}

#[test]
fn public_bounds() {
assert_eq!(Sparse::from_coo(1,1,&[Entry{row:0,col:1,value:3.0}]),Err(Error::Bounds{entry:0,axis:Axis::Column,index:1,limit:1}));
}
