#[test]
fn spec_r1_source_compiles_as_standalone_rust_2024() {
    assert_eq!(MAX_DIM, 4096);
}

#[test]
fn spec_r2_traits_and_equality() {
    let a = Sparse::from_coo(1, 1, &[Entry { row: 0, col: 0, value: 2.0 }]).unwrap();
    let b = a.clone();
    assert_eq!(a, b);
    assert_eq!(Axis::Row, Axis::Row);

    let x = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![2.0]).unwrap();
    let y = x.clone();
    assert_eq!(x, y);
    assert_eq!(TextError::Header, TextError::Header);
}

#[test]
fn spec_r3_dimensions_checked_in_row_then_column_order() {
    assert_eq!(
        Sparse::from_coo(MAX_DIM + 1, MAX_DIM + 1, &[]),
        Err(Error::Shape { axis: Axis::Row, size: MAX_DIM + 1 })
    );
    assert_eq!(
        Sparse::from_coo(MAX_DIM, MAX_DIM + 1, &[]),
        Err(Error::Shape { axis: Axis::Column, size: MAX_DIM + 1 })
    );
    assert_eq!(Sparse::from_coo(MAX_DIM, MAX_DIM, &[]).unwrap().shape(), (MAX_DIM, MAX_DIM));
    assert_eq!(
        Csr::from_parts(MAX_DIM + 1, 0, vec![], vec![], vec![]),
        Err(Error::Shape { axis: Axis::Row, size: MAX_DIM + 1 })
    );
}

#[test]
fn spec_r4_zero_shapes_and_empty_coo() {
    for shape in [(0, 0), (0, 3), (4, 0)] {
        assert_eq!(Sparse::from_coo(shape.0, shape.1, &[]).unwrap().shape(), shape);
    }
    assert_eq!(
        Sparse::from_coo(0, 2, &[Entry { row: 0, col: 0, value: 1.0 }]),
        Err(Error::Bounds { entry: 0, axis: Axis::Row, index: 0, limit: 0 })
    );
}

#[test]
fn spec_r5_coo_bounds_follow_entry_and_axis_order() {
    let entries = [
        Entry { row: 0, col: 3, value: f64::NAN },
        Entry { row: 2, col: 0, value: 1.0 },
    ];
    assert_eq!(
        Sparse::from_coo(2, 3, &entries),
        Err(Error::Bounds { entry: 0, axis: Axis::Column, index: 3, limit: 3 })
    );
    assert_eq!(
        Sparse::from_coo(2, 3, &entries[1..]),
        Err(Error::Bounds { entry: 0, axis: Axis::Row, index: 2, limit: 2 })
    );
}

#[test]
fn spec_r6_nonfinite_values_and_bounds_priority() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            Sparse::from_coo(1, 1, &[Entry { row: 0, col: 0, value }]),
            Err(Error::NonFinite { entry: 0 })
        );
    }
    assert!(Sparse::from_coo(1, 1, &[Entry { row: 1, col: 0, value: f64::NAN }]).is_err());
    assert!(Sparse::from_coo(1, 1, &[Entry { row: 0, col: 1, value: f64::INFINITY }]).is_err());
    assert!(Sparse::from_coo(1, 1, &[Entry { row: 0, col: 0, value: -0.0 }]).is_ok());
    assert!(Sparse::from_coo(1, 1, &[Entry { row: 0, col: 0, value: f64::from_bits(1) }]).is_ok());
}

#[test]
fn spec_r7_validation_completes_before_duplicate_summation() {
    let entries = [
        Entry { row: 0, col: 0, value: f64::MAX },
        Entry { row: 0, col: 0, value: f64::MAX },
        Entry { row: 1, col: 0, value: f64::NAN },
    ];
    assert_eq!(Sparse::from_coo(2, 1, &entries), Err(Error::NonFinite { entry: 2 }));
}

#[test]
fn spec_r8_duplicate_addition_uses_original_occurrence_order() {
    let entries = [
        Entry { row: 0, col: 0, value: 1.0e16 },
        Entry { row: 0, col: 0, value: 1.0 },
        Entry { row: 0, col: 0, value: -1.0e16 },
    ];
    let result = Sparse::from_coo(1, 1, &entries).unwrap();
    assert!(result.entries().is_empty());
}

#[test]
fn spec_r9_sum_overflow_reports_added_entry_immediately() {
    let entries = [
        Entry { row: 0, col: 0, value: f64::MAX },
        Entry { row: 0, col: 0, value: f64::MAX },
        Entry { row: 0, col: 0, value: -f64::MAX },
    ];
    assert_eq!(Sparse::from_coo(1, 1, &entries), Err(Error::SumOverflow { entry: 1 }));
}

#[test]
fn spec_r10_cancelled_coordinates_can_receive_later_contributions() {
    let entries = [
        Entry { row: 0, col: 0, value: 3.0 },
        Entry { row: 0, col: 0, value: -3.0 },
        Entry { row: 0, col: 0, value: 5.0 },
    ];
    assert_eq!(
        Sparse::from_coo(1, 1, &entries).unwrap().entries(),
        &[Entry { row: 0, col: 0, value: 5.0 }]
    );
    assert!(Sparse::from_coo(1, 1, &[Entry { row: 0, col: 0, value: -0.0 }])
        .unwrap()
        .entries()
        .is_empty());
}

#[test]
fn spec_r11_keep_smallest_subnormal_sum() {
    let tiny = f64::from_bits(1);
    let s = Sparse::from_coo(1, 1, &[Entry { row: 0, col: 0, value: tiny }]).unwrap();
    assert_eq!(s.entries()[0].value.to_bits(), tiny.to_bits());
}

#[test]
fn spec_r12_coo_entries_are_canonical_row_major() {
    let s = Sparse::from_coo(
        3,
        4,
        &[
            Entry { row: 2, col: 0, value: 1.0 },
            Entry { row: 0, col: 3, value: 2.0 },
            Entry { row: 0, col: 1, value: 3.0 },
            Entry { row: 1, col: 0, value: 4.0 },
        ],
    )
    .unwrap();
    assert_eq!(
        s.entries(),
        &[
            Entry { row: 0, col: 1, value: 3.0 },
            Entry { row: 0, col: 3, value: 2.0 },
            Entry { row: 1, col: 0, value: 4.0 },
            Entry { row: 2, col: 0, value: 1.0 },
        ]
    );
}

#[test]
fn spec_r13_shape_and_entries_preserve_empty_shape() {
    let s = Sparse::from_coo(0, MAX_DIM, &[]).unwrap();
    assert_eq!(s.shape(), (0, MAX_DIM));
    assert!(s.entries().is_empty());
}

#[test]
fn spec_r14_coo_input_and_clones_are_unchanged() {
    let input = [Entry { row: 0, col: 0, value: 2.0 }];
    let before = input;
    let s = Sparse::from_coo(1, 1, &input).unwrap();
    let clone = s.clone();
    let _ = s.shape();
    let _ = s.entries();
    assert_eq!(input[0].value.to_bits(), before[0].value.to_bits());
    assert_eq!(clone, s);
}

#[test]
fn spec_r15_malformed_parts_return_errors_without_panicking() {
    assert_eq!(
        Csr::from_parts(1, 1, vec![0], vec![], vec![]),
        Err(Error::RowPtrLength { expected: 2, actual: 1 })
    );
}

#[test]
fn spec_r16_error_indices_are_original_zero_based_indices() {
    let entries = [
        Entry { row: 0, col: 0, value: 1.0 },
        Entry { row: 0, col: 0, value: f64::MAX },
        Entry { row: 0, col: 0, value: f64::MAX },
    ];
    assert_eq!(Sparse::from_coo(1, 1, &entries), Err(Error::SumOverflow { entry: 2 }));
}

#[test]
fn spec_r17_successful_numeric_results_are_finite() {
    let a = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![2.0]).unwrap();
    assert_eq!(a.mul_vector(&[3.0]).unwrap(), vec![6.0]);
}

#[test]
fn spec_r18_csr_error_precedence_follows_validation_passes() {
    assert_eq!(
        Csr::from_parts(1, 1, vec![1, 0], vec![9], vec![f64::NAN]),
        Err(Error::RowPtrStart { found: 1 })
    );
}

#[test]
fn spec_r19_csr_row_pointers_count_entries_through_each_row() {
    let s = Sparse::from_coo(
        4,
        3,
        &[
            Entry { row: 0, col: 1, value: 1.0 },
            Entry { row: 3, col: 2, value: 2.0 },
        ],
    )
    .unwrap()
    .to_csr();
    assert_eq!(s.row_ptr(), &[0, 1, 1, 1, 2]);
}

#[test]
fn spec_r20_csr_accessors_and_conversion_preserve_canonical_contents() {
    let s = Sparse::from_coo(2, 3, &[Entry { row: 1, col: 2, value: 4.0 }]).unwrap();
    let csr = s.to_csr();
    assert_eq!(csr.shape(), (2, 3));
    assert_eq!(csr.to_coo(), s);
    assert_eq!(csr.col_indices(), &[2]);
    assert_eq!(csr.values(), &[4.0]);
}

#[test]
fn spec_r21_from_parts_checks_lengths_in_order() {
    assert_eq!(
        Csr::from_parts(2, 1, vec![], vec![0], vec![]),
        Err(Error::RowPtrLength { expected: 3, actual: 0 })
    );
    assert_eq!(
        Csr::from_parts(0, 1, vec![0], vec![0], vec![]),
        Err(Error::ColumnValueLength { columns: 1, values: 0 })
    );
}

#[test]
fn spec_r22_from_parts_checks_endpoints_before_interior_order() {
    assert_eq!(
        Csr::from_parts(2, 2, vec![0, 3, 1], vec![], vec![]),
        Err(Error::RowPtrEnd { found: 1, expected: 0 })
    );
    assert_eq!(
        Csr::from_parts(0, 2, vec![1], vec![0], vec![1.0]),
        Err(Error::RowPtrStart { found: 1 })
    );
}

#[test]
fn spec_r23_interior_pointer_order_checked_before_slots() {
    assert_eq!(
        Csr::from_parts(2, 2, vec![0, 2, 1], vec![usize::MAX], vec![1.0]),
        Err(Error::RowPtrEnd { found: 1, expected: 1 })
    );
    assert_eq!(
        Csr::from_parts(3, 2, vec![0, 2, 1, 1], vec![usize::MAX], vec![1.0]),
        Err(Error::RowPtrOrder { row: 1 })
    );
}

#[test]
fn spec_r24_slot_validation_order_and_row_local_column_order() {
    assert_eq!(
        Csr::from_parts(1, 2, vec![0, 1], vec![2], vec![f64::NAN]),
        Err(Error::ColumnBounds { slot: 0, column: 2, limit: 2 })
    );
    assert_eq!(
        Csr::from_parts(1, 3, vec![0, 2], vec![1, 1], vec![f64::NAN, 0.0]),
        Err(Error::ColumnOrder { slot: 1 })
    );
    assert_eq!(
        Csr::from_parts(2, 2, vec![0, 1, 2], vec![1, 0], vec![1.0, 2.0]).unwrap().values(),
        &[1.0, 2.0]
    );
    assert_eq!(
        Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![f64::INFINITY]),
        Err(Error::NonFiniteValue { slot: 0 })
    );
    assert_eq!(
        Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![-0.0]),
        Err(Error::ExplicitZero { slot: 0 })
    );
}

#[test]
fn spec_r25_vector_length_then_all_input_finiteness() {
    let a = Csr::from_parts(0, 2, vec![0], vec![], vec![]).unwrap();
    assert_eq!(a.mul_vector(&[f64::NAN]), Err(Error::VectorLength { expected: 2, actual: 1 }));
    assert_eq!(a.mul_vector(&[1.0, f64::INFINITY]), Err(Error::NonFiniteVector { index: 1 }));
}

#[test]
fn spec_r26_product_overflow_precedes_accumulation() {
    let a = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![f64::MAX]).unwrap();
    assert_eq!(a.mul_vector(&[2.0]), Err(Error::ProductOverflow { row: 0, column: 0 }));
}

#[test]
fn spec_r27_accumulation_overflow_and_zero-column_arithmetic() {
    let a = Csr::from_parts(1, 2, vec![0, 2], vec![0, 1], vec![f64::MAX, f64::MAX]).unwrap();
    assert_eq!(
        a.mul_vector(&[1.0, 1.0]),
        Err(Error::AccumulationOverflow { row: 0, column: 1 })
    );
    let b = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![f64::MAX]).unwrap();
    assert_eq!(b.mul_vector(&[0.0]).unwrap(), vec![0.0]);
}

#[test]
fn spec_r28_vector_result_has_all_rows_and_normalizes_zero() {
    let a = Csr::from_parts(3, 1, vec![0, 1, 1, 2], vec![0, 0], vec![-1.0, 1.0]).unwrap();
    let result = a.mul_vector(&[0.0]).unwrap();
    assert_eq!(result.len(), 3);
    assert!(result.iter().all(|v| v.to_bits() == 0));
}

#[test]
fn spec_r29_transpose_sorts_columns_with_many_contributors() {
    let a = Csr::from_parts(3, 2, vec![0, 1, 2, 3], vec![1, 1, 1], vec![1.0, 2.0, 3.0]).unwrap();
    let t = a.transpose();
    assert_eq!(t.row_ptr(), &[0, 0, 3]);
    assert_eq!(t.col_indices(), &[0, 1, 2]);
    assert_eq!(t.values(), &[1.0, 2.0, 3.0]);
}

#[test]
fn spec_r30_transpose_zero_shapes_and_involution() {
    for (rows, cols) in [(0, 3), (3, 0), (0, 0)] {
        let a = Csr::from_parts(rows, cols, vec![0; rows + 1], vec![], vec![]).unwrap();
        assert_eq!(a.transpose().shape(), (cols, rows));
        assert_eq!(a.transpose().transpose(), a);
    }
}

#[test]
fn spec_r31_multiply_dimension_mismatch_even_when_empty() {
    let a = Csr::from_parts(0, 2, vec![0], vec![], vec![]).unwrap();
    let b = Csr::from_parts(3, 0, vec![0, 0, 0, 0], vec![], vec![]).unwrap();
    assert_eq!(a.multiply(&b), Err(Error::DimensionMismatch { left_cols: 2, right_rows: 3 }));
}

#[test]
fn spec_r32_multiply_visits_left_inner_slots_then_rhs_slots() {
    let left = Csr::from_parts(1, 2, vec![0, 2], vec![0, 1], vec![1.0, 1.0]).unwrap();
    let rhs = Csr::from_parts(2, 1, vec![0, 1, 2], vec![0, 0], vec![1.0e16, -1.0e16]).unwrap();
    let product = left.multiply(&rhs).unwrap();
    assert_eq!(product.values(), &[0.0_f64.to_bits() as f64]);
}

#[test]
fn spec_r33_multiply_reports_product_and_sum_overflow() {
    let a = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![f64::MAX]).unwrap();
    let b = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![2.0]).unwrap();
    assert_eq!(a.multiply(&b), Err(Error::MultiplyOverflow { row: 0, inner: 0, column: 0 }));

    let left = Csr::from_parts(1, 2, vec![0, 2], vec![0, 1], vec![1.0, 1.0]).unwrap();
    let right = Csr::from_parts(2, 1, vec![0, 1, 2], vec![0, 0], vec![f64::MAX, f64::MAX]).unwrap();
    assert_eq!(
        left.multiply(&right),
        Err(Error::MultiplySumOverflow { row: 0, inner: 1, column: 0 })
    );
}

#[test]
fn spec_r34_multiply_omits_final_zero_and_keeps_tiny_nonzero() {
    let left = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![1.0]).unwrap();
    let zero = Csr::from_parts(1, 2, vec![0, 2], vec![0, 1], vec![1.0, -1.0]).unwrap();
    let result = left.multiply(&zero).unwrap();
    assert_eq!(result.row_ptr(), &[0, 0]);
    let tiny = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![f64::from_bits(1)]).unwrap();
    assert_eq!(left.multiply(&tiny).unwrap().values()[0].to_bits(), 1);
}

#[test]
fn spec_r35_multiply_handles_empty_rows_and_zero_inner_dimension() {
    let a = Csr::from_parts(3, 0, vec![0, 0, 0, 0], vec![], vec![]).unwrap();
    let b = Csr::from_parts(0, 2, vec![0], vec![], vec![]).unwrap();
    let result = a.multiply(&b).unwrap();
    assert_eq!(result.shape(), (3, 2));
    assert_eq!(result.row_ptr(), &[0, 0, 0, 0]);
}

#[test]
fn spec_r36_prune_tolerance_validation_includes_empty_matrix() {
    let a = Csr::from_parts(0, 0, vec![0], vec![], vec![]).unwrap();
    for tolerance in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert_eq!(a.prune(tolerance), Err(Error::Tolerance));
    }
    assert!(a.prune(-0.0).is_ok());
}

#[test]
fn spec_r37_prune_uses_inclusive_absolute_value_threshold() {
    let a = Csr::from_parts(1, 3, vec![0, 3], vec![0, 1, 2], vec![-0.5, 0.5, f64::from_bits(1)]).unwrap();
    let pruned = a.prune(0.5).unwrap();
    assert!(pruned.values().is_empty());
    assert_eq!(a.prune(0.0).unwrap().values().len(), 3);
}

#[test]
fn spec_r38_prune_preserves_shape_bits_and_is_idempotent() {
    let tiny = f64::from_bits(1);
    let a = Csr::from_parts(2, 2, vec![0, 1, 2], vec![0, 1], vec![tiny, 2.0]).unwrap();
    let once = a.prune(tiny).unwrap();
    let twice = once.prune(tiny).unwrap();
    assert_eq!(once.shape(), (2, 2));
    assert_eq!(once.row_ptr(), &[0, 0, 1]);
    assert_eq!(once.values(), &[2.0]);
    assert_eq!(twice, once);
    assert_eq!(a.values()[0].to_bits(), tiny.to_bits());
}

#[test]
fn spec_r39_text_header_uses_canonical_decimal() {
    let a = Csr::from_parts(0, 7, vec![0], vec![], vec![]).unwrap();
    assert_eq!(a.write_text(), "SM1 0 7 0\n");
}

#[test]
fn spec_r40_text_records_use_lowercase_fixed_width_bits_and_lf() {
    let a = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![1.0]).unwrap();
    assert_eq!(a.write_text(), "SM1 1 1 1\n0 0 3ff0000000000000\n");
}

#[test]
fn spec_r41_empty_text_has_header_only_and_keeps_shape() {
    let a = Csr::from_parts(0, 3, vec![0], vec![], vec![]).unwrap();
    assert_eq!(a.write_text(), "SM1 0 3 0\n");
}

#[test]
fn spec_r42_parse_requires_final_lf_before_other_checks() {
    assert_eq!(Csr::parse_text(""), Err(Error::Text { line: 1, kind: TextError::Newline }));
    assert_eq!(
        Csr::parse_text("invalid\nbad"),
        Err(Error::Text { line: 2, kind: TextError::Newline })
    );
}

#[test]
fn spec_r43_header_splits_only_on_single_ascii_spaces() {
    for text in ["SM1  1 1 0\n", "SM1\t1 1 0\n", " SM1 1 1 0\n", "SM1 1 1\n"] {
        assert_eq!(Csr::parse_text(text), Err(Error::Text { line: 1, kind: TextError::Header }));
    }
}

#[test]
fn spec_r44_decimal_grammar_and_line_numbers() {
    for token in ["", "+1", "-1", "01", "１２"] {
        let text = format!("SM1 {} 1 0\n", token);
        assert_eq!(Csr::parse_text(&text), Err(Error::Text { line: 1, kind: TextError::Decimal }));
    }
    assert_eq!(
        Csr::parse_text("SM1 1 1 1\n0 x 0000000000000000\n"),
        Err(Error::Text { line: 2, kind: TextError::Decimal })
    );
}

#[test]
fn spec_r45_parse_checks_dimensions_after_decimal_tokens() {
    assert_eq!(
        Csr::parse_text("SM1 4097 0 0\n"),
        Err(Error::Text { line: 1, kind: TextError::Dimension })
    );
    assert_eq!(
        Csr::parse_text("SM1 x 0 0\n"),
        Err(Error::Text { line: 1, kind: TextError::Decimal })
    );
}

#[test]
fn spec_r46_parse_count_precedes_record_inspection() {
    assert_eq!(
        Csr::parse_text("SM1 1 1 2\nbad\n"),
        Err(Error::Text { line: 1, kind: TextError::Count })
    );
    assert_eq!(
        Csr::parse_text("SM1 0 0 0\n\n"),
        Err(Error::Text { line: 1, kind: TextError::Count })
    );
}

#[test]
fn spec_r47_record_lexical_checks_precede_semantics() {
    assert_eq!(
        Csr::parse_text("SM1 1 1 1\n9 0 zzzzzzzzzzzzzzzz\n"),
        Err(Error::Text { line: 2, kind: TextError::Hex })
    );
    assert_eq!(
        Csr::parse_text("SM1 1 1 1\n0  0000000000000000\n"),
        Err(Error::Text { line: 2, kind: TextError::Record })
    );
}

#[test]
fn spec_r48_record_semantic_check_order_and_bit_values() {
    assert_eq!(
        Csr::parse_text("SM1 1 1 1\n1 1 7ff0000000000000\n"),
        Err(Error::Text { line: 2, kind: TextError::RowBounds })
    );
    assert_eq!(
        Csr::parse_text("SM1 1 1 1\n0 1 7ff0000000000000\n"),
        Err(Error::Text { line: 2, kind: TextError::ColumnBounds })
    );
    assert_eq!(
        Csr::parse_text("SM1 1 1 1\n0 0 7ff0000000000000\n"),
        Err(Error::Text { line: 2, kind: TextError::NonFinite })
    );
    for bits in ["0000000000000000", "8000000000000000"] {
        let text = format!("SM1 1 1 1\n0 0 {bits}\n");
        assert_eq!(Csr::parse_text(&text), Err(Error::Text { line: 2, kind: TextError::Zero }));
    }
    let tiny = Csr::parse_text("SM1 1 1 1\n0 0 0000000000000001\n").unwrap();
    assert_eq!(tiny.values()[0].to_bits(), 1);
}

#[test]
fn spec_r49_semantics_precede_record_order() {
    assert_eq!(
        Csr::parse_text("SM1 1 1 2\n0 0 3ff0000000000000\n0 0 7ff0000000000000\n"),
        Err(Error::Text { line: 3, kind: TextError::NonFinite })
    );
    assert_eq!(
        Csr::parse_text("SM1 1 1 2\n0 0 3ff0000000000000\n0 0 4000000000000000\n"),
        Err(Error::Text { line: 3, kind: TextError::Order })
    );
}

#[test]
fn spec_r50_text_round_trip_preserves_bits_and_canonical_string() {
    let a = Csr::from_parts(
        2,
        3,
        vec![0, 1, 2],
        vec![2, 0],
        vec![f64::from_bits(1), -2.5],
    )
    .unwrap();
    let text = a.write_text();
    let parsed = Csr::parse_text(&text).unwrap();
    assert_eq!(parsed, a);
    assert_eq!(parsed.write_text(), text);
    assert_eq!(parsed.values()[0].to_bits(), a.values()[0].to_bits());
}
