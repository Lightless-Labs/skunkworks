// R1-R2: compilation checks the Rust public API.
fn r(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter().map(|row| row.iter().map(|s| (*s).to_owned()).collect()).collect()
}

#[test]
fn hidden_empty_ignores_options() {
    // R3, R17
    assert_eq!(render(&r(&[]), &Options { alignments: vec![Alignment::Right], header: true }).unwrap(), "");
}

#[test]
fn hidden_empty_row_precedence() {
    // R4, R17
    assert_eq!(render(&r(&[&["x", "y"], &["x"], &[]]), &Options { alignments: vec![Alignment::Left], header: false }), Err(TableError::EmptyRow { row: 2 }));
}

#[test]
fn hidden_ragged_precedence() {
    // R4-R5, R17
    assert_eq!(render(&r(&[&["\t"], &["x", "z"]]), &Options { alignments: vec![Alignment::Left, Alignment::Right], header: false }), Err(TableError::Ragged { row: 1, expected: 1, actual: 2 }));
}

#[test]
fn hidden_alignment_short() {
    // R5
    assert_eq!(render(&r(&[&["a", "b"]]), &Options { alignments: vec![Alignment::Left], header: false }), Err(TableError::AlignmentCount { expected: 2, actual: 1 }));
}

#[test]
fn hidden_alignment_long() {
    // R5
    assert_eq!(render(&r(&[&["a"]]), &Options { alignments: vec![Alignment::Left, Alignment::Right], header: false }), Err(TableError::AlignmentCount { expected: 1, actual: 2 }));
}

#[test]
fn hidden_alignment_before_char() {
    // R5-R6, R17
    assert_eq!(render(&r(&[&["\n"]]), &Options { alignments: vec![Alignment::Left, Alignment::Left], header: false }), Err(TableError::AlignmentCount { expected: 1, actual: 2 }));
}

#[test]
fn hidden_center_odd() {
    // R10
    assert_eq!(render(&r(&[&["x"], &["abcd"]]), &Options { alignments: vec![Alignment::Center], header: false }).unwrap(), "+------+\n|  x   |\n| abcd |\n+------+\n");
}

#[test]
fn hidden_center_even() {
    // R10
    assert_eq!(render(&r(&[&["v"], &["12345"]]), &Options { alignments: vec![Alignment::Center], header: false }).unwrap(), "+-------+\n|   v   |\n| 12345 |\n+-------+\n");
}

#[test]
fn hidden_center_empty() {
    // R7-R8, R10
    assert_eq!(render(&r(&[&[""], &["abc"]]), &Options { alignments: vec![Alignment::Center], header: false }).unwrap(), "+-----+\n|     |\n| abc |\n+-----+\n");
}

#[test]
fn hidden_right_empty() {
    // R7-R8, R10
    assert_eq!(render(&r(&[&[""], &["ab"]]), &Options { alignments: vec![Alignment::Right], header: false }).unwrap(), "+----+\n|    |\n| ab |\n+----+\n");
}

#[test]
fn hidden_header_only() {
    // R11-R16
    assert_eq!(render(&r(&[&["title", "h"]]), &Options { alignments: vec![], header: true }).unwrap(), "+-------+---+\n| title | h |\n+-------+---+\n");
}

#[test]
fn hidden_header_empty() {
    // R7-R8, R15-R16
    assert_eq!(render(&r(&[&["", ""], &["xx", "yy"]]), &Options { alignments: vec![Alignment::Center, Alignment::Right], header: true }).unwrap(), "+----+----+\n|    |    |\n+----+----+\n| xx | yy |\n+----+----+\n");
}

#[test]
fn hidden_body_no_separators() {
    // R11-R16
    assert_eq!(render(&r(&[&["h"], &["a"], &["bc"], &["d"]]), &Options { alignments: vec![], header: true }).unwrap(), "+----+\n| h  |\n+----+\n| a  |\n| bc |\n| d  |\n+----+\n");
}

#[test]
fn hidden_header_false() {
    // R15, R18
    assert_eq!(render(&r(&[&["a"], &["b"], &["c"]]), &Options { alignments: vec![], header: false }).unwrap(), "+---+\n| a |\n| b |\n| c |\n+---+\n");
}

#[test]
fn hidden_header_contributes() {
    // R8, R10, R16
    assert_eq!(render(&r(&[&["longest"], &["x"]]), &Options { alignments: vec![Alignment::Right], header: true }).unwrap(), "+---------+\n| longest |\n+---------+\n|       x |\n+---------+\n");
}

#[test]
fn hidden_empty_columns() {
    // R7-R8, R10-R13
    assert_eq!(render(&r(&[&["", " "], &[" ", ""]]), &Options { alignments: vec![Alignment::Right, Alignment::Center], header: false }).unwrap(), "+---+---+\n|   |   |\n|   |   |\n+---+---+\n");
}

#[test]
fn hidden_control_position() {
    // R2, R6
    assert_eq!(render(&r(&[&["ok", "ab\r"]]), &Options { alignments: vec![], header: false }), Err(TableError::InvalidChar { row: 0, column: 1, byte: 2 }));
}

#[test]
fn hidden_nonascii() {
    // R6
    assert_eq!(render(&r(&[&["zΩ"]]), &Options { alignments: vec![], header: false }), Err(TableError::InvalidChar { row: 0, column: 0, byte: 1 }));
}

#[test]
fn hidden_del() {
    // R6
    assert_eq!(render(&r(&[&["\u{7f}"]]), &Options { alignments: vec![], header: false }), Err(TableError::InvalidChar { row: 0, column: 0, byte: 0 }));
}

#[test]
fn hidden_width_exact() {
    // R7-R9
    assert_eq!(render(&r(&[&["eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee   "]]), &Options { alignments: vec![Alignment::Right], header: false }).unwrap(), "+----------------------------------+\n| eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee |\n+----------------------------------+\n");
}

#[test]
fn hidden_width_over() {
    // R9
    assert_eq!(render(&r(&[&["eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"]]), &Options { alignments: vec![], header: false }), Err(TableError::TooWide { column: 0, width: 33 }));
}

#[test]
fn hidden_width_priority() {
    // R8-R9
    assert_eq!(render(&r(&[&["a", "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"], &["zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz", "b"]]), &Options { alignments: vec![], header: false }), Err(TableError::TooWide { column: 0, width: 35 }));
}

#[test]
fn hidden_char_before_width() {
    // R6, R9, R17
    assert_eq!(render(&r(&[&["zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz", "b"], &["a", "\t"]]), &Options { alignments: vec![], header: false }), Err(TableError::InvalidChar { row: 1, column: 1, byte: 0 }));
}

#[test]
fn hidden_spaces_alignment() {
    // R7, R10
    assert_eq!(render(&r(&[&["  j  ", "a b "], &["length", "z"]]), &Options { alignments: vec![Alignment::Left, Alignment::Center], header: false }).unwrap(), "+--------+-----+\n|   j    | a b |\n| length |  z  |\n+--------+-----+\n");
}

#[test]
fn hidden_larger() {
    // R8, R10-R16
    assert_eq!(render(&r(&[&["0", "v", "z"], &["1", "vv", "z"], &["2", "vvv", "z"], &["3", "vvvv", "z"], &["4", "vvvvv", "z"], &["5", "vvvvvv", "z"], &["6", "vvvvvvv", "z"], &["7", "vvvvvvvv", "z"], &["8", "vvvvvvvvv", "z"], &["9", "v", "z"], &["10", "vv", "z"], &["11", "vvv", "z"], &["12", "vvvv", "z"], &["13", "vvvvv", "z"], &["14", "vvvvvv", "z"], &["15", "vvvvvvv", "z"], &["16", "vvvvvvvv", "z"], &["17", "vvvvvvvvv", "z"], &["18", "v", "z"], &["19", "vv", "z"], &["20", "vvv", "z"], &["21", "vvvv", "z"], &["22", "vvvvv", "z"], &["23", "vvvvvv", "z"], &["24", "vvvvvvv", "z"], &["25", "vvvvvvvv", "z"], &["26", "vvvvvvvvv", "z"], &["27", "v", "z"], &["28", "vv", "z"], &["29", "vvv", "z"]]), &Options { alignments: vec![Alignment::Right, Alignment::Center, Alignment::Left], header: true }).unwrap(), "+----+-----------+---+\n|  0 |     v     | z |\n+----+-----------+---+\n|  1 |    vv     | z |\n|  2 |    vvv    | z |\n|  3 |   vvvv    | z |\n|  4 |   vvvvv   | z |\n|  5 |  vvvvvv   | z |\n|  6 |  vvvvvvv  | z |\n|  7 | vvvvvvvv  | z |\n|  8 | vvvvvvvvv | z |\n|  9 |     v     | z |\n| 10 |    vv     | z |\n| 11 |    vvv    | z |\n| 12 |   vvvv    | z |\n| 13 |   vvvvv   | z |\n| 14 |  vvvvvv   | z |\n| 15 |  vvvvvvv  | z |\n| 16 | vvvvvvvv  | z |\n| 17 | vvvvvvvvv | z |\n| 18 |     v     | z |\n| 19 |    vv     | z |\n| 20 |    vvv    | z |\n| 21 |   vvvv    | z |\n| 22 |   vvvvv   | z |\n| 23 |  vvvvvv   | z |\n| 24 |  vvvvvvv  | z |\n| 25 | vvvvvvvv  | z |\n| 26 | vvvvvvvvv | z |\n| 27 |     v     | z |\n| 28 |    vv     | z |\n| 29 |    vvv    | z |\n+----+-----------+---+\n");
}

#[test]
fn hidden_pure_repeat() {
    // R16: immutable inputs and repeatable calls, including after an error.
    let input = r(&[&["one", "two"], &["three", "four"]]);
    let before = input.clone();
    let first = render(&input, &Options { alignments: vec![], header: false });
    let _ = render(&r(&[&[]]), &Options { alignments: vec![], header: false });
    assert_eq!(render(&input, &Options { alignments: vec![], header: false }), first);
    assert_eq!(input, before);
}
