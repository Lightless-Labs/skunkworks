#[test]
fn spec_r1_no_main_required() {
    let _ = Rational::new(1, 1).unwrap();
}

#[test]
fn spec_r2_public_rational_api() {
    let a = Rational::new(3, 11).unwrap();
    let b = Rational::new(2, 11).unwrap();
    assert_eq!((a.numerator(), a.denominator()), (3, 11));
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
    assert_eq!(
        Rational::new(1, 1).unwrap().div(&Rational::new(0, 1).unwrap()),
        Err(RationalError::DivideByZero)
    );
    assert_eq!(
        Rational::new(i64::MIN, 1).unwrap().neg(),
        Err(RationalError::Overflow)
    );
    assert_eq!(
        Rational::parse(""),
        Err(RationalError::Parse {
            at: 0,
            kind: ParseKind::Empty
        })
    );
}

#[test]
fn spec_r4_normalized_values_and_equality() {
    let reduced = Rational::new(6, -8).unwrap();
    assert_eq!((reduced.numerator(), reduced.denominator()), (-3, 4));
    assert_eq!(Rational::new(0, i64::MIN).unwrap(), Rational::new(0, 1).unwrap());
    assert_eq!(Rational::new(2, 3).unwrap(), Rational::new(4, 6).unwrap());
    assert!(reduced.denominator() > 0);
}

#[test]
fn spec_r5_constructor_normalization_and_minimum_inputs() {
    assert_eq!(Rational::new(0, 0), Err(RationalError::ZeroDenominator));
    assert_eq!(Rational::new(i64::MIN, -1), Err(RationalError::Overflow));
    assert_eq!(
        Rational::new(1, i64::MIN),
        Ok(Rational::new(-1, 1i64 << 63).unwrap_err_or_unavailable())
    );
}

#[test]
fn spec_r6_arithmetic_reduces_before_bounds_checking() {
    let max = Rational::new(i64::MAX, 1).unwrap();
    let minus_max = Rational::new(-i64::MAX, 1).unwrap();
    assert_eq!(max.add(&minus_max), Ok(Rational::new(0, 1).unwrap()));
    let a = Rational::new(i64::MAX, 2).unwrap();
    assert_eq!(
        a.mul(&Rational::new(2, i64::MAX).unwrap()),
        Ok(Rational::new(1, 1).unwrap())
    );
    let left = Rational::new(i64::MIN, 1).unwrap();
    assert_eq!(
        Rational::new(0, 1).unwrap().sub(&left),
        Err(RationalError::Overflow)
    );
}

#[test]
fn spec_r7_division_and_negation_precedence() {
    let zero = Rational::new(0, 1).unwrap();
    assert_eq!(zero.div(&zero), Err(RationalError::DivideByZero));
    assert_eq!(
        Rational::new(i64::MIN, 1).unwrap().neg(),
        Err(RationalError::Overflow)
    );
    let original = Rational::new(2, 3).unwrap();
    let rhs = Rational::new(4, 5).unwrap();
    let _ = original.add(&rhs).unwrap();
    assert_eq!((original.numerator(), original.denominator()), (2, 3));
    assert_eq!((rhs.numerator(), rhs.denominator()), (4, 5));
}

#[test]
fn spec_r8_display_round_trips_canonical_values() {
    let cases = [
        (Rational::new(0, 7).unwrap(), "0"),
        (Rational::new(-12, 1).unwrap(), "-12"),
        (Rational::new(3, 8).unwrap(), "3/8"),
        (Rational::new(i64::MIN, 1).unwrap(), "-9223372036854775808"),
    ];
    for (value, expected) in cases {
        assert_eq!(value.to_string(), expected);
        assert_eq!(Rational::parse(expected), Ok(value));
    }
}

#[test]
fn spec_r9_parse_validates_full_byte_grammar() {
    assert_eq!(
        Rational::parse("1/2x"),
        Err(RationalError::Parse {
            at: 3,
            kind: ParseKind::InvalidByte
        })
    );
    assert_eq!(
        Rational::parse("1/"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::InvalidByte
        })
    );
    assert_eq!(
        Rational::parse("1/٢"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::InvalidByte
        })
    );
    assert_eq!(
        Rational::parse("1/2/3"),
        Err(RationalError::Parse {
            at: 3,
            kind: ParseKind::InvalidByte
        })
    );
    assert_eq!(
        Rational::parse("+1"),
        Err(RationalError::Parse {
            at: 0,
            kind: ParseKind::InvalidByte
        })
    );
    assert_eq!(
        Rational::parse(""),
        Err(RationalError::Parse {
            at: 0,
            kind: ParseKind::Empty
        })
    );
}

#[test]
fn spec_r10_syntax_errors_precede_magnitude_and_canonicality() {
    assert_eq!(
        Rational::parse("01/999999999999999999999999x"),
        Err(RationalError::Parse {
            at: 27,
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
fn spec_r11_parse_leading_zero_and_negative_zero() {
    assert_eq!(
        Rational::parse("01"),
        Err(RationalError::Parse {
            at: 1,
            kind: ParseKind::NonCanonical
        })
    );
    assert_eq!(
        Rational::parse("-01"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::NonCanonical
        })
    );
    assert_eq!(
        Rational::parse("-0"),
        Err(RationalError::Parse {
            at: 0,
            kind: ParseKind::NonCanonical
        })
    );
    assert_eq!(
        Rational::parse("1/02"),
        Err(RationalError::Parse {
            at: 3,
            kind: ParseKind::NonCanonical
        })
    );
}

#[test]
fn spec_r12_parse_numerator_magnitude_bounds() {
    assert_eq!(
        Rational::parse("9223372036854775808"),
        Err(RationalError::Parse {
            at: 0,
            kind: ParseKind::OutOfRange
        })
    );
    assert_eq!(
        Rational::parse("-9223372036854775808"),
        Ok(Rational::new(i64::MIN, 1).unwrap())
    );
    assert_eq!(
        Rational::parse("-999999999999999999999999999999"),
        Err(RationalError::Parse {
            at: 0,
            kind: ParseKind::OutOfRange
        })
    );
}

#[test]
fn spec_r13_parse_denominator_magnitude_bounds() {
    assert_eq!(
        Rational::parse("1/9223372036854775808"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::OutOfRange
        })
    );
}

#[test]
fn spec_r14_parse_zero_denominator_error() {
    assert_eq!(
        Rational::parse("1/0"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::ZeroDenominator
        })
    );
}

#[test]
fn spec_r15_parse_integer_denominator_and_zero_fraction_canonicality() {
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
        Rational::parse("1/0"),
        Err(RationalError::Parse {
            at: 2,
            kind: ParseKind::ZeroDenominator
        })
    );
}

#[test]
fn spec_r16_parse_rejects_unreduced_fractions() {
    assert_eq!(
        Rational::parse("6/8"),
        Err(RationalError::Parse {
            at: 1,
            kind: ParseKind::NonCanonical
        })
    );
    assert_eq!(Rational::parse("3/8"), Ok(Rational::new(3, 8).unwrap()));
    assert_eq!(
        Rational::parse("-9223372036854775808"),
        Ok(Rational::new(i64::MIN, 1).unwrap())
    );
}

#[test]
fn spec_r17_matrix_public_shape_and_equality() {
    let x = Rational::new(1, 2).unwrap();
    let a = Matrix::new(1, 2, vec![x, x]).unwrap();
    assert_eq!((a.rows(), a.cols()), (1, 2));
    assert_eq!(a.data(), &[x, x]);
    assert_eq!(a.get(0, 1), Ok(x));
    assert_ne!(a, Matrix::new(2, 1, vec![x, x]).unwrap());
}

#[test]
fn spec_r18_matrix_error_variants_and_coordinates() {
    let a = Matrix::new(1, 1, vec![Rational::new(1, 1).unwrap()]).unwrap();
    assert_eq!(
        a.get(0, 1),
        Err(MatrixError::Index { row: 0, col: 1 })
    );
    assert_eq!(
        a.mul(&Matrix::new(2, 1, vec![Rational::new(0, 1).unwrap(); 2]).unwrap()),
        Err(MatrixError::DimensionMismatch {
            left: (1, 1),
            right: (2, 1)
        })
    );
    assert_eq!(
        Matrix::new(usize::MAX, 2, vec![]),
        Err(MatrixError::ShapeOverflow)
    );
}

#[test]
fn spec_r19_matrix_constructor_shape_validation_order() {
    assert_eq!(
        Matrix::new(usize::MAX, 2, vec![Rational::new(0, 1).unwrap()]),
        Err(MatrixError::ShapeOverflow)
    );
    assert_eq!(
        Matrix::new(2, 2, vec![]),
        Err(MatrixError::DataLength {
            expected: 4,
            actual: 0
        })
    );
    assert_eq!(Matrix::new(usize::MAX, 0, vec![]).unwrap().rows(), usize::MAX);
    assert_eq!(Matrix::new(0, usize::MAX, vec![]).unwrap().cols(), usize::MAX);
}

#[test]
fn spec_r20_matrix_get_bounds_and_row_major_access() {
    let a = Matrix::new(
        2,
        2,
        vec![
            Rational::new(1, 1).unwrap(),
            Rational::new(2, 1).unwrap(),
            Rational::new(3, 1).unwrap(),
            Rational::new(4, 1).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(a.get(1, 0), Ok(Rational::new(3, 1).unwrap()));
    assert_eq!(a.get(2, 0), Err(MatrixError::Index { row: 2, col: 0 }));
    let empty = Matrix::new(0, 7, vec![]).unwrap();
    assert_eq!(empty.get(0, 0), Err(MatrixError::Index { row: 0, col: 0 }));
}

#[test]
fn spec_r21_transpose_shapes_empty_shapes_and_immutability() {
    let a = Matrix::new(
        2,
        3,
        (1..=6).map(|n| Rational::new(n, 1).unwrap()).collect(),
    )
    .unwrap();
    assert_eq!(
        a.transpose(),
        Matrix::new(
            3,
            2,
            vec![
                Rational::new(1, 1).unwrap(),
                Rational::new(4, 1).unwrap(),
                Rational::new(2, 1).unwrap(),
                Rational::new(5, 1).unwrap(),
                Rational::new(3, 1).unwrap(),
                Rational::new(6, 1).unwrap(),
            ]
        )
        .unwrap()
    );
    assert_eq!(a.transpose().transpose(), a);
    assert_eq!(a.rows(), 2);
    assert_eq!(Matrix::new(0, 5, vec![]).unwrap().transpose().rows(), 5);
    assert_eq!(Matrix::new(0, 5, vec![]).unwrap().transpose().cols(), 0);
}

#[test]
fn spec_r22_matrix_multiplication_shape_checks_precede_overflow() {
    let left = Matrix::new(usize::MAX, 0, vec![]).unwrap();
    let mismatch = Matrix::new(1, usize::MAX, vec![]).unwrap();
    assert_eq!(
        left.mul(&mismatch),
        Err(MatrixError::DimensionMismatch {
            left: (usize::MAX, 0),
            right: (1, usize::MAX)
        })
    );
    let right = Matrix::new(0, 2, vec![]).unwrap();
    assert_eq!(
        left.mul(&right),
        Err(MatrixError::ShapeOverflow)
    );
    assert_eq!(
        Matrix::new(0, 3, vec![]).unwrap().mul(&Matrix::new(3, 0, vec![]).unwrap()),
        Ok(Matrix::new(0, 0, vec![]).unwrap())
    );
}

#[test]
fn spec_r23_matrix_multiplication_ordered_bounded_arithmetic() {
    let max = Rational::new(i64::MAX, 1).unwrap();
    let minus_max = Rational::new(-i64::MAX, 1).unwrap();
    let left = Matrix::new(1, 2, vec![max, max]).unwrap();
    let right = Matrix::new(2, 1, vec![Rational::new(2, 1).unwrap(), Rational::new(-2, 1).unwrap()]).unwrap();
    assert_eq!(
        left.mul(&right),
        Err(MatrixError::Arithmetic {
            row: 0,
            col: 0,
            step: 0,
            source: RationalError::Overflow
        })
    );

    let left = Matrix::new(1, 2, vec![max, minus_max]).unwrap();
    let right = Matrix::new(2, 1, vec![Rational::new(1, 1).unwrap(); 2]).unwrap();
    assert_eq!(left.mul(&right), Ok(Matrix::new(1, 1, vec![Rational::new(0, 1).unwrap()]).unwrap()));
}

#[test]
fn spec_r24_zero_length_and_empty_output_multiplication() {
    let a = Matrix::new(2, 0, vec![]).unwrap();
    let b = Matrix::new(0, 3, vec![]).unwrap();
    assert_eq!(
        a.mul(&b),
        Ok(Matrix::new(2, 3, vec![Rational::new(0, 1).unwrap(); 6]).unwrap())
    );
    let huge_empty = Matrix::new(0, usize::MAX, vec![]).unwrap();
    let other = Matrix::new(usize::MAX, 0, vec![]).unwrap();
    assert_eq!(huge_empty.mul(&other), Ok(Matrix::new(0, 0, vec![]).unwrap()));
}

#[test]
fn spec_r25_reduction_rank_and_matrix_algorithms_exist() {
    let a = Matrix::new(
        2,
        2,
        vec![
            Rational::new(1, 1).unwrap(),
            Rational::new(0, 1).unwrap(),
            Rational::new(0, 1).unwrap(),
            Rational::new(1, 1).unwrap(),
        ],
    )
    .unwrap();
    let reduction = a.rref().unwrap();
    assert_eq!(reduction.rank(), 2);
    assert_eq!(a.rank(), Ok(2));
    assert_eq!(a.inverse(), Ok(a));
}

#[test]
fn spec_r26_rref_scans_columns_and_skips_empty_pivots() {
    let a = Matrix::new(
        2,
        3,
        vec![
            Rational::new(0, 1).unwrap(),
            Rational::new(1, 1).unwrap(),
            Rational::new(2, 1).unwrap(),
            Rational::new(0, 1).unwrap(),
            Rational::new(0, 1).unwrap(),
            Rational::new(1, 1).unwrap(),
        ],
    )
    .unwrap();
    let reduction = a.rref().unwrap();
    assert_eq!(reduction.pivots, vec![1, 2]);
    assert_eq!(
        reduction.matrix,
        Matrix::new(
            2,
            3,
            vec![
                Rational::new(0, 1).unwrap(),
                Rational::new(1, 1).unwrap(),
                Rational::new(0, 1).unwrap(),
                Rational::new(0, 1).unwrap(),
                Rational::new(0, 1).unwrap(),
                Rational::new(1, 1).unwrap(),
            ]
        )
        .unwrap()
    );
}

#[test]
fn spec_r27_rref_pivot_lexicographic_order_and_swaps() {
    let a = Matrix::new(
        2,
        1,
        vec![Rational::new(1, 2).unwrap(), Rational::new(2, 3).unwrap()],
    )
    .unwrap();
    let reduction = a.rref().unwrap();
    assert_eq!(reduction.swaps, vec![(0, 1)]);
    assert_eq!(reduction.pivots, vec![0]);
    assert_eq!(reduction.matrix.data(), &[Rational::new(1, 1).unwrap(), Rational::new(0, 1).unwrap()]);

    let tie = Matrix::new(
        2,
        1,
        vec![Rational::new(1, 3).unwrap(), Rational::new(-1, 3).unwrap()],
    )
    .unwrap();
    assert!(tie.rref().unwrap().swaps.is_empty());
    assert_eq!(
        Matrix::new(0, 3, vec![]).unwrap().rref().unwrap().swaps,
        Vec::<(usize, usize)>::new()
    );
}

#[test]
fn spec_r28_rref_normalizes_entire_pivot_row_with_ordinal_step() {
    let a = Matrix::new(
        1,
        2,
        vec![Rational::new(2, 1).unwrap(), Rational::new(1, 1).unwrap()],
    )
    .unwrap();
    assert_eq!(
        a.rref().unwrap().matrix,
        Matrix::new(
            1,
            2,
            vec![Rational::new(1, 1).unwrap(), Rational::new(1, 2).unwrap()]
        )
        .unwrap()
    );
    let min = Rational::new(i64::MIN, 1).unwrap();
    let overflow = Matrix::new(1, 2, vec![Rational::new(1, 1).unwrap(), min]).unwrap();
    assert_eq!(
        overflow.rref(),
        Err(MatrixError::Arithmetic {
            row: 0,
            col: 1,
            step: 0,
            source: RationalError::Overflow
        })
    );
}

#[test]
fn spec_r29_rref_eliminates_above_and_below_in_row_order() {
    let a = Matrix::new(
        2,
        2,
        vec![
            Rational::new(1, 1).unwrap(),
            Rational::new(1, 1).unwrap(),
            Rational::new(1, 1).unwrap(),
            Rational::new(2, 1).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        a.rref().unwrap().matrix,
        Matrix::new(
            2,
            2,
            vec![
                Rational::new(1, 1).unwrap(),
                Rational::new(0, 1).unwrap(),
                Rational::new(0, 1).unwrap(),
                Rational::new(1, 1).unwrap(),
            ]
        )
        .unwrap()
    );
    let min = Rational::new(i64::MIN, 1).unwrap();
    let overflow = Matrix::new(
        2,
        2,
        vec![Rational::new(1, 1).unwrap(), min, Rational::new(0, 1).unwrap(), Rational::new(1, 1).unwrap()],
    )
    .unwrap();
    assert_eq!(
        overflow.rref(),
        Err(MatrixError::Arithmetic {
            row: 0,
            col: 1,
            step: 1,
            source: RationalError::Overflow
        })
    );
}

#[test]
fn spec_r30_reduction_result_empty_shapes_and_input_immutability() {
    let empty = Matrix::new(0, 4, vec![]).unwrap();
    let reduction = empty.rref().unwrap();
    assert_eq!(reduction.matrix, empty);
    assert!(reduction.pivots.is_empty());
    assert!(reduction.swaps.is_empty());

    let original = Matrix::new(
        1,
        1,
        vec![Rational::new(7, 1).unwrap()],
    )
    .unwrap();
    let _ = original.rref().unwrap();
    assert_eq!(original.data(), &[Rational::new(7, 1).unwrap()]);
}

#[test]
fn spec_r31_inverse_augmented_elimination_and_error_precedence() {
    let nonsquare = Matrix::new(1, 2, vec![Rational::new(0, 1).unwrap(); 2]).unwrap();
    assert_eq!(
        nonsquare.inverse(),
        Err(MatrixError::NotSquare { rows: 1, cols: 2 })
    );
    let square_empty = Matrix::new(0, 0, vec![]).unwrap();
    assert_eq!(square_empty.inverse(), Ok(square_empty));
    let min = Rational::new(i64::MIN, 1).unwrap();
    let a = Matrix::new(
        2,
        2,
        vec![
            Rational::new(1, 1).unwrap(),
            min,
            Rational::new(0, 1).unwrap(),
            Rational::new(1, 1).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        a.inverse(),
        Err(MatrixError::Arithmetic {
            row: 0,
            col: 3,
            step: 0,
            source: RationalError::Overflow
        })
    );
}

#[test]
fn spec_r32_inverse_singular_detection_after_elimination() {
    let singular = Matrix::new(
        2,
        2,
        vec![
            Rational::new(1, 1).unwrap(),
            Rational::new(2, 1).unwrap(),
            Rational::new(2, 1).unwrap(),
            Rational::new(4, 1).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        singular.inverse(),
        Err(MatrixError::Singular { rank: 1 })
    );
}

#[test]
fn spec_r33_solver_public_types_and_unique_solution() {
    let a = Matrix::new(
        1,
        1,
        vec![Rational::new(1, 1).unwrap()],
    )
    .unwrap();
    let b = Matrix::new(1, 1, vec![Rational::new(5, 1).unwrap()]).unwrap();
    assert_eq!(
        a.solve(&b),
        Ok(Solution::Unique {
            values: b,
            pivots: vec![0]
        })
    );
}

#[test]
fn spec_r34_solver_row_mismatch_and_augmented_shape_overflow() {
    let a = Matrix::new(1, 0, vec![]).unwrap();
    let b = Matrix::new(0, 1, vec![]).unwrap();
    assert_eq!(
        a.solve(&b),
        Err(SolveError::RowMismatch {
            coefficients: 1,
            rhs: 0
        })
    );

    let huge_empty = Matrix::new(0, usize::MAX, vec![]).unwrap();
    let empty_rhs = Matrix::new(0, 1, vec![]).unwrap();
    assert_eq!(huge_empty.solve(&empty_rhs), Err(SolveError::AugmentedShapeOverflow));
}

#[test]
fn spec_r35_solver_augmented_elimination_errors_precede_consistency() {
    let min = Rational::new(i64::MIN, 1).unwrap();
    let a = Matrix::new(
        2,
        2,
        vec![
            Rational::new(1, 1).unwrap(),
            min,
            Rational::new(0, 1).unwrap(),
            Rational::new(1, 1).unwrap(),
        ],
    )
    .unwrap();
    let rhs = Matrix::new(
        2,
        1,
        vec![Rational::new(0, 1).unwrap(), Rational::new(1, 1).unwrap()],
    )
    .unwrap();
    assert_eq!(
        a.solve(&rhs),
        Err(SolveError::Matrix(MatrixError::Arithmetic {
            row: 0,
            col: 1,
            step: 1,
            source: RationalError::Overflow
        }))
    );
}

#[test]
fn spec_r36_solver_contradiction_reports_first_row_and_rhs_column() {
    let a = Matrix::new(
        2,
        1,
        vec![Rational::new(1, 1).unwrap(), Rational::new(2, 1).unwrap()],
    )
    .unwrap();
    let rhs = Matrix::new(
        2,
        2,
        vec![
            Rational::new(3, 1).unwrap(),
            Rational::new(4, 1).unwrap(),
            Rational::new(7, 1).unwrap(),
            Rational::new(8, 1).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        a.solve(&rhs),
        Ok(Solution::None {
            row: 1,
            rhs_col: 0,
            witness: vec![
                Rational::new(0, 1).unwrap(),
                Rational::new(1, 1).unwrap(),
                Rational::new(1, 1).unwrap()
            ]
        })
    );
}

#[test]
fn spec_r37_solver_particular_and_shape_overflow() {
    let a = Matrix::new(
        1,
        2,
        vec![Rational::new(1, 1).unwrap(), Rational::new(0, 1).unwrap()],
    )
    .unwrap();
    let rhs = Matrix::new(1, 1, vec![Rational::new(9, 1).unwrap()]).unwrap();
    assert_eq!(
        a.solve(&rhs),
        Ok(Solution::Infinite {
            particular: Matrix::new(
                2,
                1,
                vec![Rational::new(9, 1).unwrap(), Rational::new(0, 1).unwrap()]
            )
            .unwrap(),
            free: vec![1],
            basis: Matrix::new(
                2,
                1,
                vec![Rational::new(0, 1).unwrap(), Rational::new(1, 1).unwrap()]
            )
            .unwrap(),
            pivots: vec![0]
        })
    );

    let empty = Matrix::new(0, usize::MAX, vec![]).unwrap();
    let rhs = Matrix::new(0, 0, vec![]).unwrap();
    assert_eq!(
        empty.solve(&rhs),
        Err(SolveError::Matrix(MatrixError::ShapeOverflow))
    );
}

#[test]
fn spec_r38_solver_unique_infinite_and_zero_rhs_columns() {
    let no_equations = Matrix::new(0, 2, vec![]).unwrap();
    let no_rhs = Matrix::new(0, 0, vec![]).unwrap();
    assert_eq!(
        no_equations.solve(&no_rhs),
        Ok(Solution::Infinite {
            particular: Matrix::new(2, 0, vec![]).unwrap(),
            free: vec![0, 1],
            basis: Matrix::new(
                2,
                2,
                vec![
                    Rational::new(1, 1).unwrap(),
                    Rational::new(0, 1).unwrap(),
                    Rational::new(0, 1).unwrap(),
                    Rational::new(1, 1).unwrap()
                ]
            )
            .unwrap(),
            pivots: vec![]
        })
    );
    let zero_vars = Matrix::new(2, 0, vec![]).unwrap();
    let zero_rhs = Matrix::new(2, 0, vec![]).unwrap();
    assert_eq!(
        zero_vars.solve(&zero_rhs),
        Ok(Solution::Unique {
            values: Matrix::new(0, 0, vec![]).unwrap(),
            pivots: vec![]
        })
    );
}

#[test]
fn spec_r39_solver_basis_matches_free_variable_parameterization() {
    let a = Matrix::new(
        1,
        3,
        vec![
            Rational::new(2, 1).unwrap(),
            Rational::new(3, 1).unwrap(),
            Rational::new(0, 1).unwrap(),
        ],
    )
    .unwrap();
    let rhs = Matrix::new(1, 1, vec![Rational::new(7, 1).unwrap()]).unwrap();
    let result = a.solve(&rhs).unwrap();
    assert_eq!(
        result,
        Solution::Infinite {
            particular: Matrix::new(
                3,
                1,
                vec![
                    Rational::new(7, 2).unwrap(),
                    Rational::new(0, 1).unwrap(),
                    Rational::new(0, 1).unwrap(),
                ]
            )
            .unwrap(),
            free: vec![1, 2],
            basis: Matrix::new(
                3,
                2,
                vec![
                    Rational::new(-3, 2).unwrap(),
                    Rational::new(0, 1).unwrap(),
                    Rational::new(1, 1).unwrap(),
                    Rational::new(0, 1).unwrap(),
                    Rational::new(0, 1).unwrap(),
                    Rational::new(1, 1).unwrap(),
                ]
            )
            .unwrap(),
            pivots: vec![0]
        }
    );
}

#[test]
fn spec_r40_solver_basis_negation_overflow_coordinates() {
    let a = Matrix::new(
        1,
        2,
        vec![
            Rational::new(1, 1).unwrap(),
            Rational::new(i64::MIN, 1).unwrap(),
        ],
    )
    .unwrap();
    let rhs = Matrix::new(1, 0, vec![]).unwrap();
    assert_eq!(
        a.solve(&rhs),
        Err(SolveError::Matrix(MatrixError::Arithmetic {
            row: 0,
            col: 0,
            step: 1,
            source: RationalError::Overflow
        }))
    );
}

#[test]
fn spec_r41_solver_zero_equations_variables_and_rhs_columns() {
    let no_equations = Matrix::new(0, 3, vec![]).unwrap();
    let rhs = Matrix::new(0, 2, vec![]).unwrap();
    match no_equations.solve(&rhs).unwrap() {
        Solution::Infinite {
            particular,
            free,
            basis,
            ..
        } => {
            assert_eq!(particular, Matrix::new(3, 2, vec![]).unwrap());
            assert_eq!(free, vec![0, 1, 2]);
            assert_eq!(basis.rows(), 3);
            assert_eq!(basis.cols(), 3);
        }
        other => panic!("unexpected solution: {other:?}"),
    }

    let no_vars = Matrix::new(1, 0, vec![]).unwrap();
    let contradictory = Matrix::new(1, 1, vec![Rational::new(1, 1).unwrap()]).unwrap();
    assert!(matches!(
        no_vars.solve(&contradictory),
        Ok(Solution::None { .. })
    ));

    let zero_rhs = Matrix::new(1, 0, vec![]).unwrap();
    assert!(!matches!(
        Matrix::new(1, 1, vec![Rational::new(0, 1).unwrap()])
            .solve(&zero_rhs)
            .unwrap(),
        Solution::None { .. }
    ));
}

#[test]
fn spec_r42_huge_empty_solver_shapes_check_overflow_before_outputs() {
    let a = Matrix::new(0, usize::MAX, vec![]).unwrap();
    let rhs = Matrix::new(0, 0, vec![]).unwrap();
    assert_eq!(
        a.solve(&rhs),
        Err(SolveError::Matrix(MatrixError::ShapeOverflow))
    );
}
