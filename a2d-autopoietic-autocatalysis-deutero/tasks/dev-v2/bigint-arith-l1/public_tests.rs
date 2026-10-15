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
    assert_eq!(text(&a.sub(&b).unwrap(),0,0), "997");
    assert_eq!(a.cmp_value(&b), std::cmp::Ordering::Greater);
    assert_eq!(text(&a,0,0), "999");
}

// R4,R9,R13,R23
#[test]
fn public_limit_and_borrow() {
    let a=parse_limited("6_802",4).unwrap();
    assert_eq!(a.sub(&value("13")).unwrap(),value("6789"));
}
