fn r(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter().map(|row| row.iter().map(|s| (*s).to_owned()).collect()).collect()
}

#[test]
fn public_unicode() {
    // R1-R2, R5-R6, R9-R11, R15-R18
    assert_eq!(render(&r(&[&["猫", "é"], &["two", "🙂"]]), &Options { alignments: vec![], header: false, max_width: 0 }).unwrap(), "+-----+---+\n| 猫   | é |\n| two | 🙂 |\n+-----+---+\n");
}

#[test]
fn public_clip() {
    // R6-R8, R12-R14, R16-R18
    assert_eq!(render(&r(&[&["alphabet"]]), &Options { alignments: vec![], header: false, max_width: 9 }).unwrap(), "+-------+\n| alph… |\n+-------+\n");
}

#[test]
fn public_header_alignment() {
    // R5, R15-R20
    assert_eq!(render(&r(&[&["ab", "cd"], &["long", "wide"]]), &Options { alignments: vec![Alignment::Right, Alignment::Center], header: true, max_width: 0 }).unwrap(), "+------+------+\n|   ab |  cd  |\n+------+------+\n| long | wide |\n+------+------+\n");
}

#[test]
fn public_large_budget() {
    // R6, R10-R11, R23
    assert_eq!(render(&r(&[&["x ", "value"]]), &Options { alignments: vec![], header: false, max_width: 100 }).unwrap(), "+---+-------+\n| x | value |\n+---+-------+\n");
}
