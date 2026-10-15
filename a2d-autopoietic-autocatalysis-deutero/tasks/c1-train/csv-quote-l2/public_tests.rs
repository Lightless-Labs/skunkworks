fn cfg(headers: bool, duplicates: DuplicatePolicy) -> Config {
    Config { delimiter:b',', quote:b'"', headers, duplicates }
}
fn strings(values: &[&str]) -> Vec<String> { values.iter().map(|s| s.to_string()).collect() }
fn table(header: Option<&[&str]>, rows: &[&[&str]], duplicates: DuplicatePolicy) -> Table {
    Table { header:header.map(strings), rows:rows.iter().map(|r| strings(r)).collect(), duplicates }
}

// R9, R10, R16
#[test]
fn public_escaped_and_quoted() {
assert_eq!(parse_csv("\"red,blue\",\"say \"\"hi\"\"\""),Ok(vec![vec!["red,blue".to_string(),"say \"hi\"".to_string()]]));
}

// R19, R20, R24, R29
#[test]
fn public_configured_reader() {
let c=Config{delimiter:b';',quote:b'\'',headers:false,duplicates:DuplicatePolicy::Reject}; let t=read_csv("'one;two';three",c).unwrap(); assert_eq!(t.rows,vec![strings(&["one;two","three"])]); assert_eq!(t.header,None);
}

// R21, R22
#[test]
fn public_stream_chunks() {
let c=cfg(false,DuplicatePolicy::KeepFirst); let mut reader=StreamReader::new(c).unwrap(); reader.feed("left,").unwrap(); reader.feed("\"right\"\r").unwrap(); reader.feed("\n").unwrap(); assert_eq!(reader.finish().unwrap().rows,vec![strings(&["left","right"])]); reader.reset(); reader.feed("again,done").unwrap(); assert_eq!(reader.finish().unwrap().rows,vec![strings(&["again","done"])]);
}

// R24, R25, R28
#[test]
fn public_header_lookup() {
let t=read_csv("code,name\n7,Eve",cfg(true,DuplicatePolicy::KeepFirst)).unwrap(); assert_eq!(t.header,Some(strings(&["code","name"]))); assert_eq!(t.column_index("name"),Some(1)); assert_eq!(t.column_index("absent"),None);
}
