// R1,R2,R15,R17
#[test]
fn hidden_zero() {
let a=vec![]; let b=vec![]; assert_eq!(diff_lines(&a,&b),Ok(vec![]));
}

// R2,R12,R16
#[test]
fn hidden_blank() {
let a=vec!["".to_string()]; let b=vec!["".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Keep {old_index:0, new_index:0, line:"".into()}]));
}

// R9,R15,R16
#[test]
fn hidden_delete_all() {
let a=vec!["a".to_string(),"".to_string(),"b".to_string()]; let b=vec![]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Delete {old_index:0, line:"a".into()},Edit::Delete {old_index:1, line:"".into()},Edit::Delete {old_index:2, line:"b".into()}]));
}

// R9,R15,R16
#[test]
fn hidden_insert_all() {
let a=vec![]; let b=vec!["".to_string(),"x".to_string(),"y".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Insert {new_index:0, line:"".into()},Edit::Insert {new_index:1, line:"x".into()},Edit::Insert {new_index:2, line:"y".into()}]));
}

// R12–R14
#[test]
fn hidden_odd_tie() {
let a=vec!["q".to_string(),"a".to_string()]; let b=vec!["q".to_string(),"b".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Keep {old_index:0, new_index:0, line:"q".into()},Edit::Delete {old_index:1, line:"a".into()},Edit::Insert {new_index:1, line:"b".into()}]));
}

// R10–R14
#[test]
fn hidden_crossed() {
let a=vec!["a".to_string(),"b".to_string()]; let b=vec!["b".to_string(),"a".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Delete {old_index:0, line:"a".into()},Edit::Keep {old_index:1, new_index:0, line:"b".into()},Edit::Insert {new_index:1, line:"a".into()}]));
}

// R8–R17
#[test]
fn hidden_duplicates() {
let a=vec!["x".to_string(),"x".to_string(),"y".to_string()]; let b=vec!["x".to_string(),"y".to_string(),"y".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Keep {old_index:0, new_index:0, line:"x".into()},Edit::Delete {old_index:1, line:"x".into()},Edit::Keep {old_index:2, new_index:1, line:"y".into()},Edit::Insert {new_index:2, line:"y".into()}]));
}

// R8–R17
#[test]
fn hidden_forced_delete() {
let a=vec!["a".to_string(),"b".to_string(),"c".to_string()]; let b=vec!["b".to_string(),"c".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Delete {old_index:0, line:"a".into()},Edit::Keep {old_index:1, new_index:0, line:"b".into()},Edit::Keep {old_index:2, new_index:1, line:"c".into()}]));
}

// R8–R17
#[test]
fn hidden_forced_insert() {
let a=vec!["b".to_string(),"c".to_string()]; let b=vec!["a".to_string(),"b".to_string(),"c".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Insert {new_index:0, line:"a".into()},Edit::Keep {old_index:0, new_index:1, line:"b".into()},Edit::Keep {old_index:1, new_index:2, line:"c".into()}]));
}

// R8,R10–R16
#[test]
fn hidden_unicode() {
let a=vec!["é".to_string(),"é".to_string(),"猫".to_string()]; let b=vec!["é".to_string(),"é".to_string(),"猫".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Delete {old_index:0, line:"é".into()},Edit::Keep {old_index:1, new_index:0, line:"é".into()},Edit::Insert {new_index:1, line:"é".into()},Edit::Keep {old_index:2, new_index:2, line:"猫".into()}]));
}

// R5,R8
#[test]
fn hidden_whitespace() {
let a=vec![" a".to_string(),"a ".to_string(),"A".to_string()]; let b=vec!["a".to_string(),"A".to_string(),"\t".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Delete {old_index:0, line:" a".into()},Edit::Insert {new_index:0, line:"a".into()},Edit::Delete {old_index:1, line:"a ".into()},Edit::Keep {old_index:2, new_index:1, line:"A".into()},Edit::Insert {new_index:2, line:"\t".into()}]));
}

// R5,R8
#[test]
fn hidden_nul() {
let a=vec!["\u{0000}".to_string(),"\t".to_string()]; let b=vec!["\u{0000}".to_string(),"\t".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Keep {old_index:0, new_index:0, line:"\u{0000}".into()},Edit::Keep {old_index:1, new_index:1, line:"\t".into()}]));
}

// R8–R17
#[test]
fn hidden_alternating() {
let a=vec!["a".to_string(),"b".to_string(),"a".to_string(),"b".to_string()]; let b=vec!["b".to_string(),"a".to_string(),"b".to_string(),"a".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Delete {old_index:0, line:"a".into()},Edit::Keep {old_index:1, new_index:0, line:"b".into()},Edit::Keep {old_index:2, new_index:1, line:"a".into()},Edit::Keep {old_index:3, new_index:2, line:"b".into()},Edit::Insert {new_index:3, line:"a".into()}]));
}

// R8–R17
#[test]
fn hidden_late_ties() {
let a=vec!["p".to_string(),"a".to_string(),"c".to_string()]; let b=vec!["p".to_string(),"b".to_string(),"d".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Keep {old_index:0, new_index:0, line:"p".into()},Edit::Delete {old_index:1, line:"a".into()},Edit::Insert {new_index:1, line:"b".into()},Edit::Delete {old_index:2, line:"c".into()},Edit::Insert {new_index:2, line:"d".into()}]));
}

// R8–R17
#[test]
fn hidden_non_greedy() {
let a=vec!["z".to_string(),"a".to_string(),"b".to_string(),"c".to_string()]; let b=vec!["a".to_string(),"b".to_string(),"z".to_string(),"c".to_string()]; assert_eq!(diff_lines(&a,&b),Ok(vec![Edit::Delete {old_index:0, line:"z".into()},Edit::Keep {old_index:1, new_index:0, line:"a".into()},Edit::Keep {old_index:2, new_index:1, line:"b".into()},Edit::Insert {new_index:2, line:"z".into()},Edit::Keep {old_index:3, new_index:3, line:"c".into()}]));
}

// R5–R7
#[test]
fn hidden_old_lf() {
assert_eq!(diff_lines(&vec!["ok".to_string(),"bad\n".to_string()],&vec![]),Err(DiffError::EmbeddedNewline{side:Side::Old,index:1}));
}

// R5–R7
#[test]
fn hidden_new_cr() {
assert_eq!(diff_lines(&vec![],&vec!["good".to_string(),"\r".to_string()]),Err(DiffError::EmbeddedNewline{side:Side::New,index:1}));
}

// R7
#[test]
fn hidden_old_precedence() {
assert_eq!(diff_lines(&vec!["ok".to_string(),"\r".to_string()],&vec!["\n".to_string()]),Err(DiffError::EmbeddedNewline{side:Side::Old,index:1}));
}

// R6,R7
#[test]
fn hidden_first_invalid() {
assert_eq!(diff_lines(&vec!["\n".to_string(),"\r".to_string()],&vec![]),Err(DiffError::EmbeddedNewline{side:Side::Old,index:0}));
}

// R6
#[test]
fn hidden_index_not_byte() {
assert_eq!(diff_lines(&vec![],&vec!["猫".to_string(),"ok".to_string(),"x\n".to_string()]),Err(DiffError::EmbeddedNewline{side:Side::New,index:2}));
}

// R3,R17
#[test]
fn hidden_limit_ok() {
let a=vec!["v".to_string();128]; assert_eq!(diff_lines(&a,&a).unwrap().len(),128);
}

// R3,R4
#[test]
fn hidden_old_limit() {
let a=vec!["
".to_string();129]; assert_eq!(diff_lines(&a,&[]),Err(DiffError::TooLarge{old_len:129,new_len:0}));
}

// R3,R4
#[test]
fn hidden_new_limit() {
let b=vec!["v".to_string();129]; assert_eq!(diff_lines(&[],&b),Err(DiffError::TooLarge{old_len:0,new_len:129}));
}

// R18
#[test]
fn hidden_borrowed() {
let a=vec!["x".into(),"y".into()]; let b=vec!["y".into()]; let ac=a.clone(); let bc=b.clone(); diff_lines(&a,&b).unwrap(); assert_eq!(a,ac); assert_eq!(b,bc);
}

// R9–R16
#[test]
fn hidden_large_shift() {
let a:Vec<String>=(0..100).map(|x|x.to_string()).collect(); let b=a[1..].to_vec(); let out=diff_lines(&a,&b).unwrap(); assert_eq!(out[0],Edit::Delete{old_index:0,line:"0".into()}); assert_eq!(out.len(),100); assert_eq!(out[99],Edit::Keep{old_index:99,new_index:98,line:"99".into()});
}
