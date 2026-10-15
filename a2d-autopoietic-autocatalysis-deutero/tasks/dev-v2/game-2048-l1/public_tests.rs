// R8-R18
#[test]
fn public_left_merge() {
let r=slide_row(&[2,0,2,4],Side::Left).unwrap();
assert_eq!(r,RowResult { cells:vec![4,4,0,0],score:4,changed:true,merges:vec![Merge {from:[0,2],to:0,value:4}] });
}

// R9,R13,R18-R19
#[test]
fn public_right_slide() {
let r=slide_row(&[2,0,0],Side::Right).unwrap();
assert_eq!(r.cells,vec![0,0,2]); assert_eq!(r.score,0); assert!(r.changed); assert!(r.merges.is_empty());
}

// R5,R19-R20
#[test]
fn public_still_and_invalid() {
let r=slide_row(&[2,4,8],Side::Left).unwrap();
assert!(!r.changed); assert_eq!(r.score,0); assert!(r.merges.is_empty());
assert_eq!(slide_row(&[3],Side::Left),Err(Error::InvalidTile {index:0,value:3}));
}
