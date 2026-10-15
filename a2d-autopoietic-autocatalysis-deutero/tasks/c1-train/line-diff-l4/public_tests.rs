// R1,R2,R23,R42,R43
#[test]
fn public_unchanged() {
let b="a\n"; let l="a\n"; let r="a\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"a\n".into(),conflicts:0,regions:vec![]});
}

// R8–R17,R23–R29
#[test]
fn public_disjoint() {
let b="a\nb\nc\n"; let l="A\nb\nc\n"; let r="a\nb\nC\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"A\nb\nC\n".into(),conflicts:0,regions:vec![]});
}

// R30,R33–R43
#[test]
fn public_conflict() {
let b="x\n"; let l="left\n"; let r="right\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"<<<<<<< LEFT 1\nleft\n||||||| BASE 1,1\nx\n=======\nright\n>>>>>>> RIGHT 1\n".into(),conflicts:1,regions:vec![Conflict{ordinal:1,base_start:0,base_len:1,output_start:0,output_len:7}]});
}

// R31,R32,R42
#[test]
fn public_preferred() {
let b="x\n"; let l="l\n"; let r="r\n";
let result=merge_three_way(b,l,r,Resolution::PreferLeft).unwrap();
assert_eq!(result,MergeResult{text:"l\n".into(),conflicts:0,regions:vec![]});
}
