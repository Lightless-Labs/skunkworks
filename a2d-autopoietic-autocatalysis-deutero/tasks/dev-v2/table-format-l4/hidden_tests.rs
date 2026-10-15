// R1-R2: compilation checks the Rust public API.
fn r(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter().map(|row| row.iter().map(|s| (*s).to_owned()).collect()).collect()
}

#[test]
fn hidden_empty_ignores_everything() {
    // R3, R7, R34
    assert_eq!(render(&r(&[]), &Options { alignments: vec![Alignment::Left], header: false, max_width: 1, mode: Mode::Markdown, overflow: vec![Overflow::Clip] }).unwrap(), "");
}

#[test]
fn hidden_shape_precedence() {
    // R4, R34
    assert_eq!(render(&r(&[&["x", "y"], &["z"], &[]]), &Options { alignments: vec![Alignment::Right], header: false, max_width: 1, mode: Mode::Markdown, overflow: vec![] }), Err(TableError::EmptyRow { row: 2 }));
}

#[test]
fn hidden_ragged_precedence() {
    // R4, R34
    assert_eq!(render(&r(&[&["a"], &["b", "c"]]), &Options { alignments: vec![Alignment::Left, Alignment::Right], header: false, max_width: 0, mode: Mode::Grid, overflow: vec![] }), Err(TableError::Ragged { row: 1, expected: 1, actual: 2 }));
}

#[test]
fn hidden_alignment_precedence() {
    // R5, R6, R7, R34
    assert_eq!(render(&r(&[&["\t", "a"]]), &Options { alignments: vec![Alignment::Left], header: false, max_width: 1, mode: Mode::Markdown, overflow: vec![Overflow::Wrap] }), Err(TableError::AlignmentCount { expected: 2, actual: 1 }));
}

#[test]
fn hidden_overflow_precedence() {
    // R6, R7, R34
    assert_eq!(render(&r(&[&["\t", "a"]]), &Options { alignments: vec![], header: false, max_width: 1, mode: Mode::Markdown, overflow: vec![Overflow::Clip] }), Err(TableError::OverflowCount { expected: 2, actual: 1 }));
}

#[test]
fn hidden_markdown_precedence() {
    // R7, R34
    assert_eq!(render(&r(&[&["\u{0}"]]), &Options { alignments: vec![], header: false, max_width: 1, mode: Mode::Markdown, overflow: vec![] }), Err(TableError::MarkdownNeedsHeader));
}

#[test]
fn hidden_budget_precedence() {
    // R8: a nonzero max_width is a scalar budget.
    // R9, R10, R11, R12, R34
    assert_eq!(render(&r(&[&["\r"]]), &Options { alignments: vec![], header: false, max_width: 4, mode: Mode::Grid, overflow: vec![] }), Err(TableError::TooNarrow { minimum: 5, provided: 4 }));
}

#[test]
fn hidden_compact_minimum() {
    // R9, R10, R26
    assert_eq!(render(&r(&[&["", ""]]), &Options { alignments: vec![], header: false, max_width: 3, mode: Mode::Compact, overflow: vec![] }), Err(TableError::TooNarrow { minimum: 4, provided: 3 }));
}

#[test]
fn hidden_markdown_minimum() {
    // R9, R10, R27, R28
    assert_eq!(render(&r(&[&["", ""]]), &Options { alignments: vec![], header: true, max_width: 12, mode: Mode::Markdown, overflow: vec![] }), Err(TableError::TooNarrow { minimum: 13, provided: 12 }));
}

#[test]
fn hidden_crlf_offset() {
    // R11, R12, R13
    assert_eq!(render(&r(&[&["é\r\nx\r"]]), &Options { alignments: vec![], header: false, max_width: 0, mode: Mode::Grid, overflow: vec![] }), Err(TableError::InvalidChar { row: 0, column: 0, byte: 5 }));
}

#[test]
fn hidden_lone_cr() {
    // R11, R12
    assert_eq!(render(&r(&[&["a\rb"]]), &Options { alignments: vec![], header: false, max_width: 0, mode: Mode::Grid, overflow: vec![] }), Err(TableError::InvalidChar { row: 0, column: 0, byte: 1 }));
}

#[test]
fn hidden_c1_control() {
    // R11, R12
    assert_eq!(render(&r(&[&["🙂\u{85}"]]), &Options { alignments: vec![], header: false, max_width: 0, mode: Mode::Grid, overflow: vec![] }), Err(TableError::InvalidChar { row: 0, column: 0, byte: 4 }));
}

#[test]
fn hidden_separator_and_scan() {
    // R11, R12
    assert_eq!(render(&r(&[&["ok", "é\u{2029}"], &["\t", "x"]]), &Options { alignments: vec![], header: false, max_width: 0, mode: Mode::Grid, overflow: vec![] }), Err(TableError::InvalidChar { row: 0, column: 1, byte: 2 }));
}

#[test]
fn hidden_empty_lines_crlf() {
    // R11, R12, R13, R18, R21, R22, R23, R24, R25, R30, R31, R32
    assert_eq!(render(&r(&[&["\r\nA\r\n\r\n", "x\ny"], &["z", "q"]]), &Options { alignments: vec![], header: true, max_width: 0, mode: Mode::Grid, overflow: vec![] }).unwrap(), "+---+---+\n|   | x |\n| A | y |\n|   |   |\n|   |   |\n+---+---+\n| z | q |\n+---+---+\n");
}

#[test]
fn hidden_trim_per_line() {
    // R13, R16, R21, R22, R23, R24, R25
    assert_eq!(render(&r(&[&["  a  \n   \nb  ", "x"]]), &Options { alignments: vec![], header: false, max_width: 0, mode: Mode::Grid, overflow: vec![] }).unwrap(), "+-----+---+\n|   a | x |\n|     |   |\n| b   |   |\n+-----+---+\n");
}

#[test]
fn hidden_wrap_spaces() {
    // R13, R18, R20, R21, R22, R36
    assert_eq!(render(&r(&[&["ab  cd e", "x"]]), &Options { alignments: vec![], header: false, max_width: 12, mode: Mode::Grid, overflow: vec![] }).unwrap(), "+------+---+\n| ab   | x |\n| cd e |   |\n+------+---+\n");
}

#[test]
fn hidden_clip_per_line() {
    // R18, R19, R21, R22, R23, R24
    assert_eq!(render(&r(&[&["abcdef\nxy\n", "Q\nR\nS\nT"]]), &Options { alignments: vec![Alignment::Right, Alignment::Center], header: false, max_width: 11, mode: Mode::Grid, overflow: vec![Overflow::Clip, Overflow::Wrap] }).unwrap(), "+-----+---+\n| ab… | Q |\n|  xy | R |\n|     | S |\n|     | T |\n+-----+---+\n");
}

#[test]
fn hidden_exact_wrap() {
    // R20, R21, R22
    assert_eq!(render(&r(&[&["123456", "q"]]), &Options { alignments: vec![], header: false, max_width: 11, mode: Mode::Grid, overflow: vec![] }).unwrap(), "+-----+---+\n| 123 | q |\n| 456 |   |\n+-----+---+\n");
}

#[test]
fn hidden_wrap_scalar() {
    // R15, R20, R21, R22, R23, R24
    assert_eq!(render(&r(&[&["é🙂猫λ", "z"]]), &Options { alignments: vec![], header: false, max_width: 11, mode: Mode::Grid, overflow: vec![] }).unwrap(), "+-----+---+\n| é🙂 | z |\n| 猫λ  |   |\n+-----+---+\n");
}

#[test]
fn hidden_rightmost_allocation() {
    // R16, R17, R20, R22
    assert_eq!(render(&r(&[&["abcd", "wxyz"]]), &Options { alignments: vec![], header: false, max_width: 14, mode: Mode::Grid, overflow: vec![] }).unwrap(), "+------+-----+\n| abcd | wxy |\n|      | z   |\n+------+-----+\n");
}

#[test]
fn hidden_markdown_atomic_wrap() {
    // R14, R15, R16, R17, R20, R27, R28, R29
    assert_eq!(render(&r(&[&["aa|z\\q"]]), &Options { alignments: vec![], header: true, max_width: 7, mode: Mode::Markdown, overflow: vec![] }).unwrap(), "| aa  |\n| \\|z |\n| \\\\q |\n| :-- |\n");
}

#[test]
fn hidden_markdown_atomic_clip() {
    // R14, R15, R19, R23, R24, R27, R28, R29
    assert_eq!(render(&r(&[&["a|z\\q"]]), &Options { alignments: vec![Alignment::Right], header: true, max_width: 7, mode: Mode::Markdown, overflow: vec![Overflow::Clip] }).unwrap(), "|  a… |\n| --: |\n");
}

#[test]
fn hidden_markdown_clip_stops() {
    // R14, R19, R27, R28, R29
    assert_eq!(render(&r(&[&["ab|x"]]), &Options { alignments: vec![], header: true, max_width: 8, mode: Mode::Markdown, overflow: vec![Overflow::Clip] }).unwrap(), "| ab…  |\n| :--- |\n");
}

#[test]
fn hidden_markdown_markers() {
    // R9, R16, R23, R24, R27, R28, R29, R30, R31
    assert_eq!(render(&r(&[&["", "", ""], &["x", "yy", "zzzz"]]), &Options { alignments: vec![Alignment::Left, Alignment::Right, Alignment::Center], header: true, max_width: 0, mode: Mode::Markdown, overflow: vec![] }).unwrap(), "|     |     |      |\n| :-- | --: | :--: |\n| x   |  yy | zzzz |\n");
}

#[test]
fn hidden_markdown_multiline_header() {
    // R18, R21, R22, R27, R28, R29, R31, R32
    assert_eq!(render(&r(&[&["h1\nh2\n", "v"], &["body", "x"]]), &Options { alignments: vec![], header: true, max_width: 0, mode: Mode::Markdown, overflow: vec![] }).unwrap(), "| h1   | v   |\n| h2   |     |\n|      |     |\n| :--- | :-- |\n| body | x   |\n");
}

#[test]
fn hidden_compact_source_spaces() {
    // R20, R26, R36
    assert_eq!(render(&r(&[&["ab cd"]]), &Options { alignments: vec![], header: false, max_width: 3, mode: Mode::Compact, overflow: vec![] }).unwrap(), "ab \ncd\n");
}

#[test]
fn hidden_compact_only_padding() {
    // R21, R22, R23, R24, R26, R35
    assert_eq!(render(&r(&[&["", ""], &["xx", "abcd"]]), &Options { alignments: vec![Alignment::Center, Alignment::Center], header: false, max_width: 8, mode: Mode::Compact, overflow: vec![] }).unwrap(), "      \nxx  abcd\n");
}

#[test]
fn hidden_compact_empty_row() {
    // R13, R21, R26, R32
    assert_eq!(render(&r(&[&[""]]), &Options { alignments: vec![], header: true, max_width: 0, mode: Mode::Compact, overflow: vec![] }).unwrap(), "\n");
}

#[test]
fn hidden_compact_header_no_marker() {
    // R23, R24, R26, R30, R31, R32
    assert_eq!(render(&r(&[&["abc", "x"], &["z\ny", "q"]]), &Options { alignments: vec![Alignment::Right, Alignment::Left], header: true, max_width: 0, mode: Mode::Compact, overflow: vec![] }).unwrap(), "abc  x\n  z  q\n  y  \n");
}

#[test]
fn hidden_grid_single_header() {
    // R21, R22, R23, R24, R25, R30, R31, R32
    assert_eq!(render(&r(&[&["a\nb", "xy"]]), &Options { alignments: vec![], header: true, max_width: 0, mode: Mode::Grid, overflow: vec![] }).unwrap(), "+---+----+\n| a | xy |\n| b |    |\n+---+----+\n");
}

#[test]
fn hidden_clip_minimum() {
    // R9, R10, R17, R19, R22, R23, R24, R25
    assert_eq!(render(&r(&[&["猫abc", "xx"]]), &Options { alignments: vec![], header: false, max_width: 9, mode: Mode::Grid, overflow: vec![Overflow::Clip, Overflow::Clip] }).unwrap(), "+---+---+\n| … | … |\n+---+---+\n");
}

#[test]
fn hidden_literal_ellipsis() {
    // R19, R36
    assert_eq!(render(&r(&[&["…abcde"]]), &Options { alignments: vec![], header: false, max_width: 7, mode: Mode::Grid, overflow: vec![Overflow::Clip] }).unwrap(), "+-----+\n| …a… |\n+-----+\n");
}

#[test]
fn hidden_large_no_cap() {
    // R15, R16, R18, R20, R21, R22, R23, R24, R25, R35
    assert_eq!(render(&r(&[&["λλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλ\nxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx", "z"]]), &Options { alignments: vec![], header: false, max_width: 200, mode: Mode::Grid, overflow: vec![] }).unwrap(), "+----------------------------------------------------+---+\n| λλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλλ      | z |\n| xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx |   |\n+----------------------------------------------------+---+\n");
}

#[test]
fn hidden_nbsp_zero_width() {
    // R11, R13, R15, R16, R20, R21, R22, R23, R24
    assert_eq!(render(&r(&[&["a \n‍🙂", "x"]]), &Options { alignments: vec![], header: false, max_width: 0, mode: Mode::Grid, overflow: vec![] }).unwrap(), "+----+---+\n| a  | x |\n| ‍🙂 |   |\n+----+---+\n");
}

#[test]
fn hidden_pure_repeat() {
    // R33: immutable inputs and repeatable calls, including after an error.
    let input = r(&[&["one", "two"], &["three", "four"]]);
    let before = input.clone();
    let first = render(&input, &Options { alignments: vec![], header: false, max_width: 0, mode: Mode::Grid, overflow: vec![] });
    let _ = render(&r(&[&[]]), &Options { alignments: vec![], header: false, max_width: 0, mode: Mode::Grid, overflow: vec![] });
    assert_eq!(render(&input, &Options { alignments: vec![], header: false, max_width: 0, mode: Mode::Grid, overflow: vec![] }), first);
    assert_eq!(input, before);
}
