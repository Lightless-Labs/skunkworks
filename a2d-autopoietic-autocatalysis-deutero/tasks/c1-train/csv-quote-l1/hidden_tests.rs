
// R1, R2, R4, R5, R18
#[test]
fn h_empty_and_bom() {
assert_eq!(parse_csv(""), Ok(vec![]));
assert_eq!(parse_csv("\u{feff}"), Ok(vec![]));
assert_eq!(parse_csv("\u{feff}α,β"), Ok(vec![vec!["α".to_string(), "β".to_string()]]));
assert_eq!(parse_csv("solo"), Ok(vec![vec!["solo".to_string()]]));
assert_eq!(parse_csv(""), Ok(vec![]));
}

// R5, R6, R7, R16
#[test]
fn h_blanks_trailing() {
assert_eq!(parse_csv("\r\n\n,,\r\nx,"), Ok(vec![
vec!["".to_string()], vec!["".to_string()], vec!["".to_string();3], vec!["x".to_string(), "".to_string()]]));
assert_eq!(parse_csv("\"\"\n"), Ok(vec![vec!["".to_string()]]));
assert_eq!(parse_csv(","), Ok(vec![vec!["".to_string();2]]));
}

// R2, R9, R10, R15, R16
#[test]
fn h_quotes_and_literal_bytes() {
assert_eq!(parse_csv("\"a\"\"b\",\"c,\rd\r\ne\nf\",\\n"), Ok(vec![vec![
"a\"b".to_string(), "c,\rd\r\ne\nf".to_string(), "\\n".to_string()]]));
assert_eq!(parse_csv("\"\"\"\""), Ok(vec![vec!["\"".to_string()]]));
}

// R2, R4, R15
#[test]
fn h_bom_literal_and_whitespace() {
assert_eq!(parse_csv("\u{feff} x ,\u{feff}y\n\u{feff}z,\"\u{feff}\""), Ok(vec![
vec![" x ".to_string(), "\u{feff}y".to_string()], vec!["\u{feff}z".to_string(), "\u{feff}".to_string()]]));
assert_eq!(parse_csv("#tag\n \t"), Ok(vec![vec!["#tag".to_string()], vec![" \t".to_string()]]));
}

// R3, R8, R14
#[test]
fn h_unexpected_quote_offsets() {
assert_eq!(parse_csv("\u{feff}é\"bad"), Err(CsvError::UnexpectedQuote { record:0, byte:5 }));
assert_eq!(parse_csv("ok\r\n \"x\""), Err(CsvError::UnexpectedQuote { record:1, byte:5 }));
assert_eq!(parse_csv("a\"\0"), Err(CsvError::UnexpectedQuote { record:0, byte:1 }));
}

// R3, R10, R11, R14
#[test]
fn h_closed_tail_offsets() {
assert_eq!(parse_csv("\"line\nmore\" \0"), Err(CsvError::TrailingByte { record:0, byte:11 }));
assert_eq!(parse_csv("r\n\"é\"\t"), Err(CsvError::TrailingByte { record:1, byte:6 }));
assert_eq!(parse_csv("\"x\"z\r"), Err(CsvError::TrailingByte { record:0, byte:3 }));
}

// R3, R7, R11, R14
#[test]
fn h_bare_cr_precedence() {
for input in ["v\r", "v\rQ"] {
assert_eq!(parse_csv(input), Err(CsvError::BareCr { record:0, byte:1 }));
}
assert_eq!(parse_csv("ok\n\"a\"\r"), Err(CsvError::BareCr { record:1, byte:6 }));
assert_eq!(parse_csv("\"a\"\r\0"), Err(CsvError::BareCr { record:0, byte:3 }));
}

// R3, R9, R10, R12
#[test]
fn h_unclosed_opening_offsets() {
assert_eq!(parse_csv("\u{feff}ok\n\"é\nlast"), Err(CsvError::UnclosedQuote { record:1, byte:6 }));
assert_eq!(parse_csv("\"x\"\""), Err(CsvError::UnclosedQuote { record:0, byte:0 }));
assert_eq!(parse_csv("v,\""), Err(CsvError::UnclosedQuote { record:0, byte:2 }));
assert_eq!(parse_csv("\"end\""), Ok(vec![vec!["end".to_string()]]));
}

// R3, R12, R13, R14, R17, R18
#[test]
fn h_nul_and_large_input() {
assert_eq!(parse_csv("\"é\0"), Err(CsvError::Nul { record:0, byte:3 }));
assert_eq!(parse_csv("\"x\"\0"), Err(CsvError::Nul { record:0, byte:3 }));
assert_eq!(parse_csv("a\n\0"), Err(CsvError::Nul { record:1, byte:2 }));
let field = "λ".repeat(4096);
let input = (format!("{field},\"q\"\"r\"\r\n")).repeat(129);
let rows = parse_csv(&input).unwrap();
assert_eq!(rows.len(),129);
assert!(rows.iter().all(|r| r == &vec![field.clone(), "q\"r".to_string()]));
}

// R6
#[test]
fn h_only_delimiters() {
assert_eq!(parse_csv(",,,,"),Ok(vec![vec!["".to_string();5]]));
}

// R3, R8
#[test]
fn h_quote_at_unquoted_end() {
assert_eq!(parse_csv("été\""),Err(CsvError::UnexpectedQuote{record:0,byte:5}));
}

// R8
#[test]
fn h_space_before_quote() {
assert_eq!(parse_csv("\t\""),Err(CsvError::UnexpectedQuote{record:0,byte:1}));
}

// R12
#[test]
fn h_single_open_quote() {
assert_eq!(parse_csv("\""),Err(CsvError::UnclosedQuote{record:0,byte:0}));
}

// R9, R12
#[test]
fn h_escaped_final_pair() {
assert_eq!(parse_csv("a,\"\"\""),Err(CsvError::UnclosedQuote{record:0,byte:2}));
}

// R5, R16
#[test]
fn h_final_empty_quote() {
assert_eq!(parse_csv("\"\""),Ok(vec![vec!["".to_string()]]));
}

// R13
#[test]
fn h_nul_in_plain() {
assert_eq!(parse_csv("xx\0\""),Err(CsvError::Nul{record:0,byte:2}));
}

// R7, R11, R14
#[test]
fn h_cr_after_quoted_field() {
assert_eq!(parse_csv("\"hi\"\rX"),Err(CsvError::BareCr{record:0,byte:4}));
}

// R3, R11
#[test]
fn h_unicode_trailing() {
assert_eq!(parse_csv("\"k\"λ"),Err(CsvError::TrailingByte{record:0,byte:3}));
}

// R5, R7
#[test]
fn h_lf_crlf_mix() {
assert_eq!(parse_csv("1\n2\r\n3\n"),Ok(vec![vec!["1".to_string()],vec!["2".to_string()],vec!["3".to_string()]]));
}

// R10
#[test]
fn h_raw_embedded_cr() {
assert_eq!(parse_csv("\"\r\r\n\n\""),Ok(vec![vec!["\r\r\n\n".to_string()]]));
}

// R9, R16
#[test]
fn h_escape_is_single_pass() {
assert_eq!(parse_csv("\"\"\"\"\"\""),Ok(vec![vec!["\"\"".to_string()]]));
}

// R4
#[test]
fn h_second_bom_not_stripped() {
assert_eq!(parse_csv("\u{feff}\u{feff}"),Ok(vec![vec!["\u{feff}".to_string()]]));
}

// R5, R15
#[test]
fn h_two_blank_records() {
assert_eq!(parse_csv("\n\r\n"),Ok(vec![vec!["".to_string()],vec!["".to_string()]]));
}

// R11
#[test]
fn h_no_padding_after_close() {
assert_eq!(parse_csv("\"done\" ,"),Err(CsvError::TrailingByte{record:0,byte:6}));
}
