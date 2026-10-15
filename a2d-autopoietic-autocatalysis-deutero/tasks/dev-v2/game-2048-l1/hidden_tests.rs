// R1-R3: Rust 2024 compilation and separate-crate API verification apply throughout.
// R4,R7,R20
#[test]
fn row_empty() {
for s in [Side::Left,Side::Right] { assert_eq!(slide_row(&[],s).unwrap(),RowResult {cells:vec![],score:0,changed:false,merges:vec![]}); }
}

// R5,R19-R20
#[test]
fn row_single_boundary() {
for v in [0,2,2048,2147483648] { for s in [Side::Left,Side::Right] {let r=slide_row(&[v],s).unwrap(); assert_eq!(r.cells,vec![v]); assert!(!r.changed);assert_eq!(r.score,0);assert!(r.merges.is_empty());} }
}

// R4,R7
#[test]
fn row_length_limit() {
let r=slide_row(&[0;32],Side::Right).unwrap(); assert_eq!(r.cells.len(),32);assert!(!r.changed);
assert_eq!(slide_row(&[0;33],Side::Left),Err(Error::RowTooLong {len:33}));
}

// R4,R6
#[test]
fn row_length_before_tile() {
let mut x=vec![0;34];x[0]=1;assert_eq!(slide_row(&x,Side::Right),Err(Error::RowTooLong {len:34}));
}

// R5
#[test]
fn row_one_invalid() {
assert_eq!(slide_row(&[0,1,0],Side::Left),Err(Error::InvalidTile {index:1,value:1}));
}

// R5
#[test]
fn row_nonpower_invalid() {
for v in [6,7,2147483649,u32::MAX] {assert_eq!(slide_row(&[0,0,v],Side::Right),Err(Error::InvalidTile {index:2,value:v}));}
}

// R6
#[test]
fn row_invalid_scan_order() {
assert_eq!(slide_row(&[0,5,0,1],Side::Right),Err(Error::InvalidTile {index:1,value:5}));
}

// R6
#[test]
fn row_invalid_before_overflow() {
for s in [Side::Left,Side::Right] { assert_eq!(slide_row(&[2147483648,2147483648,3],s),Err(Error::InvalidTile {index:2,value:3})); }
}

// R11-R12
#[test]
fn row_overflow_left() {
assert_eq!(slide_row(&[0,2147483648,0,2147483648],Side::Left),Err(Error::MergeOverflow {first:1,second:3}));
}

// R9,R11-R12
#[test]
fn row_overflow_right() {
assert_eq!(slide_row(&[2147483648,0,2147483648,0],Side::Right),Err(Error::MergeOverflow {first:2,second:0}));
}

// R12
#[test]
fn row_first_overflow() {
let x=[2147483648;4]; assert_eq!(slide_row(&x,Side::Left),Err(Error::MergeOverflow {first:0,second:1}));assert_eq!(slide_row(&x,Side::Right),Err(Error::MergeOverflow {first:3,second:2}));
}

// R5,R10,R17
#[test]
fn row_largest_success() {
let r=slide_row(&[1073741824,1073741824],Side::Right).unwrap();assert_eq!(r.cells,vec![0,2147483648]);assert_eq!(r.score,2147483648);assert_eq!(r.merges,vec![Merge{from:[1,0],to:1,value:2147483648}]);
}

// R4,R17
#[test]
fn row_wide_score() {
let r=slide_row(&[1073741824;32],Side::Left).unwrap();assert_eq!(r.score,34359738368);assert_eq!(r.merges.len(),16);assert_eq!(r.cells,[vec![2147483648;16],vec![0;16]].concat());
}

// R10
#[test]
fn row_no_chain() {
let r=slide_row(&[4,4,8,0],Side::Left).unwrap();assert_eq!(r.cells,vec![8,8,0,0]);assert_eq!(r.score,8);assert_eq!(r.merges.len(),1);
}

// R10,R14-R17
#[test]
fn row_four_equal() {
let r=slide_row(&[8,8,8,8],Side::Left).unwrap();assert_eq!(r.cells,vec![16,16,0,0]);assert_eq!(r.score,32);assert_eq!(r.merges,vec![Merge{from:[0,1],to:0,value:16},Merge{from:[2,3],to:1,value:16}]);
}

// R9-R10,R14-R17
#[test]
fn row_right_encounter_order() {
let r=slide_row(&[16,16,4,4,2,2],Side::Right).unwrap();assert_eq!(r.cells,vec![0,0,0,32,8,4]);assert_eq!(r.score,44);assert_eq!(r.merges,vec![Merge{from:[5,4],to:5,value:4},Merge{from:[3,2],to:4,value:8},Merge{from:[1,0],to:3,value:32}]);
}

// R8,R13-R16
#[test]
fn row_gaps_and_unmerged() {
let r=slide_row(&[0,8,0,2,0,2,0],Side::Left).unwrap();assert_eq!(r.cells,vec![8,4,0,0,0,0,0]);assert_eq!(r.merges,vec![Merge{from:[3,5],to:1,value:4}]);
}

// R9-R10,R14-R15
#[test]
fn row_right_triple() {
let r=slide_row(&[4,0,4,4],Side::Right).unwrap();assert_eq!(r.cells,vec![0,0,4,8]);assert_eq!(r.merges,vec![Merge{from:[3,2],to:3,value:8}]);
}

// R8,R10
#[test]
fn row_unequal_barrier() {
let r=slide_row(&[2,0,4,0,2],Side::Left).unwrap();assert_eq!(r.cells,vec![2,4,2,0,0]);assert_eq!(r.score,0);assert!(r.merges.is_empty());
}

// R18-R19
#[test]
fn row_pure_slide() {
let r=slide_row(&[0,0,16,32],Side::Left).unwrap();assert_eq!(r.cells,vec![16,32,0,0]);assert!(r.changed);assert_eq!(r.score,0);assert!(r.merges.is_empty());
}

// R18-R19
#[test]
fn row_still_right() {
let r=slide_row(&[0,0,16,32],Side::Right).unwrap();assert!(!r.changed);assert_eq!(r.score,0);assert!(r.merges.is_empty());
}

// R13,R18-R20
#[test]
fn row_all_zero() {
let r=slide_row(&[0;11],Side::Left).unwrap();assert_eq!(r.cells,vec![0;11]);assert!(!r.changed);
}

// R3,R7
#[test]
fn row_owned_result() {
let x=vec![0,32,32];let original=x.clone();let mut r=slide_row(&x,Side::Left).unwrap();r.cells[0]=2;assert_eq!(x,original);assert_ne!(r,slide_row(&x,Side::Left).unwrap());
}

// R20
#[test]
fn row_target_not_terminal() {
let r=slide_row(&[2048,2048,4096],Side::Left).unwrap();assert_eq!(r.cells,vec![4096,4096,0]);assert_eq!(r.score,4096);
}

// R10,R18
#[test]
fn row_two_moves_can_chain() {
let a=slide_row(&[16,16,32],Side::Right).unwrap();assert_eq!(a.cells,vec![0,32,32]);let b=slide_row(&a.cells,Side::Right).unwrap();assert_eq!(b.cells,vec![0,0,64]);assert_eq!(b.score,64);
}
