fn q(n: i64, d: i64) -> Rational { Rational::new(n, d).unwrap() }
fn pe(at: usize, kind: ParseKind) -> RationalError { RationalError::Parse { at, kind } }
fn mat(rows: usize, cols: usize, nums: &[i64]) -> Matrix {
    Matrix::new(rows, cols, nums.iter().map(|&n| q(n, 1)).collect()).unwrap()
}
fn fm(rows: usize, cols: usize, entries: &[(i64, i64)]) -> Matrix {
    Matrix::new(rows, cols, entries.iter().map(|&(n,d)| q(n,d)).collect()).unwrap()
}
fn ae(row: usize, col: usize, step: usize) -> MatrixError {
    MatrixError::Arithmetic { row, col, step, source: RationalError::Overflow }
}
fn public_api_smoke() {
    let a=q(3,11); let b=q(2,11);
    assert_eq!((a.numerator(),a.denominator()),(3,11));
    assert_eq!(a.add(&b),Ok(q(5,11))); assert_eq!(a.sub(&b),Ok(q(1,11)));
    assert_eq!(a.mul(&b),Ok(q(6,121))); assert_eq!(a.div(&b),Ok(q(3,2)));
    assert_eq!(a.neg(),Ok(q(-3,11))); assert_eq!(a.to_string(),"3/11");
    assert_eq!(Rational::parse("3/11"),Ok(a));
    let m=Matrix::new(1,2,vec![a,b]).unwrap();
    assert_eq!((m.rows(),m.cols()),(1,2)); assert_eq!(m.data(),&[a,b]);
    assert_eq!(m.get(0,1),Ok(b));
}
// R2–R8,R17–R20
#[test]
fn public_rational_operations() {
    public_api_smoke();
    assert_eq!(q(2,7).add(&q(1,7)),Ok(q(3,7)));
    assert_eq!(Rational::parse("-4/7").unwrap().to_string(),"-4/7");
    assert_eq!(q(2,7).mul(&q(7,3)),Ok(q(2,3)));
}

// R17–R21
#[test]
fn public_matrix_access_and_transpose() {
    let a=mat(2,3,&[1,2,3,4,5,6]); assert_eq!((a.rows(),a.cols()),(2,3));
    assert_eq!(a.data().len(),6); assert_eq!(a.get(1,2),Ok(q(6,1)));
    assert_eq!(a.transpose(),mat(3,2,&[1,4,2,5,3,6]));
}

// R22–R24
#[test]
fn public_matrix_multiplication() {
    assert_eq!(mat(2,2,&[1,2,3,4]).mul(&mat(2,1,&[5,6])),Ok(mat(2,1,&[17,39])));
    assert_eq!(mat(1,0,&[]).mul(&mat(0,2,&[])),Ok(mat(1,2,&[0,0])));
}

// R18–R20,R22
#[test]
fn public_matrix_errors() {
    assert_eq!(Matrix::new(1,2,vec![]),Err(MatrixError::DataLength{expected:2,actual:0}));
    assert_eq!(mat(1,1,&[1]).get(1,0),Err(MatrixError::Index{row:1,col:0}));
    assert_eq!(mat(1,2,&[1,2]).mul(&mat(1,1,&[1])),Err(MatrixError::DimensionMismatch{left:(1,2),right:(1,1)}));
}
