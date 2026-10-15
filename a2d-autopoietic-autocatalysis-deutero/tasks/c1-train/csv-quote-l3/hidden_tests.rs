fn cfg(headers: bool, duplicates: DuplicatePolicy) -> Config {
    Config { delimiter:b',', quote:b'"', headers, duplicates }
}
fn strings(values: &[&str]) -> Vec<String> { values.iter().map(|s| s.to_string()).collect() }
fn table(header: Option<&[&str]>, rows: &[&[&str]], duplicates: DuplicatePolicy) -> Table {
    Table { header:header.map(strings), rows:rows.iter().map(|r| strings(r)).collect(), duplicates }
}
fn cs(column: ColumnRef, kind: ColumnType, nullable: bool) -> ColumnSpec { ColumnSpec{column,kind,nullable} }
fn ix(index: usize, kind: ColumnType, nullable: bool) -> ColumnSpec { cs(ColumnRef::Index(index),kind,nullable) }
fn err(row: usize, spec: usize, column: Option<usize>, kind: CellErrorKind) -> CellError { CellError{row,spec,column,kind} }

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

// R19, R20, R27, R29
#[test]
fn h_config_and_dialect() {
{
let mut c=cfg(false,DuplicatePolicy::Reject); c.delimiter=0; c.quote=255;
assert_eq!(read_csv("\0",c),Err(CsvError::InvalidConfig{component:ConfigPart::Delimiter}));
assert!(matches!(StreamReader::new(c),Err(CsvError::InvalidConfig{component:ConfigPart::Delimiter})));
c.delimiter=b',';
assert_eq!(read_csv("",c),Err(CsvError::InvalidConfig{component:ConfigPart::Quote}));
c.quote=b',';
assert_eq!(read_csv("",c),Err(CsvError::InvalidConfig{component:ConfigPart::Quote}));
for b in [b' ',b'\n',127,128,255] { c.delimiter=b; c.quote=b'"'; assert_eq!(read_csv("",c),Err(CsvError::InvalidConfig{component:ConfigPart::Delimiter})); }
}
{
let c=Config{delimiter:b'!',quote:b'~',headers:false,duplicates:DuplicatePolicy::KeepFirst};
assert_eq!(read_csv("~a!b~~c~!\"q\",z",c).unwrap().rows,vec![strings(&["a!b~c","\"q\",z"])]);
let c=Config{delimiter:b'X',quote:b'Q',..c};
assert_eq!(read_csv("QaXXbQXc",c).unwrap().rows,vec![strings(&["aXXb","c"])]);
assert_eq!(parse_csv("a;b").unwrap(),vec![strings(&["a;b"])]);
}
{
for quote in [0,b' ',b'\r',127,200] { assert_eq!(read_csv("",Config{quote,..cfg(false,DuplicatePolicy::Reject)}),Err(CsvError::InvalidConfig{component:ConfigPart::Quote})); }
assert!(read_csv("",Config{delimiter:b'~',quote:b'!',..cfg(false,DuplicatePolicy::Reject)}).is_ok());
}
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

// R3, R4, R8, R11, R12, R14, R20, R21, R22, R23, R24
#[test]
fn h_stream_lifecycle() {
{
let mut r=StreamReader::new(cfg(false,DuplicatePolicy::Reject)).unwrap();
r.feed("good\r").unwrap(); r.feed("\nλ").unwrap(); r.feed("\"bad").unwrap();
let error=Err(CsvError::UnexpectedQuote{record:1,byte:8});
assert_eq!(r.finish(),error); assert_eq!(r.finish(),error);
assert_eq!(r.feed(""),Err(CsvError::ReaderFinished));
assert_eq!(r.feed("tail"),Err(CsvError::ReaderFinished)); assert_eq!(r.finish(),error);
}
{
let c=Config{delimiter:b';',quote:b'\'',headers:true,duplicates:DuplicatePolicy::KeepLast};
let mut r=StreamReader::new(c).unwrap(); r.feed("a;b\n1;2").unwrap(); let got=r.finish();
assert_eq!(r.finish(),got); assert_eq!(r.feed("more"),Err(CsvError::ReaderFinished));
r.reset(); assert_eq!(r.finish(),read_csv("",c)); r.reset(); r.feed("h\nx").unwrap(); assert_eq!(r.finish(),read_csv("h\nx",c));
r.reset(); r.feed("discard").unwrap(); r.reset(); r.feed("'\0").unwrap(); assert!(r.finish().is_err());
r.reset(); r.feed("new\nvalue").unwrap(); assert_eq!(r.finish(),read_csv("new\nvalue",c));
}
{
let c=cfg(true,DuplicatePolicy::KeepFirst); let mut r=StreamReader::new(c).unwrap(); for _ in 0..100{r.feed("").unwrap();} r.feed("\u{feff}").unwrap(); r.feed("").unwrap(); assert_eq!(r.finish(),read_csv("",c)); r.reset(); r.feed("h").unwrap(); assert_eq!(r.finish().unwrap().header,Some(strings(&["h"])));
}
{
let c=Config{delimiter:b';',quote:b'\'',headers:false,duplicates:DuplicatePolicy::Reject};
for (input,error) in [("é'x",CsvError::UnexpectedQuote{record:0,byte:2}),("'x'!",CsvError::TrailingByte{record:0,byte:3}),("v;'x",CsvError::UnclosedQuote{record:0,byte:2})] {
let mut r=StreamReader::new(c).unwrap(); for ch in input.chars(){r.feed(&ch.to_string()).unwrap();} assert_eq!(r.finish(),Err(error)); }
}
}

// R5, R24, R29
#[test]
fn h_headers_empty_and_ragged() {
for headers in [false,true] { let c=cfg(headers,DuplicatePolicy::KeepFirst); let t=read_csv("",c).unwrap(); assert_eq!(t.header,None); assert!(t.rows.is_empty()); assert_eq!(t.duplicates,DuplicatePolicy::KeepFirst); }
let t=read_csv("\na\n",cfg(true,DuplicatePolicy::Reject)).unwrap(); assert_eq!(t.header,Some(strings(&[""]))); assert_eq!(t.rows,vec![strings(&["a"])]);
let t=read_csv("a,b,c\nx\n\n",cfg(false,DuplicatePolicy::Reject)).unwrap(); assert_eq!(t.header,None); assert_eq!(t.rows,vec![strings(&["a","b","c"]),strings(&["x"]),strings(&[""])]);
}

// R20, R24, R25, R27, R28, R29
#[test]
fn h_header_duplicate_policies() {
{
assert_eq!(read_csv("z,q,q,z\nshort",cfg(true,DuplicatePolicy::Reject)),Err(CsvError::DuplicateHeader{name:"q".into(),first:1,second:2}));
assert_eq!(read_csv(",\"\",A,a, A",cfg(true,DuplicatePolicy::Reject)),Err(CsvError::DuplicateHeader{name:"".into(),first:0,second:1}));
assert!(read_csv("A,a, A\n1,2,3",cfg(true,DuplicatePolicy::Reject)).is_ok());
}
{
for (policy,expected) in [(DuplicatePolicy::KeepFirst,0),(DuplicatePolicy::KeepLast,2)] {
let t=read_csv("x,y,x\nL,M,R",cfg(true,policy)).unwrap(); assert_eq!(t.header,Some(strings(&["x","y","x"]))); assert_eq!(t.rows,vec![strings(&["L","M","R"])]); assert_eq!(t.column_index("x"),Some(expected)); assert_eq!(t.column_index("X"),None); }
let t=table(Some(&["k","k"]),&[],DuplicatePolicy::Reject); assert_eq!(t.column_index("k"),Some(0));
assert_eq!(table(None,&[],DuplicatePolicy::KeepLast).column_index("k"),None);
}
{
let c=Config{delimiter:b'|',quote:b'~',headers:true,duplicates:DuplicatePolicy::KeepLast};
let t=read_csv("~x|y~|~a~~b~|\n1|2|3",c).unwrap(); assert_eq!(t.column_index("x|y"),Some(0)); assert_eq!(t.column_index("a~b"),Some(1)); assert_eq!(t.column_index(""),Some(2)); assert_eq!(t.column_index(" x|y"),None);
}
}

// R14, R21, R22, R23, R25, R26, R27
#[test]
fn h_header_precedence_and_cache() {
{
assert_eq!(read_csv("a,b\n1,2\r\n\"x\ny\"",cfg(true,DuplicatePolicy::KeepFirst)),Err(CsvError::HeaderWidth{record:2,expected:2,actual:1}));
assert_eq!(read_csv("a\nx,y\n",cfg(true,DuplicatePolicy::Reject)),Err(CsvError::HeaderWidth{record:1,expected:1,actual:2}));
}
{
assert_eq!(read_csv("a,a\nshort\nv\"",cfg(true,DuplicatePolicy::Reject)),Err(CsvError::UnexpectedQuote{record:2,byte:11}));
assert_eq!(read_csv("a,b\nshort\n\"oops",cfg(true,DuplicatePolicy::Reject)),Err(CsvError::UnclosedQuote{record:2,byte:10}));
}
{
let c=cfg(true,DuplicatePolicy::Reject); let mut r=StreamReader::new(c).unwrap();
r.feed("z,z\n").unwrap(); r.feed("x,y").unwrap(); let expected=read_csv("z,z\nx,y",c); assert_eq!(r.finish(),expected); assert_eq!(r.finish(),expected); assert_eq!(r.feed("\n"),Err(CsvError::ReaderFinished));
r.reset(); r.feed("a,b\n").unwrap(); r.feed("x").unwrap(); assert_eq!(r.finish(),Err(CsvError::HeaderWidth{record:1,expected:2,actual:1}));
}
}

// R17, R21, R24, R26, R29
#[test]
fn h_many_stream_records() {
let c=cfg(true,DuplicatePolicy::KeepLast); let mut r=StreamReader::new(c).unwrap(); r.feed("id,id\r").unwrap(); r.feed("\n").unwrap(); for i in 0..211 {r.feed(&format!("{i},\"λ\"\n")).unwrap();} let t=r.finish().unwrap(); assert_eq!(t.rows.len(),211); assert_eq!(t.column_index("id"),Some(1)); assert_eq!(t.rows[210],strings(&["210","λ"]));
}

// R30, R33, R36, R37
#[test]
fn h_integer_endpoints() {
let t=table(None,&[&["-9223372036854775808"],&["9223372036854775807"],&["0"],&["-17"]],DuplicatePolicy::Reject);
let r=extract_columns(&t,&[ix(0,ColumnType::Integer,false)],ErrorPolicy::Collect);
assert!(r.errors.is_empty()); assert_eq!(r.rows,vec![vec![Some(Value::Integer(i64::MIN))],vec![Some(Value::Integer(i64::MAX))],vec![Some(Value::Integer(0))],vec![Some(Value::Integer(-17))]]);
}

// R33, R36, R37
#[test]
fn h_integer_syntax_and_overflow() {
let cells=["-0","00","+7"," 9","9 ","１","1_0","-","9223372036854775808","-9223372036854775809","999999999999999999999999999x"];
let rows:Vec<Vec<String>>=cells.iter().map(|s| strings(&[s])).collect(); let t=Table{header:None,rows,duplicates:DuplicatePolicy::Reject};
let r=extract_columns(&t,&[ix(0,ColumnType::Integer,false)],ErrorPolicy::Collect);
assert_eq!(r.errors.len(),cells.len()); for (i,e) in r.errors.iter().enumerate(){ assert_eq!(*e,err(i,0,Some(0),if i==8||i==9{CellErrorKind::IntegerOverflow}else{CellErrorKind::InvalidInteger})); }
}

// R34
#[test]
fn h_decimal_endpoints() {
let t=table(None,&[&["92233720368547758.07"],&["-92233720368547758.08"],&["-0.01"],&["0.00"],&["2.09"]],DuplicatePolicy::Reject);
let r=extract_columns(&t,&[ix(0,ColumnType::Decimal,false)],ErrorPolicy::Collect); assert!(r.errors.is_empty()); assert_eq!(r.rows,vec![vec![Some(Value::Decimal(i64::MAX))],vec![Some(Value::Decimal(i64::MIN))],vec![Some(Value::Decimal(-1))],vec![Some(Value::Decimal(0))],vec![Some(Value::Decimal(209))]]);
}

// R34, R36, R37
#[test]
fn h_decimal_syntax_range() {
let cells=["-0.00","00.01",".12","1.2","1.234","+1.00","1.0x","1e2","1.00 ","1..00","92233720368547758.08","-92233720368547758.09","9999999999999999999999.0x","١.00"];
let t=Table{header:None,rows:cells.iter().map(|s| strings(&[s])).collect(),duplicates:DuplicatePolicy::Reject};
let r=extract_columns(&t,&[ix(0,ColumnType::Decimal,false)],ErrorPolicy::Collect); assert_eq!(r.errors.len(),cells.len()); for (i,e) in r.errors.iter().enumerate(){assert_eq!(*e,err(i,0,Some(0),if i==10||i==11{CellErrorKind::DecimalOverflow}else{CellErrorKind::InvalidDecimal}));}
}

// R32, R35, R37
#[test]
fn h_boolean_strict() {
let cells=["yes","no","1","0","true","false","YES","01"," yes",""];
let t=Table{header:None,rows:cells.iter().map(|s| strings(&[s])).collect(),duplicates:DuplicatePolicy::Reject};
let r=extract_columns(&t,&[ix(0,ColumnType::Boolean,false)],ErrorPolicy::Collect);
assert_eq!(&r.rows[..4],&[vec![Some(Value::Boolean(true))],vec![Some(Value::Boolean(false))],vec![Some(Value::Boolean(true))],vec![Some(Value::Boolean(false))]]);
assert_eq!(r.errors.len(),6); assert_eq!(r.errors.last(),Some(&err(9,0,Some(0),CellErrorKind::Empty)));
}

// R32, R36, R41
#[test]
fn h_nullable_text_and_empty() {
let t=table(None,&[&[""," \t","null","\0λ\n"]],DuplicatePolicy::Reject);
let specs=[ix(0,ColumnType::Text,false),ix(0,ColumnType::Integer,true),ix(0,ColumnType::Decimal,false),ix(0,ColumnType::Boolean,false),ix(1,ColumnType::Text,true),ix(2,ColumnType::Text,true),ix(3,ColumnType::Text,false)];
let before=t.clone(); let r=extract_columns(&t,&specs,ErrorPolicy::Collect); assert_eq!(t,before);
assert_eq!(r.rows[0],vec![Some(Value::Text("".into())),Some(Value::Null),None,None,Some(Value::Text(" \t".into())),Some(Value::Text("null".into())),Some(Value::Text("\0λ\n".into()))]);
assert_eq!(r.errors,vec![err(0,2,Some(0),CellErrorKind::Empty),err(0,3,Some(0),CellErrorKind::Empty)]);
}

// R30, R31, R36, R37
#[test]
fn h_unknown_missing_precedence() {
let t=table(None,&[&[""],&[]],DuplicatePolicy::Reject);
let specs=[cs(ColumnRef::Name("absent".into()),ColumnType::Integer,true),ix(8,ColumnType::Text,true),ix(0,ColumnType::Integer,true)];
let r=extract_columns(&t,&specs,ErrorPolicy::Collect);
assert_eq!(r.rows,vec![vec![None,None,Some(Value::Null)],vec![None,None,None]]);
assert_eq!(r.errors,vec![err(0,0,None,CellErrorKind::UnknownColumn),err(0,1,Some(8),CellErrorKind::MissingCell),err(1,0,None,CellErrorKind::UnknownColumn),err(1,1,Some(8),CellErrorKind::MissingCell),err(1,2,Some(0),CellErrorKind::MissingCell)]);
}

// R30, R36, R37
#[test]
fn h_collect_order_and_prefix() {
let t=table(None,&[&["x","7","no"],&["8","bad","yes"]],DuplicatePolicy::Reject);
let specs=[ix(1,ColumnType::Integer,false),ix(0,ColumnType::Integer,false),ix(2,ColumnType::Boolean,false),ix(0,ColumnType::Integer,false)];
let r=extract_columns(&t,&specs,ErrorPolicy::Collect);
assert_eq!(r.errors,vec![err(0,1,Some(0),CellErrorKind::InvalidInteger),err(0,3,Some(0),CellErrorKind::InvalidInteger),err(1,0,Some(1),CellErrorKind::InvalidInteger)]);
assert_eq!(r.rows[0],vec![Some(Value::Integer(7)),None,Some(Value::Boolean(false)),None]); assert_eq!(r.rows[1],vec![None,Some(Value::Integer(8)),Some(Value::Boolean(true)),Some(Value::Integer(8))]);
}

// R30, R38
#[test]
fn h_first_per_row_stops() {
let t=table(None,&[&["3","bad","4"],&["x","bad","5"],&["6","7","8"]],DuplicatePolicy::Reject);
let specs=[ix(0,ColumnType::Integer,false),ix(1,ColumnType::Integer,false),ix(2,ColumnType::Integer,false)];
let r=extract_columns(&t,&specs,ErrorPolicy::FirstPerRow);
assert_eq!(r.errors,vec![err(0,1,Some(1),CellErrorKind::InvalidInteger),err(1,0,Some(0),CellErrorKind::InvalidInteger)]);
assert_eq!(r.rows,vec![vec![Some(Value::Integer(3)),None,None],vec![None,None,None],vec![Some(Value::Integer(6)),Some(Value::Integer(7)),Some(Value::Integer(8))]]);
}

// R30, R39
#[test]
fn h_stop_includes_failing_row() {
let t=table(None,&[&["1","2","3"],&["4","bad","5"],&["6","7","8"]],DuplicatePolicy::Reject);
let specs=[ix(0,ColumnType::Integer,false),ix(1,ColumnType::Integer,false),ix(2,ColumnType::Integer,false)];
let r=extract_columns(&t,&specs,ErrorPolicy::Stop);
assert_eq!(r.rows,vec![vec![Some(Value::Integer(1)),Some(Value::Integer(2)),Some(Value::Integer(3))],vec![Some(Value::Integer(4)),None,None]]); assert_eq!(r.errors,vec![err(1,1,Some(1),CellErrorKind::InvalidInteger)]);
}

// R31, R39, R40
#[test]
fn h_zero_specs_and_rows() {
let t=table(Some(&["h"]),&[&["x"],&[]],DuplicatePolicy::Reject);
for policy in [ErrorPolicy::Collect,ErrorPolicy::FirstPerRow,ErrorPolicy::Stop] {let r=extract_columns(&t,&[],policy); assert_eq!(r,TypedResult{rows:vec![vec![],vec![]],errors:vec![]});
let empty=table(Some(&["h"]),&[],DuplicatePolicy::Reject); let r=extract_columns(&empty,&[cs(ColumnRef::Name("missing".into()),ColumnType::Integer,false)],policy); assert_eq!(r,TypedResult{rows:vec![],errors:vec![]});}
}

// R25, R28, R30, R31, R41
#[test]
fn h_duplicate_name_resolution() {
for (policy,value) in [(DuplicatePolicy::KeepFirst,2),(DuplicatePolicy::KeepLast,9),(DuplicatePolicy::Reject,2)] {
let t=table(Some(&["n","n"]),&[&["2","9"]],policy); let specs=[cs(ColumnRef::Name("n".into()),ColumnType::Integer,false),ix(1,ColumnType::Integer,false)]; let saved=specs.clone();
let r=extract_columns(&t,&specs,ErrorPolicy::Collect); assert_eq!(specs,saved); assert_eq!(r.rows,vec![vec![Some(Value::Integer(value)),Some(Value::Integer(9))]]); assert!(r.errors.is_empty()); }
}

// R24, R26, R30, R31, R40
#[test]
fn h_header_rows_and_ragged() {
let t=read_csv("num,flag\n-3,yes\n7,no",cfg(true,DuplicatePolicy::Reject)).unwrap();
let specs=[cs(ColumnRef::Name("flag".into()),ColumnType::Boolean,false),cs(ColumnRef::Name("num".into()),ColumnType::Integer,false)];
assert_eq!(extract_columns(&t,&specs,ErrorPolicy::Stop).rows,vec![vec![Some(Value::Boolean(true)),Some(Value::Integer(-3))],vec![Some(Value::Boolean(false)),Some(Value::Integer(7))]]);
let t=table(Some(&["a"]),&[&["1","2"],&["3"]],DuplicatePolicy::Reject); let r=extract_columns(&t,&[ix(1,ColumnType::Integer,false)],ErrorPolicy::Collect); assert_eq!(r.errors,vec![err(1,0,Some(1),CellErrorKind::MissingCell)]);
}

// R32, R33, R34, R35
#[test]
fn h_no_trim_or_null_aliases() {
let t=table(None,&[&["NULL"," null "," ","0.00","no"]],DuplicatePolicy::Reject);
let specs=[ix(0,ColumnType::Integer,true),ix(1,ColumnType::Decimal,true),ix(2,ColumnType::Boolean,true),ix(3,ColumnType::Decimal,true),ix(4,ColumnType::Boolean,true)];
let r=extract_columns(&t,&specs,ErrorPolicy::Collect); assert_eq!(r.errors.len(),3); assert_eq!(r.rows[0][3],Some(Value::Decimal(0))); assert_eq!(r.rows[0][4],Some(Value::Boolean(false)));
}

// R32, R36
#[test]
fn h_all_null_types() {
let t=table(None,&[&[""]],DuplicatePolicy::Reject); let specs=[ColumnType::Text,ColumnType::Integer,ColumnType::Decimal,ColumnType::Boolean].map(|k|ix(0,k,true));
let r=extract_columns(&t,&specs,ErrorPolicy::Collect); assert_eq!(r.rows,vec![vec![Some(Value::Null);4]]); assert!(r.errors.is_empty());
}

// R17, R30, R33, R37, R38, R39
#[test]
fn h_large_accumulation() {
let t=Table{header:None,rows:(0..160).map(|_| strings(&["bad","5","-0"])).collect(),duplicates:DuplicatePolicy::Reject};
let specs=[ix(0,ColumnType::Integer,false),ix(1,ColumnType::Integer,false),ix(2,ColumnType::Integer,false)];
let r=extract_columns(&t,&specs,ErrorPolicy::Collect); assert_eq!(r.rows.len(),160); assert_eq!(r.errors.len(),320); assert_eq!(r.errors[319],err(159,2,Some(2),CellErrorKind::InvalidInteger));
assert_eq!(extract_columns(&t,&specs,ErrorPolicy::FirstPerRow).errors.len(),160); assert_eq!(extract_columns(&t,&specs,ErrorPolicy::Stop).rows.len(),1);
}
