fn make(r: usize, c: usize, items: &[(usize,usize,f64)]) -> Sparse {
    let es: Vec<_> = items.iter().map(|&(row,col,value)| Entry{row,col,value}).collect();
    Sparse::from_coo(r,c,&es).unwrap()
}

// R1, R2, R3, R4, R13, R15, R18
#[test]
fn hidden_shape_limits() {
    {

assert_eq!(MAX_DIM, 4096);
assert_eq!(Sparse::from_coo(4097, 5000, &[]), Err(Error::Shape{axis:Axis::Row,size:4097}));
assert_eq!(Sparse::from_coo(2, 4097, &[]), Err(Error::Shape{axis:Axis::Column,size:4097}));
let m = make(4096, 4096, &[(4095,4095,3.0)]);
assert_eq!(m.clone(), m); assert_eq!(m.shape(), (4096,4096));
    }
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

// R8, R9, R12, R16, R17
#[test]
fn hidden_fold_order() {
    {

let m=make(2,3,&[(1,2,1e16),(0,1,9.0),(1,2,1.0),(1,2,-1e16),(1,2,2.0)]);
assert_eq!(m.entries(), &[Entry{row:0,col:1,value:9.0},Entry{row:1,col:2,value:2.0}]);
    }
    {

let e=[Entry{row:1,col:1,value:2.0},Entry{row:0,col:0,value:f64::MAX},Entry{row:0,col:0,value:f64::MAX},Entry{row:0,col:0,value:-f64::MAX}];
assert_eq!(Sparse::from_coo(2,2,&e),Err(Error::SumOverflow{entry:2}));
    }
}

// R2, R8, R10, R11, R12, R13, R14, R15, R17
#[test]
fn hidden_cancel_reappear() {
    {

let m=make(3,4,&[(2,3,7.0),(0,1,5.0),(2,3,-7.0),(0,1,-5.0),(2,3,4.0),(1,1,-0.0)]);
assert_eq!(m.entries(),&[Entry{row:2,col:3,value:4.0}]); assert_eq!(m.shape(),(3,4));
    }
    {

let tiny=f64::from_bits(1);
let m=make(2,3,&[(1,2,tiny),(0,0,-tiny),(1,0,2.0*tiny)]);
assert_eq!(m.entries().iter().map(|e|e.value.to_bits()).collect::<Vec<_>>(),vec![(-tiny).to_bits(),2,1]);
    }
    {

let e=[Entry{row:2,col:0,value:3.0},Entry{row:0,col:2,value:-1.0},Entry{row:2,col:0,value:4.0}]; let saved=e;
let m=Sparse::from_coo(3,3,&e).unwrap(); let copy=m.clone();
assert_eq!(e,saved); assert_eq!(m,copy); assert_eq!(m.entries(),copy.entries());
assert_eq!(format!("{:?}",Axis::Column),"Column");
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

// R3, R18, R21, R22, R23
#[test]
fn hidden_parts_lengths() {
    {

assert_eq!(Csr::from_parts(4097,0,vec![],vec![],vec![]),Err(Error::Shape{axis:Axis::Row,size:4097}));
assert_eq!(Csr::from_parts(1,2,vec![0],vec![0],vec![]),Err(Error::RowPtrLength{expected:2,actual:1}));
assert_eq!(Csr::from_parts(1,2,vec![7,8],vec![0,1],vec![1.0]),Err(Error::ColumnValueLength{columns:2,values:1}));
    }
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

// R6, R11, R20, R24
#[test]
fn hidden_parts_values() {
    {

for v in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY] { assert_eq!(Csr::from_parts(2,2,vec![0,0,1],vec![1],vec![v]),Err(Error::NonFiniteValue{slot:0})); }
for v in [0.0,-0.0] { assert_eq!(Csr::from_parts(1,1,vec![0,1],vec![0],vec![v]),Err(Error::ExplicitZero{slot:0})); }
assert!(Csr::from_parts(1,1,vec![0,1],vec![0],vec![f64::from_bits(1)]).is_ok());
    }
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

// R4, R19, R20, R29, R30
#[test]
fn hidden_transpose_rect() {
    {

let a=make(4,6,&[(3,4,2.0),(0,4,3.0),(2,1,-4.0),(0,0,f64::from_bits(1))]).to_csr(); let old=a.clone(); let b=a.transpose();
assert_eq!(b.shape(),(6,4)); assert_eq!(b.row_ptr(),&[0,1,2,2,2,4,4]); assert_eq!(b.col_indices(),&[0,2,0,3]);
assert_eq!(b.values(),&[f64::from_bits(1),-4.0,3.0,2.0]); assert_eq!(b.transpose(),a); assert_eq!(a,old);
    }
    {

for (r,c) in [(0,0),(0,8),(7,0),(3,5)] { let a=make(r,c,&[]).to_csr(); let b=a.transpose(); assert_eq!(b.shape(),(c,r)); assert_eq!(b.row_ptr(),vec![0;c+1]); assert_eq!(b.transpose(),a); }
    }
}

// R4, R18, R19, R31, R35
#[test]
fn hidden_multiply_dimensions() {
    {

assert_eq!(make(0,4,&[]).to_csr().multiply(&make(3,0,&[]).to_csr()),Err(Error::DimensionMismatch{left_cols:4,right_rows:3}));
let a=make(1,2,&[(0,1,f64::MAX)]).to_csr(); assert_eq!(a.multiply(&make(1,1,&[(0,0,f64::MAX)]).to_csr()),Err(Error::DimensionMismatch{left_cols:2,right_rows:1}));
    }
    {

for (r,k,c) in [(4,0,5),(0,3,7),(3,4,0),(2,5,3)] { let a=make(r,k,&[]).to_csr(); let b=make(k,c,&[]).to_csr(); let x=a.multiply(&b).unwrap(); assert_eq!(x.shape(),(r,c)); assert_eq!(x.row_ptr(),vec![0;r+1]); assert!(x.values().is_empty()); }
    }
}

// R14, R32, R34, R35
#[test]
fn hidden_multiply_sorted() {
    {

let a=make(3,4,&[(1,0,2.0),(1,3,3.0),(2,2,-1.0)]).to_csr();
let b=make(4,5,&[(0,4,7.0),(3,0,4.0),(3,4,-1.0),(2,2,8.0)]).to_csr(); let aa=a.clone(); let bb=b.clone(); let c=a.multiply(&b).unwrap();
assert_eq!(c.shape(),(3,5)); assert_eq!(c.row_ptr(),&[0,0,2,3]); assert_eq!(c.col_indices(),&[0,4,2]); assert_eq!(c.values(),&[12.0,11.0,-8.0]); assert_eq!(a,aa); assert_eq!(b,bb);
    }
}

// R11, R17, R32, R33, R34
#[test]
fn hidden_multiply_cancel() {
    {

let a=make(1,3,&[(0,0,1.0),(0,1,1.0),(0,2,1.0)]).to_csr(); let b=make(3,3,&[(0,0,6.0),(1,0,-6.0),(2,0,2.0),(0,1,5.0),(1,1,-5.0),(2,2,f64::from_bits(1))]).to_csr(); let c=a.multiply(&b).unwrap();
assert_eq!(c.col_indices(),&[0,2]); assert_eq!(c.values(),&[2.0,f64::from_bits(1)]);
    }
    {

let a=make(1,1,&[(0,0,-f64::from_bits(1))]).to_csr(); let b=make(1,1,&[(0,0,0.25)]).to_csr(); let c=a.multiply(&b).unwrap(); assert!(c.values().is_empty()); assert_eq!(c.row_ptr(),&[0,0]);
    }
}

// R17, R32, R34
#[test]
fn hidden_multiply_rounding() {
    {

let a=make(1,4,&[(0,0,1.0),(0,1,1.0),(0,2,1.0),(0,3,1.0)]).to_csr();
let b=make(4,2,&[(0,1,1e16),(1,1,1.0),(2,1,-1e16),(3,1,7.0)]).to_csr();
assert_eq!(a.multiply(&b).unwrap().values(),&[7.0]);
    }
}

// R16, R18, R32, R33
#[test]
fn hidden_multiply_product_error() {
    {

let a=make(2,3,&[(1,0,f64::MAX),(1,1,f64::MAX)]).to_csr();
let b=make(3,4,&[(0,3,2.0),(1,0,2.0)]).to_csr();
assert_eq!(a.multiply(&b),Err(Error::MultiplyOverflow{row:1,inner:0,column:3}));
    }
    {

let a=make(2,3,&[(1,0,1.0),(1,1,1.0),(1,2,1.0)]).to_csr();
let b=make(3,2,&[(0,1,f64::MAX),(1,1,f64::MAX),(2,1,-f64::MAX)]).to_csr();
assert_eq!(a.multiply(&b),Err(Error::MultiplySumOverflow{row:1,inner:1,column:1}));
    }
}

// R18, R36
#[test]
fn hidden_prune_bad() {
    {

for t in [-1.0,f64::INFINITY,f64::NEG_INFINITY,f64::NAN] { assert_eq!(make(0,3,&[]).to_csr().prune(t),Err(Error::Tolerance)); }
    }
}

// R37, R38
#[test]
fn hidden_prune_inclusive() {
    {

let a=make(4,4,&[(0,1,-2.0),(0,2,2.0),(1,0,1.5),(2,1,-3.0),(3,3,4.0)]).to_csr(); let old=a.clone(); let b=a.prune(2.0).unwrap();
assert_eq!(b.row_ptr(),&[0,0,0,1,2]); assert_eq!(b.col_indices(),&[1,3]); assert_eq!(b.values(),&[-3.0,4.0]); assert_eq!(b.shape(),(4,4)); assert_eq!(a,old); assert_eq!(b.prune(2.0).unwrap(),b);
    }
}

// R11, R36, R37, R38
#[test]
fn hidden_prune_zero_tiny() {
    {

let a=make(2,2,&[(0,1,-f64::from_bits(1)),(1,0,f64::from_bits(2))]).to_csr();
assert_eq!(a.prune(-0.0).unwrap(),a); assert_eq!(a.prune(0.0).unwrap(),a);
let b=a.prune(f64::from_bits(1)).unwrap(); assert_eq!(b.row_ptr(),&[0,0,1]); assert_eq!(b.values()[0].to_bits(),2);
    }
}

// R37, R38, R41
#[test]
fn hidden_prune_all() {
    {

let a=make(3,7,&[(0,1,-f64::MAX),(2,6,1.0)]).to_csr().prune(f64::MAX).unwrap();
assert_eq!(a.row_ptr(),&[0,0,0,0]); assert_eq!(a.write_text(),"SM1 3 7 0\n");
    }
}

// R11, R39, R40, R50
#[test]
fn hidden_text_bits() {
    {

let a=make(2,4,&[(1,3,-1.0),(0,2,f64::from_bits(1)),(1,0,0.5)]).to_csr();
let s="SM1 2 4 3\n0 2 0000000000000001\n1 0 3fe0000000000000\n1 3 bff0000000000000\n";
assert_eq!(a.write_text(),s); assert_eq!(Csr::parse_text(s).unwrap(),a); assert_eq!(Csr::parse_text(s).unwrap().write_text(),s);
    }
}

// R4, R39, R41, R50
#[test]
fn hidden_text_empty() {
    {

for (r,c) in [(0,0),(0,12),(9,0),(4096,4096)] { let a=make(r,c,&[]).to_csr(); let s=format!("SM1 {} {} 0\n",r,c); assert_eq!(a.write_text(),s); assert_eq!(Csr::parse_text(&s).unwrap(),a); }
    }
}

// R16, R18, R42
#[test]
fn hidden_text_newline() {
    {

for (s,line) in [("",1),("bad",1),("bad\nno",2),("SM1 1 1 1\n0 0 3ff0000000000000",2)] { assert_eq!(Csr::parse_text(s),Err(Error::Text{line,kind:TextError::Newline})); }
    }
}

// R18, R43
#[test]
fn hidden_text_header() {
    {

for s in ["sm1 1 1 0\n"," SM1 1 1 0\n","SM1  1 1 0\n","SM1\t1 1 0\n","\n","\u{feff}SM1 1 1 0\n","SM1 1 1 0 \n"] { assert_eq!(Csr::parse_text(s),Err(Error::Text{line:1,kind:TextError::Header})); }
    }
}

// R18, R44, R45
#[test]
fn hidden_text_decimals() {
    {

for s in ["SM1 01 1 0\n","SM1 +1 1 0\n","SM1 1 -0 0\n","SM1 1 1 00\n","SM1 1 1 184467440737095516160\n","SM1 9999 1 +2\n","SM1 １ 1 0\n","SM1 1 1 0\r\n"] { assert_eq!(Csr::parse_text(s),Err(Error::Text{line:1,kind:TextError::Decimal})); }
    }
}

// R18, R45, R46
#[test]
fn hidden_text_dimensions() {
    {

for s in ["SM1 4097 1 1\nbad\n","SM1 0 4097 0\n"] { assert_eq!(Csr::parse_text(s),Err(Error::Text{line:1,kind:TextError::Dimension})); }
    }
}

// R18, R42, R46
#[test]
fn hidden_text_counts() {
    {

for s in ["SM1 1 1 1\n","SM1 1 1 0\nbad\n","SM1 1 1 2\nbad\n","SM1 0 0 0\n\n"] { assert_eq!(Csr::parse_text(s),Err(Error::Text{line:1,kind:TextError::Count})); }
    }
}

// R16, R18, R44, R47
#[test]
fn hidden_text_record_lexical() {
    {

for record in ["0  0 3ff0000000000000","0\t0 3ff0000000000000","0 0 3ff0000000000000 ",""] { let s=format!("SM1 1 1 1\n{}\n",record); assert_eq!(Csr::parse_text(&s),Err(Error::Text{line:2,kind:TextError::Record})); }
for record in ["00 0 3ff0000000000000","0 +0 3ff0000000000000","0 184467440737095516160 z"] { let s=format!("SM1 1 1 1\n{}\n",record); assert_eq!(Csr::parse_text(&s),Err(Error::Text{line:2,kind:TextError::Decimal})); }
    }
}

// R18, R47
#[test]
fn hidden_text_hex() {
    {

for bits in ["3FF0000000000000","3ff000000000000","03ff0000000000000","0x3ff00000000000","gg00000000000000","3ff0000000000000\r"] { let s=format!("SM1 1 1 1\n9 9 {}\n",bits); assert_eq!(Csr::parse_text(&s),Err(Error::Text{line:2,kind:TextError::Hex})); }
    }
}

// R16, R18, R48
#[test]
fn hidden_text_semantics() {
    {

for (rec,kind) in [("1 1 7ff0000000000000",TextError::RowBounds),("0 1 7ff0000000000000",TextError::ColumnBounds),("0 0 7ff0000000000000",TextError::NonFinite),("0 0 fff0000000000000",TextError::NonFinite),("0 0 7ff8000000000001",TextError::NonFinite),("0 0 0000000000000000",TextError::Zero),("0 0 8000000000000000",TextError::Zero)] { let s=format!("SM1 1 1 1\n{}\n",rec); assert_eq!(Csr::parse_text(&s),Err(Error::Text{line:2,kind})); }
    }
}

// R18, R48, R49
#[test]
fn hidden_text_order() {
    {

for rec in ["1 0 3ff0000000000000","0 1 3ff0000000000000","1 0 bff0000000000000"] { let s=format!("SM1 2 2 2\n1 0 3ff0000000000000\n{}\n",rec); assert_eq!(Csr::parse_text(&s),Err(Error::Text{line:3,kind:TextError::Order})); }
let s="SM1 2 2 2\n1 0 3ff0000000000000\n0 0 0000000000000000\n"; assert_eq!(Csr::parse_text(s),Err(Error::Text{line:3,kind:TextError::Zero}));
    }
}

// R29, R34, R38, R40, R50
#[test]
fn hidden_text_roundtrip_ops() {
    {

let a=make(3,2,&[(2,1,-2.0),(0,1,0.125),(1,0,3.0)]).to_csr(); let b=a.multiply(&a.transpose()).unwrap().prune(0.25).unwrap(); let text=b.write_text(); let parsed=Csr::parse_text(&text).unwrap(); assert_eq!(parsed,b); assert_eq!(parsed.write_text(),text); assert_eq!(parsed.transpose().transpose(),b);
    }
}
