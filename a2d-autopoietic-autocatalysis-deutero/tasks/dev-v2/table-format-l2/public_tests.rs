fn r(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter().map(|row| row.iter().map(|s| (*s).to_owned()).collect()).collect()
}

#[test]
fn public_alignments() {
    // R1-R2, R5-R8, R10-R14
    assert_eq!(render(&r(&[&["ab", "x", "uv"], &["long", "wide", "word"]]), &Options { alignments: vec![Alignment::Left, Alignment::Right, Alignment::Center], header: false }).unwrap(), "+------+------+------+\n| ab   |    x |  uv  |\n| long | wide | word |\n+------+------+------+\n");
}

#[test]
fn public_header() {
    // R10-R16
    assert_eq!(render(&r(&[&["key", "val"], &["x", "42"]]), &Options { alignments: vec![], header: true }).unwrap(), "+-----+-----+\n| key | val |\n+-----+-----+\n| x   | 42  |\n+-----+-----+\n");
}

#[test]
fn public_default() {
    // R5, R10-R13, R18
    assert_eq!(render(&r(&[&["old", "v"], &["n", "new"]]), &Options { alignments: vec![], header: false }).unwrap(), "+-----+-----+\n| old | v   |\n| n   | new |\n+-----+-----+\n");
}

#[test]
fn public_trim() {
    // R7-R8, R10-R13
    assert_eq!(render(&r(&[&[" a  ", "q "], &["xx", "s"]]), &Options { alignments: vec![], header: false }).unwrap(), "+----+---+\n|  a | q |\n| xx | s |\n+----+---+\n");
}
