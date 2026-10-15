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

// R12, R14, R19, R20
#[test]
fn hidden_csr_roundtrip() {
    {

let s=make(5,6,&[(4,1,2.0),(1,5,-3.0),(1,0,7.0),(4,1,1.0)]); let a=s.to_csr();
assert_eq!(a.row_ptr(),&[0,0,2,2,2,3]); assert_eq!(a.col_indices(),&[0,5,1]); assert_eq!(a.values(),&[7.0,-3.0,3.0]);
assert_eq!(a.shape(),(5,6)); assert_eq!(a.to_coo(),s); assert_eq!(a.clone(),a);
    }
}

// R4, R19, R20, R22
#[test]
fn hidden_csr_empty() {
    {

for (r,c) in [(0,0),(0,5),(4,0),(4,3)] { let a=make(r,c,&[]).to_csr(); assert_eq!(a.row_ptr(),vec![0;r+1]); assert_eq!(a.to_coo().shape(),(r,c)); assert_eq!(Csr::from_parts(r,c,vec![0;r+1],vec![],vec![]).unwrap(),a); }
    }
}

// R3, R18, R21
#[test]
fn hidden_parts_lengths() {
    {

assert_eq!(Csr::from_parts(4097,0,vec![],vec![],vec![]),Err(Error::Shape{axis:Axis::Row,size:4097}));
assert_eq!(Csr::from_parts(1,2,vec![0],vec![0],vec![]),Err(Error::RowPtrLength{expected:2,actual:1}));
assert_eq!(Csr::from_parts(1,2,vec![7,8],vec![0,1],vec![1.0]),Err(Error::ColumnValueLength{columns:2,values:1}));
    }
}

// R18, R22, R23
#[test]
fn hidden_parts_endpoints() {
    {

assert_eq!(Csr::from_parts(1,1,vec![2,0],vec![],vec![]),Err(Error::RowPtrStart{found:2}));
assert_eq!(Csr::from_parts(2,1,vec![0,usize::MAX,2],vec![0],vec![1.0]),Err(Error::RowPtrEnd{found:2,expected:1}));
assert_eq!(Csr::from_parts(0,1,vec![0],vec![0],vec![1.0]),Err(Error::RowPtrEnd{found:0,expected:1}));
    }
}

// R15, R23, R24
#[test]
fn hidden_parts_monotonic() {
    {

assert_eq!(Csr::from_parts(3,1,vec![0,usize::MAX,0,1],vec![99],vec![f64::NAN]),Err(Error::RowPtrOrder{row:1}));
assert_eq!(Csr::from_parts(3,1,vec![0,1,0,1],vec![0],vec![1.0]),Err(Error::RowPtrOrder{row:1}));
    }
}

// R16, R18, R24
#[test]
fn hidden_parts_slot_precedence() {
    {

assert_eq!(Csr::from_parts(1,2,vec![0,2],vec![1,2],vec![1.0,f64::NAN]),Err(Error::ColumnBounds{slot:1,column:2,limit:2}));
assert_eq!(Csr::from_parts(1,3,vec![0,2],vec![2,1],vec![1.0,f64::NAN]),Err(Error::ColumnOrder{slot:1}));
assert_eq!(Csr::from_parts(1,3,vec![0,2],vec![1,1],vec![1.0,0.0]),Err(Error::ColumnOrder{slot:1}));
    }
}

// R6, R11, R24
#[test]
fn hidden_parts_values() {
    {

for v in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY] { assert_eq!(Csr::from_parts(2,2,vec![0,0,1],vec![1],vec![v]),Err(Error::NonFiniteValue{slot:0})); }
for v in [0.0,-0.0] { assert_eq!(Csr::from_parts(1,1,vec![0,1],vec![0],vec![v]),Err(Error::ExplicitZero{slot:0})); }
assert!(Csr::from_parts(1,1,vec![0,1],vec![0],vec![f64::from_bits(1)]).is_ok());
    }
}

// R20, R24
#[test]
fn hidden_parts_row_reset() {
    {

let a=Csr::from_parts(3,4,vec![0,2,2,3],vec![1,3,0],vec![2.0,5.0,-1.0]).unwrap();
assert_eq!(a.to_coo(),make(3,4,&[(0,1,2.0),(0,3,5.0),(2,0,-1.0)]));
    }
}

// R16, R18, R25
#[test]
fn hidden_vector_validation() {
    {

let a=make(2,3,&[(0,0,f64::MAX)]).to_csr();
assert_eq!(a.mul_vector(&[f64::NAN]),Err(Error::VectorLength{expected:3,actual:1}));
assert_eq!(a.mul_vector(&[2.0,f64::INFINITY,f64::NAN]),Err(Error::NonFiniteVector{index:1}));
assert_eq!(make(0,2,&[]).to_csr().mul_vector(&[1.0,f64::NAN]),Err(Error::NonFiniteVector{index:1}));
    }
}

// R16, R26, R27
#[test]
fn hidden_vector_overflows() {
    {

let a=make(3,4,&[(1,2,f64::MAX),(2,0,f64::MAX)]).to_csr();
assert_eq!(a.mul_vector(&[2.0,0.0,-2.0,0.0]),Err(Error::ProductOverflow{row:1,column:2}));
let a=make(2,3,&[(1,0,f64::MAX),(1,1,f64::MAX),(1,2,-f64::MAX)]).to_csr();
assert_eq!(a.mul_vector(&[1.0,1.0,1.0]),Err(Error::AccumulationOverflow{row:1,column:1}));
    }
}

// R14, R17, R27, R28
#[test]
fn hidden_vector_rounding() {
    {

let a=make(3,4,&[(1,0,1e16),(1,1,1.0),(1,2,-1e16),(1,3,3.0),(2,2,-1.0)]).to_csr(); let old=a.clone(); let x=[1.0,1.0,1.0,1.0];
assert_eq!(a.mul_vector(&x).unwrap(),vec![0.0,3.0,-1.0]); assert_eq!(a,old); assert_eq!(x,[1.0;4]);
let z=make(2,1,&[(1,0,-f64::from_bits(1))]).to_csr().mul_vector(&[0.25]).unwrap();
assert!(z.iter().all(|v|v.to_bits()==0));
assert_eq!(make(3,0,&[]).to_csr().mul_vector(&[]).unwrap(),vec![0.0;3]);
let d=2.0_f64.powi(-27);
let separate=make(1,2,&[(0,0,-1.0),(0,1,1.0+d)]).to_csr().mul_vector(&[1.0,1.0-d]).unwrap();
assert_eq!(separate[0].to_bits(),0); // A fused second multiply-add would be -2^-54.
    }
}
