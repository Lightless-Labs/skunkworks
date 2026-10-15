fn cfg(headers: bool, duplicates: DuplicatePolicy) -> Config {
    Config { delimiter:b',', quote:b'"', headers, duplicates }
}
fn strings(values: &[&str]) -> Vec<String> { values.iter().map(|s| s.to_string()).collect() }
fn table(header: Option<&[&str]>, rows: &[&[&str]], duplicates: DuplicatePolicy) -> Table {
    Table { header:header.map(strings), rows:rows.iter().map(|r| strings(r)).collect(), duplicates }
}

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

// R19, R27, R29
#[test]
fn h_config_precedence() {
let mut c=cfg(false,DuplicatePolicy::Reject); c.delimiter=0; c.quote=255;
assert_eq!(read_csv("\0",c),Err(CsvError::InvalidConfig{component:ConfigPart::Delimiter}));
assert!(matches!(StreamReader::new(c),Err(CsvError::InvalidConfig{component:ConfigPart::Delimiter})));
c.delimiter=b',';
assert_eq!(read_csv("",c),Err(CsvError::InvalidConfig{component:ConfigPart::Quote}));
c.quote=b',';
assert_eq!(read_csv("",c),Err(CsvError::InvalidConfig{component:ConfigPart::Quote}));
for b in [b' ',b'\n',127,128,255] { c.delimiter=b; c.quote=b'"'; assert_eq!(read_csv("",c),Err(CsvError::InvalidConfig{component:ConfigPart::Delimiter})); }
}

// R19, R20
#[test]
fn h_dialect_boundaries() {
let c=Config{delimiter:b'!',quote:b'~',headers:false,duplicates:DuplicatePolicy::KeepFirst};
assert_eq!(read_csv("~a!b~~c~!\"q\",z",c).unwrap().rows,vec![strings(&["a!b~c","\"q\",z"])]);
let c=Config{delimiter:b'X',quote:b'Q',..c};
assert_eq!(read_csv("QaXXbQXc",c).unwrap().rows,vec![strings(&["aXXb","c"])]);
assert_eq!(parse_csv("a;b").unwrap(),vec![strings(&["a;b"])]);
}

// R3, R4, R7, R9, R21, R29
#[test]
fn h_chunk_every_boundary() {
let input="\u{feff}a,\"b\"\"c\r\nd\"\r\ne,f\n";
let c=cfg(false,DuplicatePolicy::KeepLast); let expected=read_csv(input,c);
let boundaries:Vec<_>=input.char_indices().map(|(i,_)|i).chain(std::iter::once(input.len())).collect();
for &split in &boundaries { let mut r=StreamReader::new(c).unwrap(); r.feed("").unwrap(); r.feed(&input[..split]).unwrap(); r.feed(&input[split..]).unwrap(); assert_eq!(r.finish(),expected); }
let mut r=StreamReader::new(c).unwrap(); for ch in input.chars(){r.feed(&ch.to_string()).unwrap();} assert_eq!(r.finish(),expected);
}

// R3, R14, R21, R22
#[test]
fn h_deferred_cached_errors() {
let mut r=StreamReader::new(cfg(false,DuplicatePolicy::Reject)).unwrap();
r.feed("good\r").unwrap(); r.feed("\nλ").unwrap(); r.feed("\"bad").unwrap();
let error=Err(CsvError::UnexpectedQuote{record:1,byte:8});
assert_eq!(r.finish(),error); assert_eq!(r.finish(),error);
assert_eq!(r.feed(""),Err(CsvError::ReaderFinished));
assert_eq!(r.feed("tail"),Err(CsvError::ReaderFinished)); assert_eq!(r.finish(),error);
}

// R22, R23, R24
#[test]
fn h_success_cache_and_reset() {
let c=Config{delimiter:b';',quote:b'\'',headers:true,duplicates:DuplicatePolicy::KeepLast};
let mut r=StreamReader::new(c).unwrap(); r.feed("a;b\n1;2").unwrap(); let got=r.finish();
assert_eq!(r.finish(),got); assert_eq!(r.feed("more"),Err(CsvError::ReaderFinished));
r.reset(); assert_eq!(r.finish(),read_csv("",c)); r.reset(); r.feed("h\nx").unwrap(); assert_eq!(r.finish(),read_csv("h\nx",c));
r.reset(); r.feed("discard").unwrap(); r.reset(); r.feed("'\0").unwrap(); assert!(r.finish().is_err());
r.reset(); r.feed("new\nvalue").unwrap(); assert_eq!(r.finish(),read_csv("new\nvalue",c));
}

// R5, R24, R29
#[test]
fn h_headers_empty_and_ragged() {
for headers in [false,true] { let c=cfg(headers,DuplicatePolicy::KeepFirst); let t=read_csv("",c).unwrap(); assert_eq!(t.header,None); assert!(t.rows.is_empty()); assert_eq!(t.duplicates,DuplicatePolicy::KeepFirst); }
let t=read_csv("\na\n",cfg(true,DuplicatePolicy::Reject)).unwrap(); assert_eq!(t.header,Some(strings(&[""]))); assert_eq!(t.rows,vec![strings(&["a"])]);
let t=read_csv("a,b,c\nx\n\n",cfg(false,DuplicatePolicy::Reject)).unwrap(); assert_eq!(t.header,None); assert_eq!(t.rows,vec![strings(&["a","b","c"]),strings(&["x"]),strings(&[""])]);
}

// R25, R27
#[test]
fn h_duplicate_earliest_exact() {
assert_eq!(read_csv("z,q,q,z\nshort",cfg(true,DuplicatePolicy::Reject)),Err(CsvError::DuplicateHeader{name:"q".into(),first:1,second:2}));
assert_eq!(read_csv(",\"\",A,a, A",cfg(true,DuplicatePolicy::Reject)),Err(CsvError::DuplicateHeader{name:"".into(),first:0,second:1}));
assert!(read_csv("A,a, A\n1,2,3",cfg(true,DuplicatePolicy::Reject)).is_ok());
}

// R25, R28, R29
#[test]
fn h_duplicate_keep_and_lookup() {
for (policy,expected) in [(DuplicatePolicy::KeepFirst,0),(DuplicatePolicy::KeepLast,2)] {
let t=read_csv("x,y,x\nL,M,R",cfg(true,policy)).unwrap(); assert_eq!(t.header,Some(strings(&["x","y","x"]))); assert_eq!(t.rows,vec![strings(&["L","M","R"])]); assert_eq!(t.column_index("x"),Some(expected)); assert_eq!(t.column_index("X"),None); }
let t=table(Some(&["k","k"]),&[],DuplicatePolicy::Reject); assert_eq!(t.column_index("k"),Some(0));
assert_eq!(table(None,&[],DuplicatePolicy::KeepLast).column_index("k"),None);
}

// R26, R27
#[test]
fn h_width_record_number() {
assert_eq!(read_csv("a,b\n1,2\r\n\"x\ny\"",cfg(true,DuplicatePolicy::KeepFirst)),Err(CsvError::HeaderWidth{record:2,expected:2,actual:1}));
assert_eq!(read_csv("a\nx,y\n",cfg(true,DuplicatePolicy::Reject)),Err(CsvError::HeaderWidth{record:1,expected:1,actual:2}));
}

// R14, R22, R27
#[test]
fn h_lexical_over_header() {
assert_eq!(read_csv("a,a\nshort\nv\"",cfg(true,DuplicatePolicy::Reject)),Err(CsvError::UnexpectedQuote{record:2,byte:11}));
assert_eq!(read_csv("a,b\nshort\n\"oops",cfg(true,DuplicatePolicy::Reject)),Err(CsvError::UnclosedQuote{record:2,byte:10}));
}

// R21, R22, R23, R25, R26
#[test]
fn h_stream_header_errors_cached() {
let c=cfg(true,DuplicatePolicy::Reject); let mut r=StreamReader::new(c).unwrap();
r.feed("z,z\n").unwrap(); r.feed("x,y").unwrap(); let expected=read_csv("z,z\nx,y",c); assert_eq!(r.finish(),expected); assert_eq!(r.finish(),expected); assert_eq!(r.feed("\n"),Err(CsvError::ReaderFinished));
r.reset(); r.feed("a,b\n").unwrap(); r.feed("x").unwrap(); assert_eq!(r.finish(),Err(CsvError::HeaderWidth{record:1,expected:2,actual:1}));
}

// R20, R24, R25, R28
#[test]
fn h_header_decoding_lookup() {
let c=Config{delimiter:b'|',quote:b'~',headers:true,duplicates:DuplicatePolicy::KeepLast};
let t=read_csv("~x|y~|~a~~b~|\n1|2|3",c).unwrap(); assert_eq!(t.column_index("x|y"),Some(0)); assert_eq!(t.column_index("a~b"),Some(1)); assert_eq!(t.column_index(""),Some(2)); assert_eq!(t.column_index(" x|y"),None);
}

// R4, R21, R23, R24
#[test]
fn h_stream_empty_chunks() {
let c=cfg(true,DuplicatePolicy::KeepFirst); let mut r=StreamReader::new(c).unwrap(); for _ in 0..100{r.feed("").unwrap();} r.feed("\u{feff}").unwrap(); r.feed("").unwrap(); assert_eq!(r.finish(),read_csv("",c)); r.reset(); r.feed("h").unwrap(); assert_eq!(r.finish().unwrap().header,Some(strings(&["h"])));
}

// R3, R8, R11, R12, R20, R21
#[test]
fn h_stream_dialect_errors() {
let c=Config{delimiter:b';',quote:b'\'',headers:false,duplicates:DuplicatePolicy::Reject};
for (input,error) in [("é'x",CsvError::UnexpectedQuote{record:0,byte:2}),("'x'!",CsvError::TrailingByte{record:0,byte:3}),("v;'x",CsvError::UnclosedQuote{record:0,byte:2})] {
let mut r=StreamReader::new(c).unwrap(); for ch in input.chars(){r.feed(&ch.to_string()).unwrap();} assert_eq!(r.finish(),Err(error)); }
}

// R19
#[test]
fn h_quote_range_validation() {
for quote in [0,b' ',b'\r',127,200] { assert_eq!(read_csv("",Config{quote,..cfg(false,DuplicatePolicy::Reject)}),Err(CsvError::InvalidConfig{component:ConfigPart::Quote})); }
assert!(read_csv("",Config{delimiter:b'~',quote:b'!',..cfg(false,DuplicatePolicy::Reject)}).is_ok());
}

// R17, R21, R24, R26, R29
#[test]
fn h_many_stream_records() {
let c=cfg(true,DuplicatePolicy::KeepLast); let mut r=StreamReader::new(c).unwrap(); r.feed("id,id\r").unwrap(); r.feed("\n").unwrap(); for i in 0..211 {r.feed(&format!("{i},\"λ\"\n")).unwrap();} let t=r.finish().unwrap(); assert_eq!(t.rows.len(),211); assert_eq!(t.column_index("id"),Some(1)); assert_eq!(t.rows[210],strings(&["210","λ"]));
}
