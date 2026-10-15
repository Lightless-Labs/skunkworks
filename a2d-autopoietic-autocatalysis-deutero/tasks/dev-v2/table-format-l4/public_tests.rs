fn r(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter().map(|row| row.iter().map(|s| (*s).to_owned()).collect()).collect()
}

#[test]
fn public_multiline() {
    // R1, R2, R3, R4, R5, R11, R12, R13, R14, R15, R16, R18, R20, R21, R22, R23, R24, R30, R31, R32
    assert_eq!(render(&r(&[&["a\nb", "cat"], &["z", "dog"]]), &Options { alignments: vec![], header: true, max_width: 0, mode: Mode::Grid, overflow: vec![] }).unwrap(), "+---+-----+\n| a | cat |\n| b |     |\n+---+-----+\n| z | dog |\n+---+-----+\n");
}

#[test]
fn public_wrapping() {
    // R8, R9, R10, R17, R18, R19, R20, R21, R22, R23, R24
    assert_eq!(render(&r(&[&["abcdef", "x"]]), &Options { alignments: vec![], header: false, max_width: 12, mode: Mode::Grid, overflow: vec![] }).unwrap(), "+------+---+\n| abcd | x |\n| ef   |   |\n+------+---+\n");
}

#[test]
fn public_compact() {
    // R23, R24, R26, R35
    assert_eq!(render(&r(&[&["a", "bb"], &["long", "c"]]), &Options { alignments: vec![Alignment::Right, Alignment::Left], header: false, max_width: 0, mode: Mode::Compact, overflow: vec![] }).unwrap(), "   a  bb\nlong  c\n");
}

#[test]
fn public_markdown() {
    // R7, R14, R15, R16, R23, R24, R27, R28, R29, R31
    assert_eq!(render(&r(&[&["name", "a|b"], &["x", "\\"]]), &Options { alignments: vec![Alignment::Left, Alignment::Center], header: true, max_width: 0, mode: Mode::Markdown, overflow: vec![] }).unwrap(), "| name | a\\|b |\n| :--- | :--: |\n| x    |  \\\\  |\n");
}
