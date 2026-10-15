fn value(text: &str) -> Big { Big::parse(text, 10000).unwrap() }
fn text(value: &Big, width: usize, group: usize) -> String { (value).format(width, group) }
fn parse_limited(text: &str, limit: usize) -> Result<Big, Error> { Big::parse(text, limit) }

// R1-R7,R14
#[test]
fn hidden_empty_and_lone_sign() {
    assert_eq!(parse_limited("",0), Err(Error::Empty));
    assert_eq!(parse_limited("+",0), Err(Error::SignOnly));
    let _copy = Error::Empty.clone(); let _debug = format!("{:?}", value("7"));
}

// R5,R8,R10,R14
#[test]
fn hidden_invalid_bytes_and_offsets() {
    for (s,i) in [(" 2",0),("12 ",2),("++1",1),("9.4",1),("+5é",2),("3\n",1),("8\0",1)] {
        assert_eq!(parse_limited(s,0), Err(Error::InvalidDigit { index:i }), "{s:?}");
    }
}

// R9-R11,R14
#[test]
fn hidden_invalid_beats_separator() {
    assert_eq!(parse_limited("_42?",0), Err(Error::InvalidDigit {index:3}));
    assert_eq!(parse_limited("+1__2!",1), Err(Error::InvalidDigit {index:5}));
    assert_eq!(parse_limited("00_?%",0), Err(Error::InvalidDigit {index:3}));
}

// R9,R11,R14
#[test]
fn hidden_separator_positions() {
    for (s,i) in [("_8",0),("9_",1),("+_9",1),("1__9",1),("__",0),("_",0),("00__1",2)] {
        assert_eq!(parse_limited(s,0),Err(Error::Separator {index:i}),"{s}");
    }
}

// R12-R14
#[test]
fn hidden_leading_zero_precedence() {
    for (s,i) in [("01",0),("+00",1),("0_2",0),("+0_0_0",1)] {
        assert_eq!(parse_limited(s,0), Err(Error::LeadingZero {index:i}));
    }
    assert_eq!(parse_limited("00_",0),Err(Error::Separator {index:2}));
}

// R8,R13-R14
#[test]
fn hidden_budget_byte_positions() {
    assert_eq!(parse_limited("+4_5_6",2),Err(Error::TooManyDigits {limit:2,index:5}));
    assert_eq!(parse_limited("7_89",1),Err(Error::TooManyDigits {limit:1,index:2}));
    assert_eq!(parse_limited("+0",0),Err(Error::TooManyDigits {limit:0,index:1}));
    assert_eq!(parse_limited("0",0),Err(Error::TooManyDigits {limit:0,index:0}));
}

// R4,R9,R13,R15
#[test]
fn hidden_exact_digit_limit() {
    assert_eq!(parse_limited("+8_7_6",3).unwrap(),value("876"));
    assert_eq!(parse_limited("6",1).unwrap(),value("6"));
    let s = "7".repeat(230); let v = parse_limited(&s,230).unwrap();
    assert_eq!(text(&v,0,0),s);
}

// R2,R16,R20,R24
#[test]
fn hidden_canonical_equality() {
    assert_eq!(value("+0"),value("0"));
    assert_eq!(value("+7_6_5"),value("765"));
    assert_eq!(text(&value("+0"),0,3),"0");
    assert_eq!(text(&value("+31"),0,0),"31");
}

// R17-R20
#[test]
fn hidden_padding_then_grouping() {
    assert_eq!(text(&value("42"),7,3),"0_000_042");
    assert_eq!(text(&value("0"),6,2),"00_00_00");
    assert_eq!(text(&value("9"),4,1),"0_0_0_9");
}

// R18-R20
#[test]
fn hidden_group_width_boundaries() {
    for (w,g,s) in [(0,0,"1234567"),(0,3,"1_234_567"),(0,7,"1234567"),(0,20,"1234567"),(0,1,"1_2_3_4_5_6_7"),(3,2,"1_23_45_67")] {
        assert_eq!(text(&value("1234567"),w,g),s);
    }
    assert_eq!(text(&value("0"),0,1),"0");
}

// R12,R18,R20
#[test]
fn hidden_padding_is_not_parse_canonical() {
    let rendered = text(&value("26"),4,0);
    assert_eq!(rendered,"0026");
    assert_eq!(parse_limited(&rendered,99),Err(Error::LeadingZero {index:0}));
}

// R16,R21
#[test]
fn hidden_value_ordering() {
    use std::cmp::Ordering::*;
    for (a,b,o) in [("0","0",Equal),("9","10",Less),("19","18",Greater),("12340","12341",Less),("7_654","7654",Equal)] {
        assert_eq!(value(a).cmp_value(&value(b)),o);
        assert_eq!(value(b).cmp_value(&value(a)),o.reverse());
    }
}

// R15,R22,R24
#[test]
fn hidden_long_carry_chain() {
    let a = value(&"9".repeat(310)); let b = value("1");
    let before = a.clone();
    assert_eq!(text(&a.add(&b),0,0), format!("1{}","0".repeat(310)));
    assert_eq!(a,before); assert_eq!(b,value("1"));
}

// R15,R23-R24
#[test]
fn hidden_long_borrow_chain() {
    let a = value(&format!("1{}","0".repeat(211)));
    let result = a.sub(&value("1"));
    assert_eq!(text(&result,0,0),"9".repeat(211));
    assert_eq!(text(&a,0,0),format!("1{}","0".repeat(211)));
}

// R16,R23-R24
#[test]
fn hidden_subtraction_equality() {
    let a = value("74_398");
    assert_eq!(a.sub(&a), value("0"));
    assert_eq!(text(&a.sub(&a),5,2),"0_00_00");
}

// R13,R22-R24
#[test]
fn hidden_arithmetic_has_no_parse_limit() {
    let a = parse_limited("9",1).unwrap();
    assert_eq!(text(&a.add(&a),0,0),"18");
    let delta = a.add(&a).sub(&a);
    assert_eq!(delta,a);
    assert_eq!(parse_limited("18",1),Err(Error::TooManyDigits {limit:1,index:1}));
}

// R5,R7-R8,R14
#[test]
fn hidden_unsigned_or_signed_minus() {
    assert_eq!(parse_limited("-",0),Err(Error::SignOnly));
    assert_eq!(parse_limited("-0_1",0),Err(Error::LeadingZero {index:1}));
    assert_eq!(parse_limited("-8_9",1),Err(Error::TooManyDigits {limit:1,index:3}));
    assert_eq!(parse_limited("+-1",3),Err(Error::InvalidDigit {index:1}));
    assert_eq!(text(&value("-1_2"),0,0),"-12");
}

// R15,R21-R24
#[test]
fn hidden_dense_decimal_oracle() {
    for i in 0u64..70 {
        let a = i*99991+7; let b = i*371+2;
        let x=value(&a.to_string()); let y=value(&b.to_string());
        assert_eq!(text(&x.add(&y),0,0),(a+b).to_string());
        assert_eq!(text(&x.sub(&y),0,0),(a-b).to_string());
        assert_eq!(x.cmp_value(&y),a.cmp(&b));
    }
}

// R16,R18,R22-R24
#[test]
fn hidden_zero_identities() {
    let x=value("902030405060708090"); let z=value("0");
    assert_eq!(x.add(&z),x); assert_eq!(z.add(&x),x);
    assert_eq!(x.sub(&z),x);
    assert_eq!(text(&z,3,0),"000");
}

// R2,R16,R21-R24
#[test]
fn hidden_operand_clones_unchanged() {
    let x=value("36_157"); let y=value("8_273");
    let saved=(x.clone(),y.clone());
    let sum=x.add(&y); let _=x.cmp_value(&y); let _=x.sub(&y);
    assert_eq!((x.clone(),y.clone()),saved);
    assert_eq!(text(&sum,0,2),"4_44_30");
}

// R5,R16,R25-R29
#[test]
fn hidden_signed_zero_normalization() {
    let z=value("-0");
    assert_eq!(z,value("0")); assert_eq!(z.signum(),0); assert_eq!(z.neg(),z);
    assert_eq!(text(&z,4,3),"0_000");
    let x=value("-9001");
    for v in [x.add(&x.neg()),x.sub(&x),z.mul(&x),x.mul(&z)] {
        assert_eq!(v,z); assert_eq!(v.signum(),0); assert_eq!(text(&v,0,0),"0");
    }
}

// R21,R26,R29
#[test]
fn hidden_signed_ordering() {
    use std::cmp::Ordering::*;
    for (a,b,o) in [("-9","-10",Greater),("-2","0",Less),("-100","-99",Less),("-0","+0",Equal),("0","2",Less)] {
        assert_eq!(value(a).cmp_value(&value(b)),o);
    }
    assert_eq!(value("-7").signum(),-1); assert_eq!(value("8").signum(),1);
}

// R22-R25,R28-R29
#[test]
fn hidden_sign_combinations_add_sub() {
    for a in [-1003i64,-12,0,7,1003] { for b in [-1003i64,-19,0,5,1003] {
        let x=value(&a.to_string()); let y=value(&b.to_string());
        assert_eq!(text(&x.add(&y),0,0),(a+b).to_string());
        assert_eq!(text(&x.sub(&y),0,0),(a-b).to_string());
        assert_eq!(text(&x.neg(),0,0),(-a).to_string());
    }}
}

// R17-R20,R25
#[test]
fn hidden_negative_padding() {
    assert_eq!(text(&value("-87"),6,2),"-00_00_87");
    assert_eq!(text(&value("-87"),0,1),"-8_7");
    assert_eq!(text(&value("-87").neg(),3,3),"087");
}

// R15,R24,R27-R29
#[test]
fn hidden_product_carries_and_signs() {
    let x=value(&"9".repeat(95)); let before=x.clone();
    let square = x.mul(&x);
    let expected = format!("{}8{}1","9".repeat(94),"0".repeat(94));
    assert_eq!(text(&square,0,0),expected);
    assert_eq!(x.neg().mul(&x.neg()),square);
    assert_eq!(x.neg().mul(&x),square.neg());
    assert_eq!(x,before);
}

// R26-R29
#[test]
fn hidden_product_small_oracle() {
    for a in [-9876i64,-1,0,1,54321] { for b in [-3001i64,0,31,998] {
        assert_eq!(text(&value(&a.to_string()).mul(&value(&b.to_string())),0,0),(a*b).to_string());
    }}
    assert_eq!(parse_limited("7",1).unwrap().mul(&value("8")),value("56"));
}
