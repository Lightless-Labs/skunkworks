#[test]
fn spec_r1_source_compatibility() {
    let value = Rational::new(3, 2).unwrap();
    assert_eq!(value.to_string(), "3/2");
}

#[test]
fn spec_r2_public_api() {
    let value = Rational::new(7, 3).unwrap();
    assert_eq!(value.numerator(), 7);
    assert_eq!(value.denominator(), 3);
    assert_eq!(value.add(&value), Ok(Rational::new(14, 3).unwrap()));
    assert_eq!(value.sub(&value), Ok(Rational::new(0, 1).unwrap()));
    assert_eq!(value.mul(&value), Ok(Rational::new(49, 9).unwrap()));
    assert_eq!(value.div(&value), Ok(Rational::new(1, 1).unwrap()));
    assert_eq!(value.neg(), Ok(Rational::new(-7, 3).unwrap()));
    assert_eq!(Rational::parse("7/3"), Ok(value));
}

#[test]
fn spec_r3_error_variants_and_parse_kind() {
    assert_eq!(
        Rational::parse(""),
        Err(RationalError::Parse {
            at: 0,
            kind: ParseKind::Empty
        })
    );
    assert_eq!(Rational::new(1, 0), Err(RationalError::ZeroDenominator));
    assert_eq!(
        Rational::new(i64::MIN, -1),
        Err(RationalError::Overflow)
    );
    assert_eq!(
        Rational::new(1, 1).div(&Rational::new(0, 1).unwrap()),
        Err(RationalError::DivideByZero)
    );
    assert_eq!(
        Rational::parse("01"),
        Err(RationalError::Parse {
            at: 1,
            kind: ParseKind::NonCanonical
        })
    );
    assert_eq!(
        Rational::parse("x"),
        Err(RationalError::Parse {
            at: 0,
            kind: ParseKind::InvalidByte
        })
    );
    assert_eq!(
        Rational::parse("9223372036854775808"),
        Err(RationalError::Parse {
            at: 0,
            kind: ParseKind::OutOfRange
        })
    );
    assert_eq!(
        Rational::parse("1/0"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::ZeroDenominator
        })
    );
}

#[test]
fn spec_r4_reduced_positive_denominator_and_zero() {
    let cases = [
        (Rational::new(12, 18).unwrap(), 2, 3),
        (Rational::new(-12, -18).unwrap(), 2, 3),
        (Rational::new(12, -18).unwrap(), -2, 3),
        (Rational::new(0, i64::MIN).unwrap(), 0, 1),
    ];
    for (value, numerator, denominator) in cases {
        assert_eq!(value.numerator(), numerator);
        assert_eq!(value.denominator(), denominator);
        assert!(value.denominator() > 0);
    }
    assert_eq!(Rational::new(2, 4).unwrap(), Rational::new(1, 2).unwrap());
}

#[test]
fn spec_r5_normalize_before_bounds_and_handle_minimum() {
    assert_eq!(
        Rational::new(i64::MIN, -1),
        Err(RationalError::Overflow)
    );
    assert_eq!(
        Rational::new(i64::MIN, i64::MIN),
        Ok(Rational::new(1, 1).unwrap())
    );
    assert_eq!(
        Rational::new(i64::MIN, 2),
        Ok(Rational::new(i64::MIN / 2, 1).unwrap())
    );
    assert_eq!(
        Rational::new(0, 0),
        Err(RationalError::ZeroDenominator)
    );
    assert_eq!(
        Rational::new(0, i64::MIN),
        Ok(Rational::new(0, 1).unwrap())
    );
    assert_eq!(
        Rational::new(1, i64::MIN),
        Err(RationalError::Overflow)
    );
}

#[test]
fn spec_r6_reduce_arithmetic_before_overflow_checks() {
    let large = Rational::new(i64::MAX, 2).unwrap();
    assert_eq!(
        large.add(&large),
        Ok(Rational::new(i64::MAX, 1).unwrap())
    );

    let a = Rational::new(i64::MAX, 2).unwrap();
    let b = Rational::new(2, i64::MAX).unwrap();
    assert_eq!(a.mul(&b), Ok(Rational::new(1, 1).unwrap()));
    assert_eq!(a.div(&a), Ok(Rational::new(1, 1).unwrap()));

    assert_eq!(
        Rational::new(i64::MAX, 1)
            .unwrap()
            .sub(&Rational::new(i64::MIN, 1).unwrap()),
        Err(RationalError::Overflow)
    );
    assert_eq!(
        Rational::new(i64::MIN, 1)
            .unwrap()
            .sub(&Rational::new(i64::MIN, 1).unwrap()),
        Ok(Rational::new(0, 1).unwrap())
    );
}

#[test]
fn spec_r7_division_precedence_negation_and_immutability() {
    let zero = Rational::new(0, 1).unwrap();
    assert_eq!(zero.div(&zero), Err(RationalError::DivideByZero));

    let minimum = Rational::new(i64::MIN, 1).unwrap();
    assert_eq!(minimum.neg(), Err(RationalError::Overflow));
    assert_eq!(minimum.numerator(), i64::MIN);
    assert_eq!(minimum.denominator(), 1);

    let value = Rational::new(5, 7).unwrap();
    let rhs = Rational::new(2, 3).unwrap();
    let _ = value.add(&rhs).unwrap();
    let _ = value.sub(&rhs).unwrap();
    let _ = value.mul(&rhs).unwrap();
    let _ = value.div(&rhs).unwrap();
    let _ = value.neg().unwrap();
    assert_eq!(value, Rational::new(5, 7).unwrap());
    assert_eq!(rhs, Rational::new(2, 3).unwrap());
}

#[test]
fn spec_r8_display_round_trip_and_decimal_format() {
    let cases = [
        (Rational::new(0, 9).unwrap(), "0"),
        (Rational::new(-12, 3).unwrap(), "-4"),
        (Rational::new(25, 7).unwrap(), "25/7"),
        (Rational::new(i64::MIN, 1).unwrap(), "-9223372036854775808"),
    ];
    for (value, expected) in cases {
        let rendered = value.to_string();
        assert_eq!(rendered, expected);
        assert_eq!(Rational::parse(&rendered), Ok(value));
    }
}

#[test]
fn spec_r9_validate_full_byte_grammar() {
    let cases = [
        ("", 0, ParseKind::Empty),
        ("+", 0, ParseKind::InvalidByte),
        (" 1", 0, ParseKind::InvalidByte),
        ("1 ", 1, ParseKind::InvalidByte),
        ("1/2/3", 3, ParseKind::InvalidByte),
        ("1/-2", 2, ParseKind::InvalidByte),
        ("--1", 1, ParseKind::InvalidByte),
        ("1/", 2, ParseKind::InvalidByte),
        ("-", 1, ParseKind::InvalidByte),
        ("1/2x", 3, ParseKind::InvalidByte),
        ("é", 0, ParseKind::InvalidByte),
        ("1é", 1, ParseKind::InvalidByte),
    ];
    for (text, at, kind) in cases {
        assert_eq!(
            Rational::parse(text),
            Err(RationalError::Parse { at, kind })
        );
    }
}

#[test]
fn spec_r10_syntax_errors_precede_canonicality_and_magnitude() {
    assert_eq!(
        Rational::parse("01x"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::InvalidByte
        })
    );
    assert_eq!(
        Rational::parse("999999999999999999999x"),
        Err(RationalError::Parse {
            at: 21,
            kind: ParseKind::InvalidByte
        })
    );
    assert_eq!(
        Rational::parse("1/00x"),
        Err(RationalError::Parse {
            at: 4,
            kind: ParseKind::InvalidByte
        })
    );
}

#[test]
fn spec_r11_canonical_zero_padding_checks_order() {
    let cases = [
        ("00", 1),
        ("-00", 2),
        ("-0", 0),
        ("1/00", 3),
    ];
    for (text, at) in cases {
        assert_eq!(
            Rational::parse(text),
            Err(RationalError::Parse {
                at,
                kind: ParseKind::NonCanonical
            })
        );
    }
    assert_eq!(
        Rational::parse("00/00"),
        Err(RationalError::Parse {
            at: 1,
            kind: ParseKind::NonCanonical
        })
    );
}

#[test]
fn spec_r12_numerator_magnitude_bounds() {
    assert_eq!(
        Rational::parse("9223372036854775807"),
        Ok(Rational::new(i64::MAX, 1).unwrap())
    );
    assert_eq!(
        Rational::parse("-9223372036854775808"),
        Ok(Rational::new(i64::MIN, 1).unwrap())
    );
    for text in [
        "9223372036854775808",
        "-9223372036854775809",
        "999999999999999999999999999999999999999999999999999",
    ] {
        assert_eq!(
            Rational::parse(text),
            Err(RationalError::Parse {
                at: 0,
                kind: ParseKind::OutOfRange
            })
        );
    }
}

#[test]
fn spec_r13_denominator_magnitude_is_checked_before_reduction() {
    assert_eq!(
        Rational::parse("1/9223372036854775807"),
        Ok(Rational::new(1, i64::MAX).unwrap())
    );
    assert_eq!(
        Rational::parse("9223372036854775808/9223372036854775808"),
        Err(RationalError::Parse {
            at: 0,
            kind: ParseKind::OutOfRange
        })
    );
    assert_eq!(
        Rational::parse("1/9223372036854775808"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::OutOfRange
        })
    );
    assert_eq!(
        Rational::parse("1/999999999999999999999999999999999999"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::OutOfRange
        })
    );
}

#[test]
fn spec_r14_zero_denominator_parse_error() {
    assert_eq!(
        Rational::parse("1/0"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::ZeroDenominator
        })
    );
    assert_eq!(
        Rational::parse("-1/0"),
        Err(RationalError::Parse {
            at: 3,
            kind: ParseKind::ZeroDenominator
        })
    );
}

#[test]
fn spec_r15_reject_explicit_one_denominator_and_zero_fraction() {
    assert_eq!(
        Rational::parse("1/1"),
        Err(RationalError::Parse {
            at: 1,
            kind: ParseKind::NonCanonical
        })
    );
    assert_eq!(
        Rational::parse("0/2"),
        Err(RationalError::Parse {
            at: 1,
            kind: ParseKind::NonCanonical
        })
    );
    assert_eq!(
        Rational::parse("0/0"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::ZeroDenominator
        })
    );
}

#[test]
fn spec_r16_reject_unreduced_fractions_and_accept_canonical_text() {
    assert_eq!(
        Rational::parse("2/4"),
        Err(RationalError::Parse {
            at: 1,
            kind: ParseKind::NonCanonical
        })
    );
    assert_eq!(
        Rational::parse("-6/9"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::NonCanonical
        })
    );
    assert_eq!(Rational::parse("0"), Ok(Rational::new(0, 1).unwrap()));
    assert_eq!(
        Rational::parse("-9223372036854775808"),
        Ok(Rational::new(i64::MIN, 1).unwrap())
    );
    assert_eq!(
        Rational::parse("-3/4"),
        Ok(Rational::new(-3, 4).unwrap())
    );
}
