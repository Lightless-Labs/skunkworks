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
// R1–R5
#[test]
fn h_normalization_signs_zero() {
    assert_eq!(q(18,-24), q(-3,4));
    assert_eq!(q(-18,-24), q(3,4));
    assert_eq!(q(0,i64::MIN), q(0,1));
    assert_eq!(q(-3,4).numerator(), -3);
    assert_eq!(q(-3,4).denominator(), 4);
    assert_eq!(Rational::new(0,0), Err(RationalError::ZeroDenominator));
}

// R4–R5
#[test]
fn h_minimum_constructor_boundaries() {
    assert_eq!(Rational::new(i64::MIN, i64::MIN), Ok(q(1,1)));
    assert_eq!(Rational::new(2,i64::MIN), Ok(q(-1,1i64<<62)));
    assert_eq!(Rational::new(1,i64::MIN), Err(RationalError::Overflow));
    assert_eq!(Rational::new(i64::MIN,-1), Err(RationalError::Overflow));
    assert_eq!(q(i64::MIN,1).numerator(), i64::MIN);
}

// R6–R7
#[test]
fn h_signed_arithmetic() {
    let a = q(-11,15); let b = q(7,10);
    assert_eq!(a.add(&b), Ok(q(-1,30)));
    assert_eq!(a.sub(&b), Ok(q(-43,30)));
    assert_eq!(a.mul(&b), Ok(q(-77,150)));
    assert_eq!(a.div(&b), Ok(q(-22,21)));
    assert_eq!(a.neg(), Ok(q(11,15)));
    assert_eq!(a, q(-11,15)); assert_eq!(b,q(7,10));
}

// R4–R6
#[test]
fn h_reduction_before_overflow() {
    let m = i64::MAX;
    assert_eq!(q(m,2).mul(&q(2,m)), Ok(q(1,1)));
    assert_eq!(q(m,m-1).div(&q(m,m-1)), Ok(q(1,1)));
    assert_eq!(q(1,m).add(&q(m-1,m)), Ok(q(1,1)));
    assert_eq!(q(m,m-1).sub(&q(m,m-1)), Ok(q(0,1)));
}

// R6–R7
#[test]
fn h_subtraction_without_negation() {
    assert_eq!(q(i64::MIN,1).sub(&q(i64::MIN,1)), Ok(q(0,1)));
    assert_eq!(q(-1,1).sub(&q(i64::MIN,1)), Ok(q(i64::MAX,1)));
    assert_eq!(q(0,1).sub(&q(i64::MIN,1)), Err(RationalError::Overflow));
}

// R6–R7
#[test]
fn h_arithmetic_result_overflows() {
    assert_eq!(q(i64::MAX,1).add(&q(1,1)), Err(RationalError::Overflow));
    assert_eq!(q(i64::MIN,1).mul(&q(-1,1)), Err(RationalError::Overflow));
    assert_eq!(q(1,i64::MAX).mul(&q(1,2)), Err(RationalError::Overflow));
    assert_eq!(q(i64::MAX,1).div(&q(1,2)), Err(RationalError::Overflow));
    assert_eq!(q(i64::MIN,1).neg(), Err(RationalError::Overflow));
}

// R5–R7
#[test]
fn h_zero_division_precedence() {
    for n in [0, 19, i64::MIN] {
        assert_eq!(q(n,1).div(&q(0,1)), Err(RationalError::DivideByZero));
    }
    assert_eq!(q(0,1).div(&q(i64::MIN,1)), Ok(q(0,1)));
    assert_eq!(q(0,1).mul(&q(i64::MIN,1)), Ok(q(0,1)));
}

// R4,R8,R16
#[test]
fn h_display_round_trip() {
    for v in [q(0,9),q(17,1),q(-5,8),q(i64::MIN,3),q(1,i64::MAX)] {
        let expected = if v.denominator()==1 { v.numerator().to_string() }
            else { format!("{}/{}",v.numerator(),v.denominator()) };
        assert_eq!(v.to_string(),expected);
        assert_eq!(Rational::parse(&expected),Ok(v));
    }
}

// R3,R9
#[test]
fn h_empty_and_missing_digit_offsets() {
    assert_eq!(Rational::parse(""),Err(pe(0,ParseKind::Empty)));
    for (s,at) in [("-",1),("9/",2),("-9/",3),("/9",0)] {
        assert_eq!(Rational::parse(s),Err(pe(at,ParseKind::InvalidByte)));
    }
}

// R9–R10
#[test]
fn h_syntax_offsets_and_signs() {
    for (s,at) in [("+9",0),("9/-2",2),("9/+2",2),("9//2",2),("9/2/7",3),("--9",1),("9  ",1),(" 9",0),("9\n",1)] {
        assert_eq!(Rational::parse(s),Err(pe(at,ParseKind::InvalidByte)),"{s:?}");
    }
}

// R9
#[test]
fn h_utf8_byte_offsets() {
    for (s,at) in [("12/7é",4),("−12",0),("12/💡",3),("-12é",3)] {
        assert_eq!(Rational::parse(s),Err(pe(at,ParseKind::InvalidByte)));
    }
}

// R9–R10
#[test]
fn h_syntax_before_other_checks() {
    for (s,at) in [("00/0x",4),("-0/0/1",4),("999999999999999999999x",21),("1/01 ",4),("1/999999999999999999999x",23)] {
        assert_eq!(Rational::parse(s),Err(pe(at,ParseKind::InvalidByte)),"{s}");
    }
}

// R10–R11
#[test]
fn h_leading_zero_offsets() {
    for (s,at) in [("007",1),("-007",2),("7/003",3),("-7/003",4),("00/00",1),("-00/0",2)] {
        assert_eq!(Rational::parse(s),Err(pe(at,ParseKind::NonCanonical)));
    }
}

// R10–R11,R14–R15
#[test]
fn h_negative_zero_priority() {
    for s in ["-0","-0/0","-0/01","-0/999999999999999999999"] {
        assert_eq!(Rational::parse(s),Err(pe(0,ParseKind::NonCanonical)));
    }
    assert_eq!(Rational::parse("0"),Ok(q(0,1)));
}

// R12,R16
#[test]
fn h_numerator_parse_limits() {
    assert_eq!(Rational::parse("9223372036854775807"),Ok(q(i64::MAX,1)));
    assert_eq!(Rational::parse("-9223372036854775808"),Ok(q(i64::MIN,1)));
    for s in ["9223372036854775808","-9223372036854775809","999999999999999999999999999999999999999999999999999"] {
        assert_eq!(Rational::parse(s),Err(pe(0,ParseKind::OutOfRange)));
    }
}

// R12–R13
#[test]
fn h_denominator_parse_limits() {
    assert_eq!(Rational::parse("1/9223372036854775807"),Ok(q(1,i64::MAX)));
    assert_eq!(Rational::parse("-3/9223372036854775808"),Err(pe(3,ParseKind::OutOfRange)));
    assert_eq!(Rational::parse("1/999999999999999999999999999999999999999999999"),Err(pe(2,ParseKind::OutOfRange)));
    assert_eq!(Rational::parse("9223372036854775808/9223372036854775808"),Err(pe(0,ParseKind::OutOfRange)));
}

// R10–R13
#[test]
fn h_canonicality_before_magnitude() {
    assert_eq!(Rational::parse("9223372036854775808/00"),Err(pe(21,ParseKind::NonCanonical)));
    assert_eq!(Rational::parse("000000000000000000000000000000000000000000000000000"),Err(pe(1,ParseKind::NonCanonical)));
    assert_eq!(Rational::parse("-01/9223372036854775808"),Err(pe(2,ParseKind::NonCanonical)));
}

// R13–R15
#[test]
fn h_zero_denominator_parse() {
    assert_eq!(Rational::parse("0/0"),Err(pe(2,ParseKind::ZeroDenominator)));
    assert_eq!(Rational::parse("-7/0"),Err(pe(3,ParseKind::ZeroDenominator)));
    assert_eq!(Rational::parse("9223372036854775808/0"),Err(pe(0,ParseKind::OutOfRange)));
}

// R15
#[test]
fn h_explicit_one_and_zero_fraction() {
    for (s,at) in [("3/1",1),("-3/1",2),("0/1",1),("0/11",1)] {
        assert_eq!(Rational::parse(s),Err(pe(at,ParseKind::NonCanonical)));
    }
}

// R12,R16
#[test]
fn h_unreduced_parse_and_minimum_gcd() {
    assert_eq!(Rational::parse("14/21"),Err(pe(2,ParseKind::NonCanonical)));
    assert_eq!(Rational::parse("-9223372036854775808/2"),Err(pe(20,ParseKind::NonCanonical)));
    assert_eq!(Rational::parse("-9223372036854775808/7"),Ok(q(i64::MIN,7)));
    assert_eq!(Rational::parse("-17/23"),Ok(q(-17,23)));
}

// R17–R20,R24
#[test]
fn h_shape_length_index_and_empty() {
    assert_eq!(Matrix::new(usize::MAX,2,vec![]),Err(MatrixError::ShapeOverflow));
    assert_eq!(Matrix::new(2,3,vec![q(1,1)]),Err(MatrixError::DataLength{expected:6,actual:1}));
    let a=fm(2,2,&[(2,3),(4,5),(-1,7),(9,2)]);
    assert_eq!((a.rows(),a.cols()),(2,2)); assert_eq!(a.data()[1],q(4,5));
    assert_eq!(a.get(1,0),Ok(q(-1,7)));
    assert_eq!(a.get(usize::MAX,4),Err(MatrixError::Index{row:usize::MAX,col:4}));
    for (r,c) in [(0,0),(0,usize::MAX),(usize::MAX,0)] {
        let z=mat(r,c,&[]); assert_eq!(z.get(0,0),Err(MatrixError::Index{row:0,col:0}));
        assert_eq!(z.transpose().transpose(),z);
    }
    assert_ne!(mat(0,2,&[]),mat(2,0,&[]));
}

// R17,R20–R21
#[test]
fn h_transpose_rectangular_fractional() {
    let a=fm(2,3,&[(1,2),(2,3),(3,4),(-1,2),(-2,3),(-3,4)]);
    let before=a.clone();
    assert_eq!(a.transpose(),fm(3,2,&[(1,2),(-1,2),(2,3),(-2,3),(3,4),(-3,4)]));
    assert_eq!(a.transpose().transpose(),before); assert_eq!(a,before);
}

// R19,R22,R24
#[test]
fn h_multiplication_shapes_empty_and_precedence() {
    assert_eq!(mat(usize::MAX,0,&[]).mul(&mat(1,2,&[0,0])),
        Err(MatrixError::DimensionMismatch{left:(usize::MAX,0),right:(1,2)}));
    assert_eq!(mat(usize::MAX,0,&[]).mul(&mat(0,2,&[])),Err(MatrixError::ShapeOverflow));
    assert_eq!(mat(3,0,&[]).mul(&mat(0,4,&[])),Ok(mat(3,4,&[0;12])));
    assert_eq!(mat(0,3,&[]).mul(&mat(3,0,&[])),Ok(mat(0,0,&[])));
    assert_eq!(mat(usize::MAX,0,&[]).mul(&mat(0,0,&[])),Ok(mat(usize::MAX,0,&[])));
}

// R6,R21–R23
#[test]
fn h_multiply_fractions_order_and_immutability() {
    let a=fm(2,3,&[(1,2),(2,3),(-1,4),(3,2),(1,3),(5,4)]);
    let b=fm(3,2,&[(2,1),(1,2),(3,1),(-1,1),(4,1),(2,1)]);
    let ac=a.clone(); let bc=b.clone();
    assert_eq!(a.mul(&b),Ok(fm(2,2,&[(2,1),(-11,12),(9,1),(35,12)])));
    assert_eq!(a,ac); assert_eq!(b,bc);
    let av: Vec<i64>=(0..48).map(|i| i%9-4).collect();
    let bv: Vec<i64>=(0..30).map(|i| i%7-3).collect();
    let mut expected=Vec::new();
    for r in 0..8 { for c in 0..5 {
        expected.push((0..6).map(|k|av[r*6+k]*bv[k*5+c]).sum());
    }}
    assert_eq!(mat(8,6,&av).mul(&mat(6,5,&bv)),Ok(mat(8,5,&expected)));
    let m=i64::MAX;
    assert_eq!(fm(1,1,&[(m,2)]).mul(&fm(1,1,&[(2,m)])),Ok(mat(1,1,&[1])));
}

// R6,R22–R23
#[test]
fn h_multiply_error_coordinates_and_temporary_overflow() {
    let m=i64::MAX;
    assert_eq!(mat(2,2,&[0,0,0,m]).mul(&mat(2,2,&[0,0,0,2])),Err(ae(1,1,1)));
    assert_eq!(mat(1,3,&[m,1,-1]).mul(&mat(3,1,&[1,1,1])),Err(ae(0,0,1)));
    assert_eq!(mat(1,2,&[m,m]).mul(&mat(2,2,&[0,2,2,0])),Err(ae(0,0,1)));
    assert_eq!(fm(1,1,&[(1,m)]).mul(&fm(1,1,&[(1,2)])),Err(ae(0,0,0)));
}
