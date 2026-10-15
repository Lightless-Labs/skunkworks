#[test]
fn spec_r1_single_file_api_compiles() {
    let matrix = Sparse::from_coo(0, 0, &[]).unwrap();
    assert_eq!(matrix.shape(), (0, 0));
}

#[test]
fn spec_r2_public_types_and_clone_equality() {
    let axis = Axis::Row;
    let entry = Entry { row: 0, col: 0, value: 1.0 };
    assert_eq!(axis, Axis::Row);
    assert_eq!(entry, Entry { row: 0, col: 0, value: 1.0 });

    let sparse = Sparse::from_coo(1, 1, &[entry]).unwrap();
    assert_eq!(sparse.clone(), sparse);

    let csr = sparse.to_csr();
    assert_eq!(csr.clone(), csr);

    let error = Sparse::from_coo(MAX_DIM + 1, 0, &[]).unwrap_err();
    assert_eq!(error.clone(), Error::Shape { axis: Axis::Row, size: MAX_DIM + 1 });
}

#[test]
fn spec_r3_dimension_checks_precede_other_checks() {
    assert_eq!(
        Sparse::from_coo(MAX_DIM + 1, MAX_DIM + 1, &[]).unwrap_err(),
        Error::Shape { axis: Axis::Row, size: MAX_DIM + 1 }
    );
    assert_eq!(
        Sparse::from_coo(MAX_DIM, MAX_DIM + 1, &[]).unwrap_err(),
        Error::Shape { axis: Axis::Column, size: MAX_DIM + 1 }
    );
    assert_eq!(
        Csr::from_parts(MAX_DIM + 1, 0, vec![], vec![], vec![]).unwrap_err(),
        Error::Shape { axis: Axis::Row, size: MAX_DIM + 1 }
    );
    assert_eq!(
        Csr::from_parts(0, MAX_DIM + 1, vec![], vec![], vec![]).unwrap_err(),
        Error::Shape { axis: Axis::Column, size: MAX_DIM + 1 }
    );
    assert_eq!(Sparse::from_coo(MAX_DIM, MAX_DIM, &[]).unwrap().shape(), (MAX_DIM, MAX_DIM));
}

#[test]
fn spec_r4_zero_shapes_and_empty_coo() {
    assert_eq!(Sparse::from_coo(0, 7, &[]).unwrap().shape(), (0, 7));
    assert_eq!(Sparse::from_coo(5, 0, &[]).unwrap().shape(), (5, 0));
    assert_eq!(
        Sparse::from_coo(0, 1, &[Entry { row: 0, col: 0, value: 1.0 }]).unwrap_err(),
        Error::Bounds { entry: 0, axis: Axis::Row, index: 0, limit: 0 }
    );
    assert_eq!(
        Sparse::from_coo(1, 0, &[Entry { row: 0, col: 0, value: 1.0 }]).unwrap_err(),
        Error::Bounds { entry: 0, axis: Axis::Column, index: 0, limit: 0 }
    );
}

#[test]
fn spec_r5_coo_checks_entry_order_and_row_before_column() {
    let entries = [
        Entry { row: 0, col: 0, value: 1.0 },
        Entry { row: 4, col: 5, value: 1.0 },
        Entry { row: 3, col: 0, value: 1.0 },
    ];
    assert_eq!(
        Sparse::from_coo(3, 3, &entries).unwrap_err(),
        Error::Bounds { entry: 1, axis: Axis::Row, index: 4, limit: 3 }
    );

    let entries = [Entry { row: 0, col: 4, value: 1.0 }];
    assert_eq!(
        Sparse::from_coo(3, 3, &entries).unwrap_err(),
        Error::Bounds { entry: 0, axis: Axis::Column, index: 4, limit: 3 }
    );
}

#[test]
fn spec_r6_coo_nonfinite_values_and_bounds_priority() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            Sparse::from_coo(1, 1, &[Entry { row: 0, col: 0, value }]).unwrap_err(),
            Error::NonFinite { entry: 0 }
        );
    }
    assert_eq!(
        Sparse::from_coo(0, 0, &[Entry { row: 0, col: 0, value: f64::NAN }]).unwrap_err(),
        Error::Bounds { entry: 0, axis: Axis::Row, index: 0, limit: 0 }
    );
    let allowed = [
        Entry { row: 0, col: 0, value: 0.0 },
        Entry { row: 0, col: 1, value: -0.0 },
        Entry { row: 0, col: 2, value: f64::MIN_POSITIVE },
        Entry { row: 0, col: 3, value: f64::from_bits(1) },
    ];
    let sparse = Sparse::from_coo(1, 4, &allowed).unwrap();
    assert_eq!(sparse.entries().len(), 2);
    assert_eq!(sparse.entries()[0].value, f64::MIN_POSITIVE);
    assert_eq!(sparse.entries()[1].value.to_bits(), 1);
}

#[test]
fn spec_r7_coo_validation_finishes_before_summing() {
    let entries = [
        Entry { row: 0, col: 0, value: f64::MAX },
        Entry { row: 0, col: 0, value: f64::MAX },
        Entry { row: 2, col: 0, value: 1.0 },
    ];
    assert_eq!(
        Sparse::from_coo(1, 1, &entries).unwrap_err(),
        Error::Bounds { entry: 2, axis: Axis::Row, index: 2, limit: 1 }
    );
}

#[test]
fn spec_r8_coo_sums_in_input_order_including_zeros() {
    let entries = [
        Entry { row: 0, col: 0, value: 1.0e16 },
        Entry { row: 0, col: 0, value: 1.0 },
        Entry { row: 0, col: 0, value: -1.0e16 },
        Entry { row: 0, col: 1, value: 3.0 },
        Entry { row: 0, col: 1, value: 0.0 },
    ];
    let sparse = Sparse::from_coo(1, 2, &entries).unwrap();
    assert_eq!(sparse.entries()[0].value, 0.0);
    assert_eq!(sparse.entries()[1].value, 3.0);
}

#[test]
fn spec_r9_coo_overflow_is_reported_at_adding_entry() {
    let entries = [
        Entry { row: 0, col: 0, value: f64::MAX },
        Entry { row: 0, col: 0, value: f64::MAX },
        Entry { row: 0, col: 0, value: -f64::MAX },
    ];
    assert_eq!(Sparse::from_coo(1, 1, &entries).unwrap_err(), Error::SumOverflow { entry: 1 });
}

#[test]
fn spec_r10_coo_omits_final_zero_but_allows_later_contributions() {
    let entries = [
        Entry { row: 0, col: 0, value: 2.0 },
        Entry { row: 0, col: 0, value: -2.0 },
        Entry { row: 0, col: 0, value: 5.0 },
        Entry { row: 0, col: 1, value: -0.0 },
    ];
    let sparse = Sparse::from_coo(1, 2, &entries).unwrap();
    assert_eq!(sparse.entries(), &[Entry { row: 0, col: 0, value: 5.0 }]);
}

#[test]
fn spec_r11_coo_retains_smallest_subnormal() {
    let sparse = Sparse::from_coo(
        1,
        1,
        &[Entry { row: 0, col: 0, value: f64::from_bits(1) }],
    ).unwrap();
    assert_eq!(sparse.entries()[0].value.to_bits(), 1);
}

#[test]
fn spec_r12_coo_canonical_row_column_order() {
    let entries = [
        Entry { row: 2, col: 1, value: 4.0 },
        Entry { row: 0, col: 2, value: 3.0 },
        Entry { row: 0, col: 0, value: 1.0 },
        Entry { row: 1, col: 1, value: 2.0 },
    ];
    let sparse = Sparse::from_coo(3, 3, &entries).unwrap();
    assert_eq!(
        sparse.entries(),
        &[
            Entry { row: 0, col: 0, value: 1.0 },
            Entry { row: 0, col: 2, value: 3.0 },
            Entry { row: 1, col: 1, value: 2.0 },
            Entry { row: 2, col: 1, value: 4.0 },
        ]
    );
}

#[test]
fn spec_r13_shape_and_borrowed_canonical_entries() {
    let sparse = Sparse::from_coo(4, 7, &[]).unwrap();
    assert_eq!(sparse.shape(), (4, 7));
    assert!(sparse.entries().is_empty());

    let sparse = Sparse::from_coo(1, 1, &[Entry { row: 0, col: 0, value: 2.0 }]).unwrap();
    let entries = sparse.entries();
    assert_eq!(entries, &[Entry { row: 0, col: 0, value: 2.0 }]);
}

#[test]
fn spec_r14_coo_input_and_clone_are_unchanged() {
    let input = [
        Entry { row: 0, col: 1, value: 2.0 },
        Entry { row: 0, col: 0, value: 3.0 },
    ];
    let before = input;
    let sparse = Sparse::from_coo(1, 2, &input).unwrap();
    assert_eq!(input, before);
    assert_eq!(sparse.clone(), sparse);
    let _ = sparse.shape();
    let _ = sparse.entries();
    assert_eq!(input, before);
    assert_eq!(sparse.shape(), (1, 2));
}

#[test]
fn spec_r15_allowed_dimensions_have_no_extra_small_limit() {
    let entries = [Entry { row: MAX_DIM - 1, col: MAX_DIM - 1, value: 1.0 }];
    let sparse = Sparse::from_coo(MAX_DIM, MAX_DIM, &entries).unwrap();
    assert_eq!(sparse.entries(), &entries);
    assert_eq!(sparse.to_csr().shape(), (MAX_DIM, MAX_DIM));
}

#[test]
fn spec_r16_coo_error_indices_refer_to_original_input() {
    let entries = [
        Entry { row: 2, col: 0, value: 1.0 },
        Entry { row: 0, col: 0, value: 1.0 },
        Entry { row: 1, col: 3, value: 1.0 },
    ];
    assert_eq!(
        Sparse::from_coo(3, 3, &entries).unwrap_err(),
        Error::Bounds { entry: 2, axis: Axis::Column, index: 3, limit: 3 }
    );
}

#[test]
fn spec_r17_numeric_results_are_finite_and_exactly_ordered() {
    let sparse = Sparse::from_coo(
        1,
        1,
        &[
            Entry { row: 0, col: 0, value: 1.0e16 },
            Entry { row: 0, col: 0, value: 1.0 },
            Entry { row: 0, col: 0, value: -1.0e16 },
        ],
    ).unwrap();
    assert_eq!(sparse.entries()[0].value, 0.0);

    let csr = Sparse::from_coo(1, 1, &[Entry { row: 0, col: 0, value: 2.0 }]).unwrap().to_csr();
    let result = csr.mul_vector(&[3.0]).unwrap();
    assert_eq!(result, vec![6.0]);
    assert!(result.iter().all(|value| value.is_finite()));
}

#[test]
fn spec_r18_error_precedence_follows_validation_passes() {
    let entries = [
        Entry { row: 0, col: 0, value: f64::MAX },
        Entry { row: 0, col: 0, value: f64::MAX },
        Entry { row: 0, col: 1, value: f64::NAN },
    ];
    assert_eq!(
        Sparse::from_coo(1, 2, &entries).unwrap_err(),
        Error::NonFinite { entry: 2 }
    );
}

#[test]
fn spec_r19_csr_row_pointers_and_canonical_values() {
    let sparse = Sparse::from_coo(
        4,
        4,
        &[
            Entry { row: 3, col: 2, value: 5.0 },
            Entry { row: 0, col: 3, value: 2.0 },
            Entry { row: 0, col: 1, value: 1.0 },
        ],
    ).unwrap();
    let csr = sparse.to_csr();
    assert_eq!(csr.row_ptr(), &[0, 2, 2, 2, 3]);
    assert_eq!(csr.col_indices(), &[1, 3, 2]);
    assert_eq!(csr.values(), &[1.0, 2.0, 5.0]);
}

#[test]
fn spec_r20_csr_accessors_and_round_trip_preserve_shape_values() {
    let sparse = Sparse::from_coo(
        3,
        5,
        &[Entry { row: 2, col: 4, value: -3.5 }],
    ).unwrap();
    let csr = sparse.to_csr();
    assert_eq!(csr.shape(), (3, 5));
    assert_eq!(csr.to_coo(), sparse);
    assert_eq!(csr.row_ptr(), &[0, 0, 0, 1]);
    assert_eq!(csr.col_indices(), &[4]);
    assert_eq!(csr.values(), &[-3.5]);
}

#[test]
fn spec_r21_from_parts_length_checks_follow_shape() {
    assert_eq!(
        Csr::from_parts(1, 2, vec![], vec![], vec![]).unwrap_err(),
        Error::RowPtrLength { expected: 2, actual: 0 }
    );
    assert_eq!(
        Csr::from_parts(1, 2, vec![0, 0], vec![0], vec![]).unwrap_err(),
        Error::ColumnValueLength { columns: 1, values: 0 }
    );
    assert_eq!(
        Csr::from_parts(MAX_DIM + 1, 0, vec![], vec![0], vec![]).unwrap_err(),
        Error::Shape { axis: Axis::Row, size: MAX_DIM + 1 }
    );
}

#[test]
fn spec_r22_from_parts_endpoints_precede_other_pointer_checks() {
    assert_eq!(
        Csr::from_parts(2, 1, vec![1, 0, 0], vec![], vec![]).unwrap_err(),
        Error::RowPtrStart { found: 1 }
    );
    assert_eq!(
        Csr::from_parts(2, 1, vec![0, 2, 1], vec![99], vec![1.0]).unwrap_err(),
        Error::RowPtrEnd { found: 1, expected: 1 }
    );
    assert_eq!(
        Csr::from_parts(0, 1, vec![1], vec![], vec![]).unwrap_err(),
        Error::RowPtrStart { found: 1 }
    );
    assert_eq!(
        Csr::from_parts(0, 1, vec![0], vec![0], vec![1.0]).unwrap_err(),
        Error::RowPtrEnd { found: 0, expected: 1 }
    );
}

#[test]
fn spec_r23_from_parts_checks_pointer_order_before_slots() {
    assert_eq!(
        Csr::from_parts(2, 1, vec![0, 2, 1], vec![99], vec![1.0]).unwrap_err(),
        Error::RowPtrEnd { found: 1, expected: 1 }
    );
    assert_eq!(
        Csr::from_parts(2, 1, vec![0, 2, 2], vec![99], vec![1.0, 2.0]).unwrap_err(),
        Error::RowPtrOrder { row: 0 }
    );
}

#[test]
fn spec_r24_from_parts_slot_validation_order_and_row_local_columns() {
    assert_eq!(
        Csr::from_parts(1, 2, vec![0, 1], vec![2], vec![f64::NAN]).unwrap_err(),
        Error::ColumnBounds { slot: 0, column: 2, limit: 2 }
    );
    assert_eq!(
        Csr::from_parts(1, 3, vec![0, 2], vec![1, 1], vec![1.0, f64::NAN]).unwrap_err(),
        Error::ColumnOrder { slot: 1 }
    );
    assert_eq!(
        Csr::from_parts(1, 3, vec![0, 1], vec![0], vec![f64::INFINITY]).unwrap_err(),
        Error::NonFiniteValue { slot: 0 }
    );
    assert_eq!(
        Csr::from_parts(1, 3, vec![0, 1], vec![0], vec![-0.0]).unwrap_err(),
        Error::ExplicitZero { slot: 0 }
    );

    let csr = Csr::from_parts(
        2,
        2,
        vec![0, 1, 2],
        vec![1, 0],
        vec![1.0, 2.0],
    ).unwrap();
    assert_eq!(csr.col_indices(), &[1, 0]);
}

#[test]
fn spec_r25_vector_length_and_full_finiteness_validation() {
    let csr = Csr::from_parts(0, 2, vec![0], vec![], vec![]).unwrap();
    assert_eq!(
        csr.mul_vector(&[f64::NAN]).unwrap_err(),
        Error::VectorLength { expected: 2, actual: 1 }
    );
    assert_eq!(
        csr.mul_vector(&[1.0, f64::INFINITY]).unwrap_err(),
        Error::NonFiniteVector { index: 1 }
    );
    let no_rows = Csr::from_parts(0, 2, vec![0], vec![], vec![]).unwrap();
    assert_eq!(
        no_rows.mul_vector(&[f64::NEG_INFINITY, 1.0]).unwrap_err(),
        Error::NonFiniteVector { index: 0 }
    );
}

#[test]
fn spec_r26_vector_product_overflow_precedes_accumulation() {
    let csr = Csr::from_parts(1, 2, vec![0, 2], vec![0, 1], vec![f64::MAX, f64::MAX]).unwrap();
    assert_eq!(
        csr.mul_vector(&[2.0, 0.0]).unwrap_err(),
        Error::ProductOverflow { row: 0, column: 0 }
    );
}

#[test]
fn spec_r27_vector_accumulates_in_slot_order_and_checks_each_addition() {
    let csr = Csr::from_parts(
        1,
        3,
        vec![0, 3],
        vec![0, 1, 2],
        vec![f64::MAX, f64::MAX, -f64::MAX],
    ).unwrap();
    assert_eq!(
        csr.mul_vector(&[1.0, 1.0, 1.0]).unwrap_err(),
        Error::AccumulationOverflow { row: 0, column: 1 }
    );

    let underflow = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![f64::MIN_POSITIVE]).unwrap();
    assert_eq!(underflow.mul_vector(&[f64::from_bits(1)]).unwrap(), vec![0.0]);
}

#[test]
fn spec_r28_vector_has_one_normalized_value_per_row() {
    let csr = Csr::from_parts(3, 2, vec![0, 0, 1, 1], vec![1], vec![-2.0]).unwrap();
    let result = csr.mul_vector(&[9.0, 0.0]).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].to_bits(), 0.0f64.to_bits());
    assert_eq!(result[1].to_bits(), 0.0f64.to_bits());
    assert_eq!(result[2].to_bits(), 0.0f64.to_bits());
}

#[test]
fn spec_r29_transpose_sorts_columns_and_preserves_value_bits() {
    let sparse = Sparse::from_coo(
        3,
        2,
        &[
            Entry { row: 2, col: 0, value: f64::from_bits(1) },
            Entry { row: 0, col: 0, value: -3.0 },
            Entry { row: 1, col: 0, value: 4.0 },
        ],
    ).unwrap();
    let transposed = sparse.to_csr().transpose();
    assert_eq!(transposed.shape(), (2, 3));
    assert_eq!(transposed.row_ptr(), &[0, 3, 3]);
    assert_eq!(transposed.col_indices(), &[0, 1, 2]);
    assert_eq!(transposed.values()[0], -3.0);
    assert_eq!(transposed.values()[1], 4.0);
    assert_eq!(transposed.values()[2].to_bits(), 1);
}

#[test]
fn spec_r30_transpose_handles_empty_shapes_and_is_involutive() {
    for (rows, cols) in [(0, 0), (0, 4), (3, 0), (2, 5)] {
        let sparse = Sparse::from_coo(rows, cols, &[]).unwrap();
        let csr = sparse.to_csr();
        let twice = csr.transpose().transpose();
        assert_eq!(twice.to_coo(), sparse);
        assert_eq!(csr.shape(), (rows, cols));
        assert_eq!(csr.transpose().shape(), (cols, rows));
    }

    let sparse = Sparse::from_coo(
        2,
        3,
        &[Entry { row: 1, col: 2, value: f64::from_bits(1) }],
    ).unwrap();
    assert_eq!(
        sparse.to_csr().transpose().transpose().to_coo().entries()[0].value.to_bits(),
        1
    );
}

#[test]
fn spec_r31_multiply_dimension_mismatch_precedes_empty_case() {
    let left = Sparse::from_coo(0, 2, &[]).unwrap().to_csr();
    let right = Sparse::from_coo(3, 0, &[]).unwrap().to_csr();
    assert_eq!(
        left.multiply(&right).unwrap_err(),
        Error::DimensionMismatch { left_cols: 2, right_rows: 3 }
    );
}

#[test]
fn spec_r32_multiply_visits_left_inner_slots_then_rhs_row_slots() {
    let left = Csr::from_parts(1, 2, vec![0, 2], vec![0, 1], vec![1.0e16, 1.0]).unwrap();
    let right = Csr::from_parts(
        2,
        1,
        vec![0, 1, 2],
        vec![0, 0],
        vec![1.0, -1.0e16],
    ).unwrap();
    let product = left.multiply(&right).unwrap();
    assert_eq!(product.to_coo().entries()[0].value, 0.0);
}

#[test]
fn spec_r33_multiply_checks_product_and_each_accumulation() {
    let left = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![f64::MAX]).unwrap();
    let right = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![2.0]).unwrap();
    assert_eq!(
        left.multiply(&right).unwrap_err(),
        Error::MultiplyOverflow { row: 0, inner: 0, column: 0 }
    );

    let left = Csr::from_parts(
        1,
        3,
        vec![0, 3],
        vec![0, 1, 2],
        vec![f64::MAX, f64::MAX, -f64::MAX],
    ).unwrap();
    let right = Csr::from_parts(
        3,
        1,
        vec![0, 1, 2, 3],
        vec![0, 0, 0],
        vec![1.0, 1.0, 1.0],
    ).unwrap();
    assert_eq!(
        left.multiply(&right).unwrap_err(),
        Error::MultiplySumOverflow { row: 0, inner: 1, column: 0 }
    );
}

#[test]
fn spec_r34_multiply_prunes_only_final_zeros_and_keeps_tiny_values() {
    let left = Csr::from_parts(
        2,
        2,
        vec![0, 2, 2],
        vec![0, 1],
        vec![1.0, 1.0],
    ).unwrap();
    let right = Csr::from_parts(
        2,
        2,
        vec![0, 2, 3],
        vec![0, 1, 0],
        vec![1.0, 2.0, -1.0],
    ).unwrap();
    let product = left.multiply(&right).unwrap();
    assert_eq!(product.row_ptr(), &[0, 2, 2]);
    assert_eq!(product.col_indices(), &[0, 1]);
    assert_eq!(product.values(), &[0.0 + 1.0, 2.0]);

    let tiny_left = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![1.0]).unwrap();
    let tiny_right = Csr::from_parts(1, 1, vec![0, 1], vec![0], vec![f64::from_bits(1)]).unwrap();
    assert_eq!(tiny_left.multiply(&tiny_right).unwrap().values()[0].to_bits(), 1);
}

#[test]
fn spec_r35_multiply_empty_dimensions_and_row_pointers() {
    let left = Sparse::from_coo(3, 0, &[]).unwrap().to_csr();
    let right = Sparse::from_coo(0, 4, &[]).unwrap().to_csr();
    let product = left.multiply(&right).unwrap();
    assert_eq!(product.shape(), (3, 4));
    assert_eq!(product.row_ptr(), &[0, 0, 0, 0]);
    assert!(product.col_indices().is_empty());
    assert!(product.values().is_empty());

    let left = Sparse::from_coo(0, 2, &[]).unwrap().to_csr();
    let right = Sparse::from_coo(2, 3, &[]).unwrap().to_csr();
    let product = left.multiply(&right).unwrap();
    assert_eq!(product.shape(), (0, 3));
    assert_eq!(product.row_ptr(), &[0]);
}
