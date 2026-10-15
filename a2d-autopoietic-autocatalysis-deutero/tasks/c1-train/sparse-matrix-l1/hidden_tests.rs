fn make(r: usize, c: usize, items: &[(usize,usize,f64)]) -> Sparse {
    let es: Vec<_> = items.iter().map(|&(row,col,value)| Entry{row,col,value}).collect();
    Sparse::from_coo(r,c,&es).unwrap()
}

// R1, R2, R3, R18
#[test]
fn hidden_shape_limits() {
    {

assert_eq!(MAX_DIM, 4096);
assert_eq!(Sparse::from_coo(4097, 5000, &[]), Err(Error::Shape{axis:Axis::Row,size:4097}));
assert_eq!(Sparse::from_coo(2, 4097, &[]), Err(Error::Shape{axis:Axis::Column,size:4097}));
let m = make(4096, 4096, &[(4095,4095,3.0)]);
assert_eq!(m.clone(), m); assert_eq!(m.shape(), (4096,4096));
    }
}

// R4, R13, R15
#[test]
fn hidden_zero_shapes() {
    {

for (r,c) in [(0,0),(0,7),(8,0),(3,4)] { let m=make(r,c,&[]); assert_eq!(m.shape(),(r,c)); assert!(m.entries().is_empty()); }
assert_eq!(Sparse::from_coo(0,0,&[Entry{row:0,col:0,value:f64::NAN}]),Err(Error::Bounds{entry:0,axis:Axis::Row,index:0,limit:0}));
assert_eq!(Sparse::from_coo(2,0,&[Entry{row:1,col:0,value:0.0}]),Err(Error::Bounds{entry:0,axis:Axis::Column,index:0,limit:0}));
    }
}

// R5, R6, R16, R18
#[test]
fn hidden_bounds_precedence() {
    {

let e=[Entry{row:0,col:1,value:4.0},Entry{row:8,col:9,value:f64::INFINITY}];
assert_eq!(Sparse::from_coo(2,3,&e),Err(Error::Bounds{entry:1,axis:Axis::Row,index:8,limit:2}));
let e=[Entry{row:0,col:2,value:f64::NAN}];
assert_eq!(Sparse::from_coo(1,2,&e),Err(Error::Bounds{entry:0,axis:Axis::Column,index:2,limit:2}));
    }
}

// R5, R6, R16, R18
#[test]
fn hidden_first_entry() {
    {

let e=[Entry{row:0,col:0,value:f64::NEG_INFINITY},Entry{row:99,col:99,value:2.0}];
assert_eq!(Sparse::from_coo(2,2,&e),Err(Error::NonFinite{entry:0}));
let e=[Entry{row:1,col:0,value:1.0},Entry{row:0,col:2,value:1.0},Entry{row:2,col:0,value:1.0}];
assert_eq!(Sparse::from_coo(2,2,&e),Err(Error::Bounds{entry:1,axis:Axis::Column,index:2,limit:2}));
    }
}

// R6, R16
#[test]
fn hidden_nonfinite_all() {
    {

for v in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY] {
let e=[Entry{row:0,col:0,value:1.0},Entry{row:1,col:1,value:v}];
assert_eq!(Sparse::from_coo(2,2,&e),Err(Error::NonFinite{entry:1})); }
    }
}

// R7, R9, R18
#[test]
fn hidden_validate_before_sum() {
    {

let e=[Entry{row:0,col:0,value:f64::MAX},Entry{row:0,col:0,value:f64::MAX},Entry{row:0,col:2,value:1.0}];
assert_eq!(Sparse::from_coo(1,2,&e),Err(Error::Bounds{entry:2,axis:Axis::Column,index:2,limit:2}));
let mut f=e; f[2]=Entry{row:0,col:1,value:f64::NAN};
assert_eq!(Sparse::from_coo(1,2,&f),Err(Error::NonFinite{entry:2}));
    }
}

// R8, R12, R17
#[test]
fn hidden_fold_order() {
    {

let m=make(2,3,&[(1,2,1e16),(0,1,9.0),(1,2,1.0),(1,2,-1e16),(1,2,2.0)]);
assert_eq!(m.entries(), &[Entry{row:0,col:1,value:9.0},Entry{row:1,col:2,value:2.0}]);
    }
}

// R9, R16, R17
#[test]
fn hidden_sum_overflow() {
    {

let e=[Entry{row:1,col:1,value:2.0},Entry{row:0,col:0,value:f64::MAX},Entry{row:0,col:0,value:f64::MAX},Entry{row:0,col:0,value:-f64::MAX}];
assert_eq!(Sparse::from_coo(2,2,&e),Err(Error::SumOverflow{entry:2}));
    }
}

// R9, R18
#[test]
fn hidden_negative_overflow() {
    {

let e=[Entry{row:0,col:1,value:-f64::MAX},Entry{row:0,col:0,value:f64::MAX},Entry{row:0,col:1,value:-f64::MAX},Entry{row:0,col:0,value:f64::MAX}];
assert_eq!(Sparse::from_coo(1,2,&e),Err(Error::SumOverflow{entry:2}));
    }
}

// R8, R10, R12, R13
#[test]
fn hidden_cancel_reappear() {
    {

let m=make(3,4,&[(2,3,7.0),(0,1,5.0),(2,3,-7.0),(0,1,-5.0),(2,3,4.0),(1,1,-0.0)]);
assert_eq!(m.entries(),&[Entry{row:2,col:3,value:4.0}]); assert_eq!(m.shape(),(3,4));
    }
}

// R6, R10, R11
#[test]
fn hidden_signed_zero() {
    {

let m=make(2,2,&[(0,0,-0.0),(0,1,0.0),(1,1,2.0),(1,1,-2.0)]);
assert!(m.entries().is_empty());
    }
}

// R11, R13, R17
#[test]
fn hidden_subnormal() {
    {

let tiny=f64::from_bits(1);
let m=make(2,3,&[(1,2,tiny),(0,0,-tiny),(1,0,2.0*tiny)]);
assert_eq!(m.entries().iter().map(|e|e.value.to_bits()).collect::<Vec<_>>(),vec![(-tiny).to_bits(),2,1]);
    }
}

// R8, R12, R13
#[test]
fn hidden_sort_and_merge() {
    {

let m=make(4,5,&[(3,0,2.0),(0,4,8.0),(2,2,-3.0),(0,1,4.0),(3,0,5.0),(2,2,1.0)]);
assert_eq!(m.entries(),&[Entry{row:0,col:1,value:4.0},Entry{row:0,col:4,value:8.0},Entry{row:2,col:2,value:-2.0},Entry{row:3,col:0,value:7.0}]);
    }
}

// R2, R13, R14, R15
#[test]
fn hidden_immutable() {
    {

let e=[Entry{row:2,col:0,value:3.0},Entry{row:0,col:2,value:-1.0},Entry{row:2,col:0,value:4.0}]; let saved=e;
let m=Sparse::from_coo(3,3,&e).unwrap(); let copy=m.clone();
assert_eq!(e,saved); assert_eq!(m,copy); assert_eq!(m.entries(),copy.entries());
assert_eq!(format!("{:?}",Axis::Column),"Column");
    }
}

// R8, R10, R12, R15
#[test]
fn hidden_larger_coo() {
    {

let mut e=Vec::new(); for r in (0..31).rev() { for c in (0..23).rev() { e.push(Entry{row:r,col:c,value:1.0}); e.push(Entry{row:r,col:c,value:2.0}); } }
let m=Sparse::from_coo(32,24,&e).unwrap(); assert_eq!(m.entries().len(),31*23);
for (i,x) in m.entries().iter().enumerate() { assert_eq!((x.row,x.col,x.value),(i/23,i%23,3.0)); }
    }
}

// R3, R5, R16
#[test]
fn hidden_exact_boundary() {
    {

let e=[Entry{row:4096,col:0,value:1.0}];
assert_eq!(Sparse::from_coo(4096,1,&e),Err(Error::Bounds{entry:0,axis:Axis::Row,index:4096,limit:4096}));
let e=[Entry{row:0,col:4096,value:1.0}];
assert_eq!(Sparse::from_coo(1,4096,&e),Err(Error::Bounds{entry:0,axis:Axis::Column,index:4096,limit:4096}));
    }
}

// R9, R17
#[test]
fn hidden_sum_no_rescue() {
    {

let e=[Entry{row:0,col:0,value:f64::MAX},Entry{row:0,col:0,value:f64::MAX},Entry{row:0,col:0,value:-f64::MAX},Entry{row:0,col:0,value:-f64::MAX}];
assert_eq!(Sparse::from_coo(1,1,&e),Err(Error::SumOverflow{entry:1}));
    }
}

// R6, R8, R11, R17
#[test]
fn hidden_finite_extremes() {
    {

let m=make(1,4,&[(0,3,f64::MIN_POSITIVE),(0,2,-f64::MAX),(0,1,f64::MAX),(0,0,f64::from_bits(1))]);
assert_eq!(m.entries().len(),4); assert_eq!(m.entries()[2].value,-f64::MAX);
    }
}

// R5, R16, R18
#[test]
fn hidden_earliest_original_index() {
    {

let e=[Entry{row:2,col:2,value:1.0},Entry{row:1,col:1,value:2.0},Entry{row:0,col:3,value:1.0},Entry{row:3,col:0,value:1.0}];
assert_eq!(Sparse::from_coo(3,3,&e),Err(Error::Bounds{entry:2,axis:Axis::Column,index:3,limit:3}));
    }
}

// R4, R10, R13
#[test]
fn hidden_all_cancelled_shape() {
    {

let m=make(7,9,&[(6,8,12.0),(6,8,-12.0)]); assert_eq!(m.shape(),(7,9)); assert!(m.entries().is_empty());
    }
}

// R8, R10, R12
#[test]
fn hidden_separated_duplicates() {
    {

let m=make(2,2,&[(1,0,1.0),(0,1,4.0),(1,0,2.0),(0,1,-4.0),(1,0,8.0)]);
assert_eq!(m.entries(),&[Entry{row:1,col:0,value:11.0}]);
    }
}

// R10, R11, R17
#[test]
fn hidden_no_epsilon() {
    {

let v=f64::EPSILON; let m=make(1,2,&[(0,1,1.0),(0,1,v),(0,1,-1.0),(0,0,-1e-300)]);
assert_eq!(m.entries()[1].value,v); assert_eq!(m.entries()[0].value,-1e-300);
    }
}
