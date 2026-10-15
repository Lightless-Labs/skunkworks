// R1-R2: compilation checks the Rust public API.
fn r(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter().map(|row| row.iter().map(|s| (*s).to_owned()).collect()).collect()
}

#[test]
fn hidden_empty_ignores() {
    // R3, R22
    assert_eq!(render(&r(&[]), &Options { alignments: vec![Alignment::Left], header: false, max_width: 1 }).unwrap(), "");
}

#[test]
fn hidden_empty_over_ragged() {
    // R4, R22
    assert_eq!(render(&r(&[&["a", "b"], &["c"], &[]]), &Options { alignments: vec![], header: false, max_width: 0 }), Err(TableError::EmptyRow { row: 2 }));
}

#[test]
fn hidden_ragged_over_options() {
    // R4-R5, R22
    assert_eq!(render(&r(&[&["a"], &["b", "c"]]), &Options { alignments: vec![Alignment::Center, Alignment::Right], header: false, max_width: 1 }), Err(TableError::Ragged { row: 1, expected: 1, actual: 2 }));
}

#[test]
fn hidden_count_over_budget() {
    // R5, R8-R9, R22
    assert_eq!(render(&r(&[&["\t"]]), &Options { alignments: vec![Alignment::Left, Alignment::Right], header: false, max_width: 1 }), Err(TableError::AlignmentCount { expected: 1, actual: 2 }));
}

#[test]
fn hidden_budget_over_char() {
    // R7-R9, R22
    assert_eq!(render(&r(&[&["\n", "x"]]), &Options { alignments: vec![], header: false, max_width: 8 }), Err(TableError::TooNarrow { minimum: 9, provided: 8 }));
}

#[test]
fn hidden_one_column_minimum() {
    // R6-R8, R12-R14
    assert_eq!(render(&r(&[&["abc"]]), &Options { alignments: vec![], header: false, max_width: 5 }).unwrap(), "+---+\n| … |\n+---+\n");
}

#[test]
fn hidden_too_narrow() {
    // R7-R8
    assert_eq!(render(&r(&[&[""]]), &Options { alignments: vec![], header: false, max_width: 4 }), Err(TableError::TooNarrow { minimum: 5, provided: 4 }));
}

#[test]
fn hidden_all_columns_minimum() {
    // R6-R8, R12-R14
    assert_eq!(render(&r(&[&["ab", "cd", "ef"]]), &Options { alignments: vec![], header: false, max_width: 13 }).unwrap(), "+---+---+---+\n| … | … | … |\n+---+---+---+\n");
}

#[test]
fn hidden_rightmost_tie() {
    // R12-R14
    assert_eq!(render(&r(&[&["abcd", "wxyz"]]), &Options { alignments: vec![], header: false, max_width: 14 }).unwrap(), "+------+-----+\n| abcd | wx… |\n+------+-----+\n");
}

#[test]
fn hidden_repeated_shrink() {
    // R12-R15
    assert_eq!(render(&r(&[&["abcdef", "123456", "UV"]]), &Options { alignments: vec![Alignment::Left, Alignment::Right, Alignment::Center], header: false, max_width: 18 }).unwrap(), "+-----+-----+----+\n| ab… | 12… | UV |\n+-----+-----+----+\n");
}

#[test]
fn hidden_unicode_truncate() {
    // R11-R14
    assert_eq!(render(&r(&[&["猫é🙂界"]]), &Options { alignments: vec![], header: false, max_width: 7 }).unwrap(), "+-----+\n| 猫é… |\n+-----+\n");
}

#[test]
fn hidden_grapheme_scalar() {
    // R11, R13-R14
    assert_eq!(render(&r(&[&["é🙂x"]]), &Options { alignments: vec![], header: false, max_width: 7 }).unwrap(), "+-----+\n| é… |\n+-----+\n");
}

#[test]
fn hidden_nonbreaking_space() {
    // R10-R11, R15
    assert_eq!(render(&r(&[&["a   "], &["z"]]), &Options { alignments: vec![], header: false, max_width: 0 }).unwrap(), "+----+\n| a  |\n| z  |\n+----+\n");
}

#[test]
fn hidden_zero_width() {
    // R9-R11, R15
    assert_eq!(render(&r(&[&["a‍b"], &["x"]]), &Options { alignments: vec![], header: false, max_width: 0 }).unwrap(), "+-----+\n| a‍b |\n| x   |\n+-----+\n");
}

#[test]
fn hidden_c1_rejected() {
    // R2, R9
    assert_eq!(render(&r(&[&["é\u{85}"]]), &Options { alignments: vec![], header: false, max_width: 0 }), Err(TableError::InvalidChar { row: 0, column: 0, byte: 2 }));
}

#[test]
fn hidden_line_separator() {
    // R2, R9
    assert_eq!(render(&r(&[&["🙂\u{2028}"]]), &Options { alignments: vec![], header: false, max_width: 0 }), Err(TableError::InvalidChar { row: 0, column: 0, byte: 4 }));
}

#[test]
fn hidden_paragraph_separator() {
    // R9
    assert_eq!(render(&r(&[&["a\u{2029}"]]), &Options { alignments: vec![], header: false, max_width: 0 }), Err(TableError::InvalidChar { row: 0, column: 0, byte: 1 }));
}

#[test]
fn hidden_unicode_error_offset() {
    // R2, R9
    assert_eq!(render(&r(&[&["界é\t"]]), &Options { alignments: vec![], header: false, max_width: 0 }), Err(TableError::InvalidChar { row: 0, column: 0, byte: 5 }));
}

#[test]
fn hidden_invalid_row_order() {
    // R9
    assert_eq!(render(&r(&[&["ok", "é\r"], &["\u{0}", "ok"]]), &Options { alignments: vec![], header: false, max_width: 0 }), Err(TableError::InvalidChar { row: 0, column: 1, byte: 2 }));
}

#[test]
fn hidden_trim_before_allocation() {
    // R10-R12
    assert_eq!(render(&r(&[&["abc    ", "xy"]]), &Options { alignments: vec![], header: false, max_width: 12 }).unwrap(), "+-----+----+\n| abc | xy |\n+-----+----+\n");
}

#[test]
fn hidden_empty_columns() {
    // R10-R12, R16-R18
    assert_eq!(render(&r(&[&["", "   "]]), &Options { alignments: vec![], header: false, max_width: 9 }).unwrap(), "+---+---+\n|   |   |\n+---+---+\n");
}

#[test]
fn hidden_exact_fit() {
    // R11, R13-R14
    assert_eq!(render(&r(&[&["é🙂猫"]]), &Options { alignments: vec![], header: false, max_width: 7 }).unwrap(), "+-----+\n| é🙂猫 |\n+-----+\n");
}

#[test]
fn hidden_no_fixed_cap() {
    // R11, R23
    assert_eq!(render(&r(&[&["λλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλ"]]), &Options { alignments: vec![], header: false, max_width: 0 }).unwrap(), "+------------------------------------------+\n| λλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλ |\n+------------------------------------------+\n");
}

#[test]
fn hidden_literal_ellipsis() {
    // R13-R14
    assert_eq!(render(&r(&[&["…abcd"]]), &Options { alignments: vec![], header: false, max_width: 7 }).unwrap(), "+-----+\n| …a… |\n+-----+\n");
}

#[test]
fn hidden_header_truncated() {
    // R12-R20
    assert_eq!(render(&r(&[&["longheader", "head"], &["z", "q"]]), &Options { alignments: vec![Alignment::Center, Alignment::Right], header: true, max_width: 13 }).unwrap(), "+-----+-----+\n| lo… | he… |\n+-----+-----+\n|  z  |   q |\n+-----+-----+\n");
}

#[test]
fn hidden_header_only() {
    // R16-R20
    assert_eq!(render(&r(&[&["é", "🙂"]]), &Options { alignments: vec![], header: true, max_width: 0 }).unwrap(), "+---+---+\n| é | 🙂 |\n+---+---+\n");
}

#[test]
fn hidden_many_columns() {
    // R11-R18, R23
    assert_eq!(render(&r(&[&["00000", "11111", "22222", "33333", "44444", "55555", "66666", "77777", "88888", "99999", "1010101010", "1111111111", "1212121212", "1313131313"], &["Ω", "Ω", "Ω", "Ω", "Ω", "Ω", "Ω", "Ω", "Ω", "Ω", "Ω", "Ω", "Ω", "Ω"]]), &Options { alignments: vec![], header: false, max_width: 65 }).unwrap(), "+----+----+----+----+----+----+----+----+---+---+---+---+---+---+\n| 0… | 1… | 2… | 3… | 4… | 5… | 6… | 7… | … | … | … | … | … | … |\n| Ω  | Ω  | Ω  | Ω  | Ω  | Ω  | Ω  | Ω  | Ω | Ω | Ω | Ω | Ω | Ω |\n+----+----+----+----+----+----+----+----+---+---+---+---+---+---+\n");
}

#[test]
fn hidden_center_odd() {
    // R15
    assert_eq!(render(&r(&[&["x"], &["four"]]), &Options { alignments: vec![Alignment::Center], header: false, max_width: 0 }).unwrap(), "+------+\n|  x   |\n| four |\n+------+\n");
}

#[test]
fn hidden_pure_repeat() {
    // R21: immutable inputs and repeatable calls, including after an error.
    let input = r(&[&["one", "two"], &["three", "four"]]);
    let before = input.clone();
    let first = render(&input, &Options { alignments: vec![], header: false, max_width: 0 });
    let _ = render(&r(&[&[]]), &Options { alignments: vec![], header: false, max_width: 0 });
    assert_eq!(render(&input, &Options { alignments: vec![], header: false, max_width: 0 }), first);
    assert_eq!(input, before);
}
