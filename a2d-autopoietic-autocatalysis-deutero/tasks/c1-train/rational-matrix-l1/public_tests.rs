fn q(n: i64, d: i64) -> Rational { Rational::new(n, d).unwrap() }
fn pe(at: usize, kind: ParseKind) -> RationalError { RationalError::Parse { at, kind } }
// R2–R5,R8
#[test]
fn public_normalize_and_format() {
    let a=Rational::new(6,-8).unwrap(); assert_eq!(a.numerator(),-3);
    assert_eq!(a.denominator(),4); assert_eq!(a.to_string(),"-3/4");
    assert_eq!(q(0,7).to_string(),"0");
}

// R6–R7
#[test]
fn public_arithmetic() {
    let a=q(2,5); let b=q(1,10);
    assert_eq!(a.add(&b),Ok(q(1,2))); assert_eq!(a.sub(&b),Ok(q(3,10)));
    assert_eq!(a.mul(&b),Ok(q(1,25))); assert_eq!(a.div(&b),Ok(q(4,1)));
    assert_eq!(a.neg(),Ok(q(-2,5)));
}

// R8–R16
#[test]
fn public_parse() {
    assert_eq!(Rational::parse("-5/9"),Ok(q(-5,9)));
    assert_eq!(Rational::parse("42"),Ok(q(42,1)));
    assert_eq!(Rational::parse("#"),Err(pe(0,ParseKind::InvalidByte)));
}

// R3,R5–R7
#[test]
fn public_basic_errors() {
    assert_eq!(Rational::new(1,0),Err(RationalError::ZeroDenominator));
    assert_eq!(q(1,2).div(&q(0,1)),Err(RationalError::DivideByZero));
    assert_eq!(q(i64::MAX,1).mul(&q(2,1)),Err(RationalError::Overflow));
}
