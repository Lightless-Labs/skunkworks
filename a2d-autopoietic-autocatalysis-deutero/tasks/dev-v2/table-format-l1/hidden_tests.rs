// R1-R2: compilation checks the Rust public API.
fn r(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter().map(|row| row.iter().map(|s| (*s).to_owned()).collect()).collect()
}

#[test]
fn hidden_empty() {
    // R3, R14
    assert_eq!(render(&r(&[])).unwrap(), "");
}

#[test]
fn hidden_documented_example() {
    // R9-R12, R15
    assert_eq!(render(&r(&[&["a", "bb"]])).unwrap(), "+---+----+\n| a | bb |\n+---+----+\n");
}

#[test]
fn hidden_empty_row() {
    // R3-R4, R14
    assert_eq!(render(&r(&[&["x"], &[]])), Err(TableError::EmptyRow { row: 1 }));
}

#[test]
fn hidden_empty_over_ragged() {
    // R4, R14
    assert_eq!(render(&r(&[&["x", "y"], &["z"], &[]])), Err(TableError::EmptyRow { row: 2 }));
}

#[test]
fn hidden_ragged_first() {
    // R4
    assert_eq!(render(&r(&[&["a", "b"], &["c"], &["d", "e", "f"]])), Err(TableError::Ragged { row: 1, expected: 2, actual: 1 }));
}

#[test]
fn hidden_shape_before_char() {
    // R4-R5, R14
    assert_eq!(render(&r(&[&["\t"], &["a", "b"]])), Err(TableError::Ragged { row: 1, expected: 1, actual: 2 }));
}

#[test]
fn hidden_tab_byte() {
    // R2, R5
    assert_eq!(render(&r(&[&["ab\tcd"]])), Err(TableError::InvalidChar { row: 0, column: 0, byte: 2 }));
}

#[test]
fn hidden_nonascii() {
    // R2, R5
    assert_eq!(render(&r(&[&["abé"]])), Err(TableError::InvalidChar { row: 0, column: 0, byte: 2 }));
}

#[test]
fn hidden_del() {
    // R5
    assert_eq!(render(&r(&[&["x\u{7f}"]])), Err(TableError::InvalidChar { row: 0, column: 0, byte: 1 }));
}

#[test]
fn hidden_invalid_order() {
    // R5
    assert_eq!(render(&r(&[&["ok", "a\n"], &["\t", "fine"]])), Err(TableError::InvalidChar { row: 0, column: 1, byte: 1 }));
}

#[test]
fn hidden_cr() {
    // R5
    assert_eq!(render(&r(&[&["\r"]])), Err(TableError::InvalidChar { row: 0, column: 0, byte: 0 }));
}

#[test]
fn hidden_all_empty() {
    // R6-R7, R9-R12
    assert_eq!(render(&r(&[&["", ""], &["   ", ""]])).unwrap(), "+---+---+\n|   |   |\n|   |   |\n+---+---+\n");
}

#[test]
fn hidden_only_spaces() {
    // R6-R7
    assert_eq!(render(&r(&[&["     "]])).unwrap(), "+---+\n|   |\n+---+\n");
}

#[test]
fn hidden_leading_preserved() {
    // R6-R7, R12
    assert_eq!(render(&r(&[&["  q", "z "], &["w", "   t"]])).unwrap(), "+-----+------+\n|   q | z    |\n| w   |    t |\n+-----+------+\n");
}

#[test]
fn hidden_internal_spaces() {
    // R6-R7, R12
    assert_eq!(render(&r(&[&["r  s"], &["t "]])).unwrap(), "+------+\n| r  s |\n| t    |\n+------+\n");
}

#[test]
fn hidden_width_boundary() {
    // R7-R8
    assert_eq!(render(&r(&[&["xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"]])).unwrap(), "+----------------------------------+\n| xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx |\n+----------------------------------+\n");
}

#[test]
fn hidden_width_over() {
    // R8
    assert_eq!(render(&r(&[&["mmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmm"]])), Err(TableError::TooWide { column: 0, width: 33 }));
}

#[test]
fn hidden_trim_before_width() {
    // R6-R8
    assert_eq!(render(&r(&[&["qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq        "]])).unwrap(), "+----------------------------------+\n| qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq |\n+----------------------------------+\n");
}

#[test]
fn hidden_width_maximum() {
    // R7-R8
    assert_eq!(render(&r(&[&["xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"], &["yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy"]])), Err(TableError::TooWide { column: 0, width: 39 }));
}

#[test]
fn hidden_width_column_order() {
    // R8
    assert_eq!(render(&r(&[&["a", "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"], &["yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy", "b"]])), Err(TableError::TooWide { column: 0, width: 34 }));
}

#[test]
fn hidden_char_over_width() {
    // R5, R8, R14
    assert_eq!(render(&r(&[&["xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"], &["z\u{0}"]])), Err(TableError::InvalidChar { row: 1, column: 0, byte: 1 }));
}

#[test]
fn hidden_scan_original() {
    // R5-R6
    assert_eq!(render(&r(&[&["ab\t   "]])), Err(TableError::InvalidChar { row: 0, column: 0, byte: 2 }));
}

#[test]
fn hidden_single_empty() {
    // R6-R7, R9-R11
    assert_eq!(render(&r(&[&[""]])).unwrap(), "+---+\n|   |\n+---+\n");
}

#[test]
fn hidden_many_columns() {
    // R7, R9-R12
    assert_eq!(render(&r(&[&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"], &["!", "!", "!", "!", "!", "!", "!", "!", "!", "!", "!", "!"]])).unwrap(), "+---+---+---+---+---+---+---+---+---+---+----+----+\n| 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 |\n| ! | ! | ! | ! | ! | ! | ! | ! | ! | ! | !  | !  |\n+---+---+---+---+---+---+---+---+---+---+----+----+\n");
}

#[test]
fn hidden_many_rows() {
    // R7, R9-R12
    assert_eq!(render(&r(&[&["0", "60"], &["1", "59"], &["2", "58"], &["3", "57"], &["4", "56"], &["5", "55"], &["6", "54"], &["7", "53"], &["8", "52"], &["9", "51"], &["10", "50"], &["11", "49"], &["12", "48"], &["13", "47"], &["14", "46"], &["15", "45"], &["16", "44"], &["17", "43"], &["18", "42"], &["19", "41"], &["20", "40"], &["21", "39"], &["22", "38"], &["23", "37"], &["24", "36"], &["25", "35"], &["26", "34"], &["27", "33"], &["28", "32"], &["29", "31"], &["30", "30"], &["31", "29"], &["32", "28"], &["33", "27"], &["34", "26"], &["35", "25"], &["36", "24"], &["37", "23"], &["38", "22"], &["39", "21"]])).unwrap(), "+----+----+\n| 0  | 60 |\n| 1  | 59 |\n| 2  | 58 |\n| 3  | 57 |\n| 4  | 56 |\n| 5  | 55 |\n| 6  | 54 |\n| 7  | 53 |\n| 8  | 52 |\n| 9  | 51 |\n| 10 | 50 |\n| 11 | 49 |\n| 12 | 48 |\n| 13 | 47 |\n| 14 | 46 |\n| 15 | 45 |\n| 16 | 44 |\n| 17 | 43 |\n| 18 | 42 |\n| 19 | 41 |\n| 20 | 40 |\n| 21 | 39 |\n| 22 | 38 |\n| 23 | 37 |\n| 24 | 36 |\n| 25 | 35 |\n| 26 | 34 |\n| 27 | 33 |\n| 28 | 32 |\n| 29 | 31 |\n| 30 | 30 |\n| 31 | 29 |\n| 32 | 28 |\n| 33 | 27 |\n| 34 | 26 |\n| 35 | 25 |\n| 36 | 24 |\n| 37 | 23 |\n| 38 | 22 |\n| 39 | 21 |\n+----+----+\n");
}

#[test]
fn hidden_pure_repeat() {
    // R13: immutable inputs and repeatable calls, including after an error.
    let input = r(&[&["one", "two"], &["three", "four"]]);
    let before = input.clone();
    let first = render(&input);
    let _ = render(&r(&[&[]]));
    assert_eq!(render(&input), first);
    assert_eq!(input, before);
}
