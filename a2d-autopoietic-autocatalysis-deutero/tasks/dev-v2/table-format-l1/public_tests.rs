fn r(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter().map(|row| row.iter().map(|s| (*s).to_owned()).collect()).collect()
}

#[test]
fn public_grid() {
    // R1-R2, R9-R12
    assert_eq!(render(&r(&[&["hi", "v"]])).unwrap(), "+----+---+\n| hi | v |\n+----+---+\n");
}

#[test]
fn public_widths() {
    // R7, R9-R12
    assert_eq!(render(&r(&[&["cat", "7"], &["x", "100"]])).unwrap(), "+-----+-----+\n| cat | 7   |\n| x   | 100 |\n+-----+-----+\n");
}

#[test]
fn public_spaces() {
    // R6-R7, R9-R12
    assert_eq!(render(&r(&[&[" a  ", "b "], &["xx", "c"]])).unwrap(), "+----+---+\n|  a | b |\n| xx | c |\n+----+---+\n");
}

#[test]
fn public_punctuation() {
    // R5, R9-R12
    assert_eq!(render(&r(&[&["+|-", "!?"], &["Z", "A"]])).unwrap(), "+-----+----+\n| +|- | !? |\n| Z   | A  |\n+-----+----+\n");
}
