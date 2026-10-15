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
// R33–R38
#[test]
fn public_solver_unique() {
    assert_eq!(mat(2,2,&[1,0,0,1]).solve(&mat(2,1,&[5,8])),
        Ok(Solution::Unique{values:mat(2,1,&[5,8]),pivots:vec![0,1]}));
}

// R37–R40
#[test]
fn public_solver_infinite() {
    assert_eq!(mat(1,2,&[1,2]).solve(&mat(1,1,&[7])),Ok(Solution::Infinite{
        particular:mat(2,1,&[7,0]),free:vec![1],basis:mat(2,1,&[-2,1]),pivots:vec![0]}));
}

// R34–R36
#[test]
fn public_solver_none_and_row_error() {
    assert_eq!(mat(1,1,&[0]).solve(&mat(1,1,&[4])),
        Ok(Solution::None{row:0,rhs_col:0,witness:vec![q(0,1),q(4,1)]}));
    assert_eq!(mat(1,1,&[1]).solve(&mat(0,1,&[])),Err(SolveError::RowMismatch{coefficients:1,rhs:0}));
}

// R2–R8,R17–R32
#[test]
fn public_supporting_operations() {
    public_api_smoke();
    assert_eq!(Rational::parse("3/8").unwrap().to_string(),"3/8");
    let a=mat(2,2,&[1,0,0,1]); assert_eq!(a.transpose(),a);
    assert_eq!(a.mul(&a),Ok(a.clone())); assert_eq!(a.inverse(),Ok(a.clone()));
    let red=a.rref().unwrap(); assert_eq!(red.matrix,a); assert_eq!(red.rank(),2);
    assert_eq!(a.rank(),Ok(2));
}
