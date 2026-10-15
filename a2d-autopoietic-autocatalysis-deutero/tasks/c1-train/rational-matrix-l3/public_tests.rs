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
// R2–R8,R17–R24
#[test]
fn public_basic_matrix_and_rational() {
    public_api_smoke();
    assert_eq!(Rational::parse("2/9"),Ok(q(2,9)));
    assert_eq!(mat(1,2,&[2,3]).mul(&mat(2,1,&[4,5])),Ok(mat(1,1,&[23])));
    assert_eq!(mat(1,2,&[2,3]).transpose(),mat(2,1,&[2,3]));
}

// R25–R30
#[test]
fn public_reduced_form_and_rank() {
    let a=mat(2,3,&[1,2,3,2,4,6]); let red=a.rref().unwrap();
    assert_eq!(red.matrix,mat(2,3,&[1,2,3,0,0,0]));
    assert_eq!(red.pivots,vec![0]); assert!(red.swaps.is_empty());
    assert_eq!(red.rank(),1); assert_eq!(a.rank(),Ok(1));
}

// R31–R32
#[test]
fn public_inverse() {
    assert_eq!(mat(2,2,&[1,2,0,1]).inverse(),Ok(mat(2,2,&[1,-2,0,1])));
    assert_eq!(mat(1,1,&[2]).inverse(),Ok(fm(1,1,&[(1,2)])));
}

// R31–R32
#[test]
fn public_singular_and_nonsquare() {
    assert_eq!(mat(2,2,&[1,2,2,4]).inverse(),Err(MatrixError::Singular{rank:1}));
    assert_eq!(mat(1,2,&[1,2]).inverse(),Err(MatrixError::NotSquare{rows:1,cols:2}));
}
