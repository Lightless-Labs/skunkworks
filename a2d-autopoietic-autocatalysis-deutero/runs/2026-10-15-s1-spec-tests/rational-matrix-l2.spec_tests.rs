#[test]
fn spec_r1_source_compiles_without_main() {
    let value = Rational::new(1, 1).unwrap();
    assert_eq!(value.numerator(), 1);
}

#[test]
fn spec_r2_public_api_and_display() {
    let a = Rational::new(3, 11).unwrap();
    let b = Rational::new(2, 11).unwrap();
    assert_eq!(a.numerator(), 3);
    assert_eq!(a.denominator(), 11);
    assert_eq!(a.add(&b), Ok(Rational::new(5, 11).unwrap()));
    assert_eq!(a.sub(&b), Ok(Rational::new(1, 11).unwrap()));
    assert_eq!(a.mul(&b), Ok(Rational::new(6, 121).unwrap()));
    assert_eq!(a.div(&b), Ok(Rational::new(3, 2).unwrap()));
    assert_eq!(a.neg(), Ok(Rational::new(-3, 11).unwrap()));
    assert_eq!(Rational::parse("3/11"), Ok(a));
    assert_eq!(a.to_string(), "3/11");
}

#[test]
fn spec_r3_error_types_and_variants() {
    assert_eq!(Rational::new(1, 0), Err(RationalError::ZeroDenominator));
    assert_eq!(Rational::new(1, i64::MIN), Err(RationalError::Overflow));
    assert_eq!(
        Rational::parse(""),
        Err(RationalError::Parse {
            at: 0,
            kind: ParseKind::Empty
        })
    );
}

#[test]
fn spec_r4_successful_values_are_reduced_with_positive_denominator() {
    assert_eq!(Rational::new(6, -8), Ok(Rational::new(-3, 4).unwrap()));
    assert_eq!(Rational::new(0, i64::MIN), Ok(Rational::new(0, 1).unwrap()));
    assert_eq!(Rational::new(2, 3), Rational::new(4, 6));
}

#[test]
fn spec_r5_new_normalizes_before_bounds_and_handles_minimum_inputs() {
    assert_eq!(Rational::new(0, 0), Err(RationalError::ZeroDenominator));
    assert_eq!(Rational::new(i64::MIN, -1), Err(RationalError::Overflow));
    assert_eq!(
        Rational::new(i64::MIN, i64::MIN),
        Ok(Rational::new(1, 1).unwrap())
    );
    assert_eq!(Rational::new(i64::MIN, -2), Ok(Rational::new(1i64 << 62, 1).unwrap()));
    assert_eq!(Rational::new(1, i64::MIN), Err(RationalError::Overflow));
}

#[test]
fn spec_r6_arithmetic_reduces_before_reporting_overflow() {
    let large = Rational::new(i64::MAX, 2).unwrap();
    assert_eq!(
        large.mul(&Rational::new(2, i64::MAX).unwrap()),
        Ok(Rational::new(1, 1).unwrap())
    );
    assert_eq!(
        Rational::new(i64::MAX, 2)
            .add(&Rational::new(i64::MAX, 2).unwrap()),
        Ok(Rational::new(i64::MAX, 1).unwrap())
    );
    assert_eq!(
        Rational::new(i64::MIN, 1).unwrap().sub(&Rational::new(i64::MIN, 1).unwrap()),
        Ok(Rational::new(0, 1).unwrap())
    );
    assert_eq!(
        Rational::new(i64::MAX, 1).unwrap().add(&Rational::new(1, 1).unwrap()),
        Err(RationalError::Overflow)
    );
}

#[test]
fn spec_r7_division_precedence_negation_and_operand_immutability() {
    let zero = Rational::new(0, 1).unwrap();
    assert_eq!(zero.div(&zero), Err(RationalError::DivideByZero));
    assert_eq!(
        Rational::new(i64::MIN, 1).unwrap().neg(),
        Err(RationalError::Overflow)
    );
    let a = Rational::new(3, 5).unwrap();
    let b = Rational::new(2, 7).unwrap();
    let _ = a.add(&b);
    let _ = a.neg();
    assert_eq!(a, Rational::new(3, 5).unwrap());
    assert_eq!(b, Rational::new(2, 7).unwrap());
}

#[test]
fn spec_r8_display_is_canonical_and_round_trips() {
    for (value, display) in [
        (Rational::new(0, 1).unwrap(), "0"),
        (Rational::new(-7, 1).unwrap(), "-7"),
        (Rational::new(5, 8).unwrap(), "5/8"),
    ] {
        assert_eq!(value.to_string(), display);
        assert_eq!(Rational::parse(display), Ok(value));
    }
}

#[test]
fn spec_r9_parse_validates_full_byte_grammar_and_offsets() {
    for (text, at) in [
        ("1x", 1),
        ("1/2/3", 3),
        ("1/+2", 2),
        (" 1", 0),
        ("1 ", 1),
        ("1é", 1),
        ("-", 1),
        ("1/", 2),
        ("1/2-", 3),
    ] {
        assert_eq!(
            Rational::parse(text),
            Err(RationalError::Parse {
                at,
                kind: if text.is_empty() {
                    ParseKind::Empty
                } else {
                    ParseKind::InvalidByte
                }
            })
        );
    }
    assert_eq!(
        Rational::parse(""),
        Err(RationalError::Parse {
            at: 0,
            kind: ParseKind::Empty
        })
    );
}

#[test]
fn spec_r10_syntax_errors_precede_canonicality_and_magnitude() {
    assert_eq!(
        Rational::parse("00x"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::InvalidByte
        })
    );
    assert_eq!(
        Rational::parse("999999999999999999999999x"),
        Err(RationalError::Parse {
            at: 24,
            kind: ParseKind::InvalidByte
        })
    );
}

#[test]
fn spec_r11_parse_rejects_leading_zeroes_and_negative_zero_in_order() {
    for (text, at) in [("00", 1), ("-00", 2), ("-0", 0), ("1/00", 3)] {
        assert_eq!(
            Rational::parse(text),
            Err(RationalError::Parse {
                at,
                kind: ParseKind::NonCanonical
            })
        );
    }
    assert_eq!(
        Rational::parse("-00"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::NonCanonical
        })
    );
}

#[test]
fn spec_r12_parse_checks_signed_numerator_magnitude() {
    assert_eq!(Rational::parse("9223372036854775807"), Ok(Rational::new(i64::MAX, 1).unwrap()));
    assert_eq!(Rational::parse("-9223372036854775808"), Ok(Rational::new(i64::MIN, 1).unwrap()));
    for text in ["9223372036854775808", "-9223372036854775809", "999999999999999999999999999999999999"] {
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
fn spec_r13_parse_checks_denominator_magnitude_before_reduction() {
    assert_eq!(
        Rational::parse("1/9223372036854775808"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::OutOfRange
        })
    );
}

#[test]
fn spec_r14_parse_reports_zero_denominator_as_parse_error() {
    assert_eq!(
        Rational::parse("1/0"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::ZeroDenominator
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
fn spec_r15_parse_rejects_unit_denominator_and_explicit_zero_fraction() {
    for (text, at) in [("1/1", 1), ("0/2", 1), ("-0/2", 0)] {
        assert_eq!(
            Rational::parse(text),
            Err(RationalError::Parse {
                at,
                kind: ParseKind::NonCanonical
            })
        );
    }
}

#[test]
fn spec_r16_parse_rejects_reducible_fractions_but_accepts_canonical_forms() {
    assert_eq!(
        Rational::parse("2/4"),
        Err(RationalError::Parse {
            at: 1,
            kind: ParseKind::NonCanonical
        })
    );
    assert_eq!(Rational::parse("0"), Ok(Rational::new(0, 1).unwrap()));
    assert_eq!(Rational::parse("-9223372036854775808"), Ok(Rational::new(i64::MIN, 1).unwrap()));
    assert_eq!(Rational::parse("-3/4"), Ok(Rational::new(-3, 4).unwrap()));
}

#[test]
fn spec_r17_matrix_shape_data_access_and_equality() {
    let a = Matrix::new(2, 1, vec![Rational::new(1, 1).unwrap(), Rational::new(2, 1).unwrap()]).unwrap();
    assert_eq!((a.rows(), a.cols()), (2, 1));
    assert_eq!(a.data(), &[Rational::new(1, 1).unwrap(), Rational::new(2, 1).unwrap()]);
    assert_ne!(a, Matrix::new(1, 2, vec![Rational::new(1, 1).unwrap(), Rational::new(2, 1).unwrap()]).unwrap());
}

#[test]
fn spec_r18_matrix_error_variants_are_exposed() {
    assert_eq!(
        Matrix::new(1, 1, vec![]),
        Err(MatrixError::DataLength {
            expected: 1,
            actual: 0
        })
    );
    assert_eq!(
        Matrix::new(usize::MAX, 2, vec![]),
        Err(MatrixError::ShapeOverflow)
    );
}

#[test]
fn spec_r19_matrix_new_checks_shape_overflow_before_length_and_allows_empty_shapes() {
    assert_eq!(
        Matrix::new(usize::MAX, 2, vec![]),
        Err(MatrixError::ShapeOverflow)
    );
    assert_eq!(
        Matrix::new(usize::MAX, 0, vec![]).map(|m| (m.rows(), m.cols())),
        Ok((usize::MAX, 0))
    );
    assert_eq!(
        Matrix::new(0, usize::MAX, vec![]).map(|m| (m.rows(), m.cols())),
        Ok((0, usize::MAX))
    );
    assert_eq!(
        Matrix::new(2, 3, vec![]),
        Err(MatrixError::DataLength {
            expected: 6,
            actual: 0
        })
    );
}

#[test]
fn spec_r20_get_bounds_and_reports_original_coordinates() {
    let m = Matrix::new(1, 2, vec![Rational::new(4, 1).unwrap(), Rational::new(9, 1).unwrap()]).unwrap();
    assert_eq!(m.get(0, 1), Ok(Rational::new(9, 1).unwrap()));
    assert_eq!(m.get(1, 0), Err(MatrixError::Index { row: 1, col: 0 }));
    assert_eq!(m.get(0, 2), Err(MatrixError::Index { row: 0, col: 2 }));
    assert_eq!(
        Matrix::new(usize::MAX, 0, vec![]).unwrap().get(0, 0),
        Err(MatrixError::Index { row: 0, col: 0 })
    );
}

#[test]
fn spec_r21_transpose_preserves_empty_shapes_and_is_involutive() {
    let a = Matrix::new(
        2,
        3,
        (1..=6).map(|n| Rational::new(n, 1).unwrap()).collect(),
    )
    .unwrap();
    let t = a.transpose();
    assert_eq!((t.rows(), t.cols()), (3, 2));
    assert_eq!(
        t.data(),
        &[
            Rational::new(1, 1).unwrap(),
            Rational::new(4, 1).unwrap(),
            Rational::new(2, 1).unwrap(),
            Rational::new(5, 1).unwrap(),
            Rational::new(3, 1).unwrap(),
            Rational::new(6, 1).unwrap()
        ]
    );
    assert_eq!(t.transpose(), a);
    assert_eq!(
        Matrix::new(0, usize::MAX, vec![]).unwrap().transpose().rows(),
        usize::MAX
    );
    assert_eq!(
        Matrix::new(0, usize::MAX, vec![]).unwrap().transpose().cols(),
        0
    );
}

#[test]
fn spec_r22_matrix_mul_checks_dimensions_before_output_shape() {
    let left = Matrix::new(usize::MAX, 0, vec![]).unwrap();
    let right = Matrix::new(1, usize::MAX, vec![]).unwrap();
    assert_eq!(
        left.mul(&right),
        Err(MatrixError::DimensionMismatch {
            left: (usize::MAX, 0),
            right: (1, usize::MAX)
        })
    );
    assert_eq!(
        Matrix::new(0, 0, vec![]).unwrap()
            .mul(&Matrix::new(0, usize::MAX, vec![]).unwrap())
            .map(|m| (m.rows(), m.cols())),
        Ok((0, usize::MAX))
    );
}

#[test]
fn spec_r23_matrix_mul_reports_first_scalar_error_in_required_order() {
    let max = Rational::new(i64::MAX, 1).unwrap();
    let one = Rational::new(1, 1).unwrap();
    let minus_one = Rational::new(-1, 1).unwrap();
    let left = Matrix::new(1, 3, vec![max, max, minus_one]).unwrap();
    let right = Matrix::new(3, 1, vec![one, one, max]).unwrap();
    assert_eq!(
        left.mul(&right),
        Err(MatrixError::Arithmetic {
            row: 0,
            col: 0,
            step: 1,
            source: RationalError::Overflow
        })
    );

    let product_overflow_left = Matrix::new(1, 1, vec![max]).unwrap();
    let product_overflow_right = Matrix::new(1, 1, vec![Rational::new(2, 1).unwrap()]).unwrap();
    assert_eq!(
        product_overflow_left.mul(&product_overflow_right),
        Err(MatrixError::Arithmetic {
            row: 0,
            col: 0,
            step: 0,
            source: RationalError::Overflow
        })
    );
}

#[test]
fn spec_r24_matrix_mul_handles_zero_inner_and_empty_output_dimensions() {
    let product = Matrix::new(1, 0, vec![])
        .unwrap()
        .mul(&Matrix::new(0, 2, vec![]).unwrap())
        .unwrap();
    assert_eq!((product.rows(), product.cols()), (1, 2));
    assert_eq!(
        product.data(),
        &[Rational::new(0, 1).unwrap(), Rational::new(0, 1).unwrap()]
    );

    let empty = Matrix::new(0, 0, vec![])
        .unwrap()
        .mul(&Matrix::new(0, usize::MAX, vec![]).unwrap())
        .unwrap();
    assert_eq!((empty.rows(), empty.cols()), (0, usize::MAX));
}
