// R8–R17
#[test]
fn public_identity() {
let a=vec!["cat".to_string(),"dog".to_string()]; let b=vec!["cat".to_string(),"dog".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Keep {old_index:0, new_index:0, line:"cat".into()},Edit::Keep {old_index:1, new_index:1, line:"dog".into()}]));
}

// R8–R17
#[test]
fn public_replacement() {
let a=vec!["one".to_string()]; let b=vec!["two".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Delete {old_index:0, line:"one".into()},Edit::Insert {new_index:0, line:"two".into()}]));
}

// R8–R17
#[test]
fn public_empty_old() {
let a=vec![]; let b=vec!["a".to_string(),"b".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Insert {new_index:0, line:"a".into()},Edit::Insert {new_index:1, line:"b".into()}]));
}
