fn make(r: usize, c: usize, items: &[(usize,usize,f64)]) -> Sparse {
    let es: Vec<_> = items.iter().map(|&(row,col,value)| Entry{row,col,value}).collect();
    Sparse::from_coo(r,c,&es).unwrap()
}
#[test]
fn public_construct() {
let s=make(2,3,&[(1,2,4.0),(0,1,2.0),(1,2,3.0)]); assert_eq!(s.shape(),(2,3)); assert_eq!(s.entries(),&[Entry{row:0,col:1,value:2.0},Entry{row:1,col:2,value:7.0}]);
}

#[test]
fn public_csr() {
let s=make(2,3,&[(0,0,2.0),(0,2,3.0),(1,1,-1.0)]); let a=s.to_csr(); assert_eq!(a.row_ptr(),&[0,2,3]); assert_eq!(a.col_indices(),&[0,2,1]); assert_eq!(a.values(),&[2.0,3.0,-1.0]); assert_eq!(a.to_coo(),s); assert_eq!(a.shape(),(2,3)); assert_eq!(a.mul_vector(&[4.0,5.0,2.0]).unwrap(),vec![14.0,-5.0]); let restored=Csr::from_parts(2,3,a.row_ptr().to_vec(),a.col_indices().to_vec(),a.values().to_vec()).unwrap(); assert_eq!(restored,a);
}

#[test]
fn public_ops() {
let a=make(2,2,&[(0,1,2.0),(1,0,3.0)]).to_csr(); let t=a.transpose(); assert_eq!(t.to_coo(),make(2,2,&[(0,1,3.0),(1,0,2.0)])); assert_eq!(a.multiply(&t).unwrap().to_coo(),make(2,2,&[(0,0,4.0),(1,1,9.0)]));
}

#[test]
fn public_prune_text() {
let a=make(2,2,&[(0,0,0.25),(1,1,2.0)]).to_csr().prune(0.5).unwrap(); assert_eq!(a.to_coo(),make(2,2,&[(1,1,2.0)])); let text="SM1 2 2 1\n1 1 4000000000000000\n"; assert_eq!(a.write_text(),text); assert_eq!(Csr::parse_text(text).unwrap(),a);
}
