fn value(text: &str) -> Big { Big::parse(text, 10000).unwrap() }
fn text(value: &Big, width: usize, group: usize) -> String { (value).format(width, group) }
fn parse_limited(text: &str, limit: usize) -> Result<Big, Error> { Big::parse(text, limit) }

// R1-R20
#[test]
fn public_parse_format() {
    let n = value("+12_345");
    assert_eq!(text(&n, 0, 0), "12345");
    assert_eq!(text(&n, 0, 3), "12_345");
    assert_eq!(n, n.clone());
}

// R21-R24
#[test]
fn public_arithmetic_compare() {
    let a = value("999"); let b = value("2");
    assert_eq!(text(&a.add(&b),0,0), "1001");
    assert_eq!(text(&a.sub(&b),0,0), "997");
    assert_eq!(a.cmp_value(&b), std::cmp::Ordering::Greater);
    assert_eq!(text(&a,0,0), "999");
}

// R16,R25-R29
#[test]
fn public_signed_product() {
    let a=value("-12"); let b=value("7");
    assert_eq!(text(&a.mul(&b),0,0),"-84");
    assert_eq!(a.signum(),-1); assert_eq!(a.neg(),value("12"));
}

// R30-R33
#[test]
fn public_division() {
    let (q,r)=value("29").div_rem(&value("6")).unwrap();
    assert_eq!(q,value("4")); assert_eq!(r,value("5"));
}
