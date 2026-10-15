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

// R21, R22
#[test]
fn public_stream_chunks() {
let c=cfg(false,DuplicatePolicy::KeepFirst); let mut reader=StreamReader::new(c).unwrap(); reader.feed("left,").unwrap(); reader.feed("\"right\"\r").unwrap(); reader.feed("\n").unwrap(); assert_eq!(reader.finish().unwrap().rows,vec![strings(&["left","right"])]); reader.reset(); reader.feed("again,done").unwrap(); assert_eq!(reader.finish().unwrap().rows,vec![strings(&["again","done"])]);
}

// R29, R30, R31, R33, R34, R35
#[test]
fn public_named_typed() {
let t=read_csv("id,cost,ok\n12,4.50,yes",cfg(true,DuplicatePolicy::Reject)).unwrap(); assert_eq!(t.column_index("cost"),Some(1)); let specs=[cs(ColumnRef::Name("id".into()),ColumnType::Integer,false),ix(1,ColumnType::Decimal,false),ix(2,ColumnType::Boolean,false)]; let r=extract_columns(&t,&specs,ErrorPolicy::Collect); assert_eq!(r.rows,vec![vec![Some(Value::Integer(12)),Some(Value::Decimal(450)),Some(Value::Boolean(true))]]); assert!(r.errors.is_empty());
}

// R32, R37
#[test]
fn public_nullable_text() {
let t=table(None,&[&["","hello"]],DuplicatePolicy::Reject); let r=extract_columns(&t,&[ix(0,ColumnType::Integer,true),ix(1,ColumnType::Text,false)],ErrorPolicy::Collect); assert_eq!(r.rows,vec![vec![Some(Value::Null),Some(Value::Text("hello".into()))]]); assert!(r.errors.is_empty());
}

// R18, R38, R39
#[test]
fn public_successful_policies() {
assert_eq!(parse_csv("x").unwrap(),vec![strings(&["x"])]); let t=table(None,&[&["21"]],DuplicatePolicy::Reject); for policy in [ErrorPolicy::FirstPerRow,ErrorPolicy::Stop]{let r=extract_columns(&t,&[ix(0,ColumnType::Integer,false)],policy); assert_eq!(r.rows,vec![vec![Some(Value::Integer(21))]]); assert!(r.errors.is_empty());}
}
