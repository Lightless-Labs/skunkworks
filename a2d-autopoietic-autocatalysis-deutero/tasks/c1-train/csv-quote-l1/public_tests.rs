
// R2, R5, R6
#[test]
fn public_basic_fields() {
assert_eq!(parse_csv("name,age\nAda,36\n"),Ok(vec![vec!["name".to_string(),"age".to_string()],vec!["Ada".to_string(),"36".to_string()]]));
}

// R9, R10, R16
#[test]
fn public_escaped_and_quoted() {
assert_eq!(parse_csv("\"red,blue\",\"say \"\"hi\"\"\""),Ok(vec![vec!["red,blue".to_string(),"say \"hi\"".to_string()]]));
}

// R5, R7, R10
#[test]
fn public_record_endings() {
assert_eq!(parse_csv("u,v\r\n\"two\nlines\",w"),Ok(vec![vec!["u".to_string(),"v".to_string()],vec!["two\nlines".to_string(),"w".to_string()]]));
}

// R2, R6, R16
#[test]
fn public_unicode_empty() {
assert_eq!(parse_csv("雪,,\"\""),Ok(vec![vec!["雪".to_string(),"".to_string(),"".to_string()]]));
}
