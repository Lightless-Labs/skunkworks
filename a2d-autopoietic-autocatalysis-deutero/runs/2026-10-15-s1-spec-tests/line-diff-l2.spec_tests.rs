#[test]
fn spec_r1_public_edit_api_is_exercised() {
    let result = unified_diff("a\n", "b\n", 0);
    assert_eq!(
        result,
        Ok("--- before\n+++ after\n@@ -1,1 +1,1 @@\n-a\n+b\n".to_string())
    );
}

#[test]
fn spec_r2_line_splitting() {
    assert_eq!(
        unified_diff("", "", 0),
        Ok(String::new())
    );
    assert_eq!(
        unified_diff("\n\n", "\n", 0),
        Ok("--- before\n+++ after\n@@ -1,2 +1,1 @@\n \n-\n".to_string())
    );
    assert_eq!(
        unified_diff("a\n", "a\n", 0),
        Ok(String::new())
    );
}

#[test]
fn spec_r3_requires_final_lf() {
    assert_eq!(
        unified_diff("x", "", 0),
        Err(DiffError::MissingFinalLf { side: Side::Old })
    );
    assert_eq!(
        unified_diff("", "x", 0),
        Err(DiffError::MissingFinalLf { side: Side::New })
    );
}

#[test]
fn spec_r4_rejects_cr_at_first_utf8_byte_offset() {
    assert_eq!(
        unified_diff("é\r\n", "x\n", 0),
        Err(DiffError::CarriageReturn {
            side: Side::Old,
            byte: 2
        })
    );
    assert_eq!(
        unified_diff("x\n", "a\r\nb\r\n", 0),
        Err(DiffError::CarriageReturn {
            side: Side::New,
            byte: 1
        })
    );
}

#[test]
fn spec_r5_line_count_limit_is_inclusive() {
    let at_limit = (0..128).map(|_| "x\n").collect::<String>();
    assert_eq!(unified_diff(&at_limit, &at_limit, 0), Ok(String::new()));

    let over_limit = (0..129).map(|_| "x\n").collect::<String>();
    assert_eq!(
        unified_diff(&over_limit, "", 0),
        Err(DiffError::TooLarge {
            old_len: 129,
            new_len: 0
        })
    );
}

#[test]
fn spec_r6_context_limit_is_inclusive() {
    assert_eq!(unified_diff("", "", 8), Ok(String::new()));
    assert_eq!(
        unified_diff("", "", 9),
        Err(DiffError::ContextTooLarge { context: 9 })
    );
}

#[test]
fn spec_r7_validation_precedence_and_identical_inputs() {
    assert_eq!(
        unified_diff("x\r", "y", 9),
        Err(DiffError::ContextTooLarge { context: 9 })
    );
    assert_eq!(
        unified_diff("x\r", "y\r", 0),
        Err(DiffError::CarriageReturn {
            side: Side::Old,
            byte: 1
        })
    );
    assert_eq!(
        unified_diff("x\r", "y\r", 0),
        Err(DiffError::CarriageReturn {
            side: Side::Old,
            byte: 1
        })
    );
    assert_eq!(
        unified_diff("x", "y", 0),
        Err(DiffError::MissingFinalLf { side: Side::Old })
    );
    assert_eq!(
        unified_diff("x\n", "y", 0),
        Err(DiffError::MissingFinalLf { side: Side::New })
    );

    let over_limit = (0..129).map(|_| "x\n").collect::<String>();
    assert_eq!(
        unified_diff(&over_limit, &over_limit, 0),
        Err(DiffError::TooLarge {
            old_len: 129,
            new_len: 129
        })
    );
}

#[test]
fn spec_r8_compares_verbatim_lines() {
    assert_eq!(
        unified_diff("A \n", "A\n", 0),
        Ok("--- before\n+++ after\n@@ -1,1 +1,1 @@\n-A \n+A\n".to_string())
    );
    assert_eq!(
        unified_diff("é\n", "e\u{301}\n", 0),
        Ok("--- before\n+++ after\n@@ -1,1 +1,1 @@\n-é\n+e\u{301}\n".to_string())
    );
    assert_eq!(
        unified_diff("a\0\n", "a\n", 0),
        Ok("--- before\n+++ after\n@@ -1,1 +1,1 @@\n-a\0\n+a\n".to_string())
    );
}

#[test]
fn spec_r9_lcs_minimizes_insertions_and_deletions() {
    assert_eq!(
        unified_diff("a\nb\n", "b\na\n", 0),
        Ok("--- before\n+++ after\n@@ -1,2 +1,2 @@\n-a\n b\n+a\n".to_string())
    );
}

#[test]
fn spec_r10_equal_lines_are_kept_and_unequal_scores_choose_direction() {
    assert_eq!(
        unified_diff("a\nb\n", "a\nc\n", 0),
        Ok("--- before\n+++ after\n@@ -2,1 +2,1 @@\n-b\n+c\n".to_string())
    );
    assert_eq!(
        unified_diff("a\n", "b\na\n", 0),
        Ok("--- before\n+++ after\n@@ -0,0 +1,1 @@\n+b\n".to_string())
    );
}

#[test]
fn spec_r11_tied_suffix_scores_use_parity() {
    assert_eq!(
        unified_diff("a\nb\n", "b\na\n", 0),
        Ok("--- before\n+++ after\n@@ -1,2 +1,2 @@\n-a\n b\n+a\n".to_string())
    );
    assert_eq!(
        unified_diff("a\nb\n", "c\nb\nd\n", 0),
        Ok("--- before\n+++ after\n@@ -1,2 +1,3 @@\n-a\n+c\n b\n+d\n".to_string())
    );
}

#[test]
fn spec_r12_exhaustion_emits_remaining_rows_with_original_indices() {
    assert_eq!(
        unified_diff("a\nb\n", "a\n", 0),
        Ok("--- before\n+++ after\n@@ -2,1 +1,0 @@\n-b\n".to_string())
    );
    assert_eq!(
        unified_diff("a\n", "a\nb\n", 0),
        Ok("--- before\n+++ after\n@@ -1,0 +2,1 @@\n+b\n".to_string())
    );
}

#[test]
fn spec_r13_identical_inputs_have_no_headers() {
    assert_eq!(unified_diff("one\n\n", "one\n\n", 8), Ok(String::new()));
}

#[test]
fn spec_r14_changes_begin_with_exact_file_headers() {
    assert_eq!(
        unified_diff("a\n", "b\n", 0).unwrap().starts_with("--- before\n+++ after\n"),
        true
    );
}

#[test]
fn spec_r15_consecutive_changes_form_one_group() {
    assert_eq!(
        unified_diff("a\nb\nc\n", "x\ny\nc\n", 0),
        Ok("--- before\n+++ after\n@@ -1,2 +1,2 @@\n-a\n-b\n+x\n+y\n".to_string())
    );
}

#[test]
fn spec_r16_groups_merge_at_twice_context_and_transitively() {
    assert_eq!(
        unified_diff("a\nb\nc\nd\ne\n", "A\nb\nC\nd\nE\n", 1),
        Ok("--- before\n+++ after\n@@ -1,5 +1,5 @@\n-a\n+A\n b\n-c\n+C\n d\n-e\n+E\n".to_string())
    );
}

#[test]
fn spec_r17_hunks_include_bounded_context() {
    assert_eq!(
        unified_diff("a\nb\nc\nd\ne\n", "a\nB\nc\nd\ne\n", 2),
        Ok("--- before\n+++ after\n@@ -1,4 +1,4 @@\n a\n-b\n+B\n c\n d\n".to_string())
    );
}

#[test]
fn spec_r18_context_counts_keeps_and_zero_context_separates_groups() {
    assert_eq!(
        unified_diff("a\nb\nc\n", "A\nb\nC\n", 0),
        Ok("--- before\n+++ after\n@@ -1,1 +1,1 @@\n-a\n+A\n@@ -3,1 +3,1 @@\n-c\n+C\n".to_string())
    );
    assert_eq!(
        unified_diff("a\nb\nc\n", "A\nb\nC\n", 1),
        Ok("--- before\n+++ after\n@@ -1,3 +1,3 @@\n-a\n+A\n b\n-c\n+C\n".to_string())
    );
}

#[test]
fn spec_r19_only_selected_rows_appear_in_script_order() {
    assert_eq!(
        unified_diff("a\nb\nc\nd\ne\n", "A\nb\nc\nd\nE\n", 0),
        Ok("--- before\n+++ after\n@@ -1,1 +1,1 @@\n-a\n+A\n@@ -5,1 +5,1 @@\n-e\n+E\n".to_string())
    );
}

#[test]
fn spec_r20_hunk_counts_include_keeps_and_side_specific_changes() {
    assert_eq!(
        unified_diff("a\nb\n", "x\na\n", 0),
        Ok("--- before\n+++ after\n@@ -1,0 +1,1 @@\n+x\n@@ -2,1 +2,0 @@\n-b\n".to_string())
    );
}

#[test]
fn spec_r21_zero_count_hunk_starts_use_consumed_line_positions() {
    assert_eq!(
        unified_diff("", "x\n", 0),
        Ok("--- before\n+++ after\n@@ -0,0 +1,1 @@\n+x\n".to_string())
    );
    assert_eq!(
        unified_diff("x\n", "", 0),
        Ok("--- before\n+++ after\n@@ -1,1 +0,0 @@\n-x\n".to_string())
    );
}

#[test]
fn spec_r22_hunk_headers_use_decimal_starts_and_always_show_counts() {
    assert_eq!(
        unified_diff("a\n", "b\n", 0),
        Ok("--- before\n+++ after\n@@ -1,1 +1,1 @@\n-a\n+b\n".to_string())
    );
}

#[test]
fn spec_r23_body_prefixes_preserve_blank_lines() {
    assert_eq!(
        unified_diff("\n", "x\n", 0),
        Ok("--- before\n+++ after\n@@ -1,1 +1,1 @@\n-\n+x\n".to_string())
    );
}

#[test]
fn spec_r24_output_has_lf_terminated_rows_without_extra_markers() {
    let output = unified_diff("a\n", "b\n", 0).unwrap();
    assert!(output.ends_with('\n'));
    assert!(!output.contains("No newline"));
    assert!(!output.contains('\r'));
    assert_eq!(
        output,
        "--- before\n+++ after\n@@ -1,1 +1,1 @@\n-a\n+b\n"
    );
}

#[test]
fn spec_r25_inputs_are_not_mutated_and_errors_have_no_partial_diff() {
    let old = String::from("a\n");
    let new = String::from("b\n");
    let old_before = old.clone();
    let new_before = new.clone();

    let result = unified_diff(&old, &new, 0);
    assert_eq!(old, old_before);
    assert_eq!(new, new_before);
    assert_eq!(
        result,
        Ok("--- before\n+++ after\n@@ -1,1 +1,1 @@\n-a\n+b\n".to_string())
    );

    assert_eq!(
        unified_diff("bad", "b\n", 0),
        Err(DiffError::MissingFinalLf { side: Side::Old })
    );
}
