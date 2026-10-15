// R1,R2,R42,R43
#[test]
fn hidden_empty() {
let b=""; let l=""; let r="";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"".into(),conflicts:0,regions:vec![]});
}

// R13–R16,R26,R29
#[test]
fn hidden_one_side() {
let b=""; let l="new\n"; let r="";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"new\n".into(),conflicts:0,regions:vec![]});
}

// R27
#[test]
fn hidden_identical_change() {
let b="old\n"; let l="new\n"; let r="new\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"new\n".into(),conflicts:0,regions:vec![]});
}

// R18,R21,R33–R43
#[test]
fn hidden_parallel_inserts() {
let b=""; let l="left\n"; let r="right\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"<<<<<<< LEFT 1\nleft\n||||||| BASE 0,0\n=======\nright\n>>>>>>> RIGHT 1\n".into(),conflicts:1,regions:vec![Conflict{ordinal:1,base_start:0,base_len:0,output_start:0,output_len:6}]});
}

// R18,R21,R35,R39
#[test]
fn hidden_insert_end() {
let b="a\n"; let l="a\nL\n"; let r="a\nR\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"a\n<<<<<<< LEFT 1\nL\n||||||| BASE 1,0\n=======\nR\n>>>>>>> RIGHT 1\n".into(),conflicts:1,regions:vec![Conflict{ordinal:1,base_start:1,base_len:0,output_start:1,output_len:6}]});
}

// R17,R22,R23
#[test]
fn hidden_adjacent_ranges() {
let b="a\nb\n"; let l="A\nb\n"; let r="a\nB\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"A\nB\n".into(),conflicts:0,regions:vec![]});
}

// R19,R22,R23
#[test]
fn hidden_insert_range_start() {
let b="a\nb\n"; let l="L\na\nb\n"; let r="A\nb\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"L\nA\nb\n".into(),conflicts:0,regions:vec![]});
}

// R19,R22,R23
#[test]
fn hidden_insert_range_end() {
let b="a\nb\n"; let l="a\nL\nb\n"; let r="A\nb\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"A\nL\nb\n".into(),conflicts:0,regions:vec![]});
}

// R19–R21,R24–R26,R30
#[test]
fn hidden_insert_inside() {
let b="a\nb\nc\n"; let l="a\nL\nb\nc\n"; let r="R\nc\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"<<<<<<< LEFT 1\na\nL\nb\n||||||| BASE 1,2\na\nb\n=======\nR\n>>>>>>> RIGHT 1\nc\n".into(),conflicts:1,regions:vec![Conflict{ordinal:1,base_start:0,base_len:2,output_start:0,output_len:10}]});
}

// R14,R15,R17,R30,R37
#[test]
fn hidden_delete_replace() {
let b="a\nb\n"; let l=""; let r="R\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"<<<<<<< LEFT 1\n||||||| BASE 1,2\na\nb\n=======\nR\n>>>>>>> RIGHT 1\n".into(),conflicts:1,regions:vec![Conflict{ordinal:1,base_start:0,base_len:2,output_start:0,output_len:7}]});
}

// R27,R43
#[test]
fn hidden_both_delete() {
let b="a\nb\n"; let l=""; let r="";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"".into(),conflicts:0,regions:vec![]});
}

// R16,R22,R23,R34–R42
#[test]
fn hidden_two_conflicts() {
let b="a\nkeep\nb\n"; let l="A\nkeep\nB\n"; let r="X\nkeep\nY\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"<<<<<<< LEFT 1\nA\n||||||| BASE 1,1\na\n=======\nX\n>>>>>>> RIGHT 1\nkeep\n<<<<<<< LEFT 2\nB\n||||||| BASE 3,1\nb\n=======\nY\n>>>>>>> RIGHT 2\n".into(),conflicts:2,regions:vec![Conflict{ordinal:1,base_start:0,base_len:1,output_start:0,output_len:7},Conflict{ordinal:2,base_start:2,base_len:1,output_start:8,output_len:7}]});
}

// R7,R39–R41
#[test]
fn hidden_unicode_prefix() {
let b="猫\na\n"; let l="猫\né\n"; let r="猫\né\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"猫\n<<<<<<< LEFT 1\né\n||||||| BASE 2,1\na\n=======\né\n>>>>>>> RIGHT 1\n".into(),conflicts:1,regions:vec![Conflict{ordinal:1,base_start:1,base_len:1,output_start:1,output_len:7}]});
}

// R7,R38
#[test]
fn hidden_marker_payload() {
let b="x\n"; let l="<<<<<<< LEFT 9\n"; let r="=======\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"<<<<<<< LEFT 1\n<<<<<<< LEFT 9\n||||||| BASE 1,1\nx\n=======\n=======\n>>>>>>> RIGHT 1\n".into(),conflicts:1,regions:vec![Conflict{ordinal:1,base_start:0,base_len:1,output_start:0,output_len:7}]});
}

// R2,R35,R37,R40
#[test]
fn hidden_blank() {
let b="\n"; let l="\nL\n"; let r="\nR\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"\n<<<<<<< LEFT 1\nL\n||||||| BASE 1,0\n=======\nR\n>>>>>>> RIGHT 1\n".into(),conflicts:1,regions:vec![Conflict{ordinal:1,base_start:1,base_len:0,output_start:1,output_len:6}]});
}

// R16,R20,R24–R26,R30
#[test]
fn hidden_transitive() {
let b="a\nb\nc\nd\ne\n"; let l="A\nb\nC\nd\ne\n"; let r="R\nd\ne\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"<<<<<<< LEFT 1\nA\nb\nC\n||||||| BASE 1,3\na\nb\nc\n=======\nR\n>>>>>>> RIGHT 1\nd\ne\n".into(),conflicts:1,regions:vec![Conflict{ordinal:1,base_start:0,base_len:3,output_start:0,output_len:11}]});
}

// R17,R20,R24–R26
#[test]
fn hidden_wide_overlap() {
let b="a\nb\nc\nd\ne\nf\n"; let l="a\nL\nf\n"; let r="a\nb\nR\ne\nf\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"a\n<<<<<<< LEFT 1\nL\n||||||| BASE 2,4\nb\nc\nd\ne\n=======\nb\nR\ne\n>>>>>>> RIGHT 1\nf\n".into(),conflicts:1,regions:vec![Conflict{ordinal:1,base_start:1,base_len:4,output_start:1,output_len:12}]});
}

// R8–R16,R27–R30
#[test]
fn hidden_lcs_crossed() {
let b="a\nb\na\n"; let l="b\na\nb\n"; let r="a\na\nb\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"a\nb\n".into(),conflicts:0,regions:vec![]});
}

// R10–R12,R17
#[test]
fn hidden_odd_parity() {
let b="p\na\nb\n"; let l="p\nx\nb\n"; let r="p\na\ny\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"p\nx\ny\n".into(),conflicts:0,regions:vec![]});
}

// R31,R32
#[test]
fn hidden_prefer_right() {
let b="a\nk\nb\n"; let l="L\nk\nb\n"; let r="R\nk\nB\n";
let result=merge_three_way(b,l,r,Resolution::PreferRight).unwrap();
assert_eq!(result,MergeResult{text:"R\nk\nB\n".into(),conflicts:0,regions:vec![]});
}

// R31,R32
#[test]
fn hidden_prefer_left_clean() {
let b="a\nk\nb\n"; let l="L\nk\nB\n"; let r="R\nk\nb\n";
let result=merge_three_way(b,l,r,Resolution::PreferLeft).unwrap();
assert_eq!(result,MergeResult{text:"L\nk\nB\n".into(),conflicts:0,regions:vec![]});
}

// R20,R24–R26,R30
#[test]
fn hidden_delete_gap() {
let b="a\nb\nc\nd\n"; let l="L\nc\nd\n"; let r="a\nR\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"<<<<<<< LEFT 1\nL\nc\nd\n||||||| BASE 1,4\na\nb\nc\nd\n=======\na\nR\n>>>>>>> RIGHT 1\n".into(),conflicts:1,regions:vec![Conflict{ordinal:1,base_start:0,base_len:4,output_start:0,output_len:13}]});
}

// R3,R6
#[test]
fn hidden_cr_first() {
assert_eq!(merge_three_way("é\r\n","\r","\r",Resolution::PreferRight),Err(MergeError::CarriageReturn{side:MergeSide::Base,byte:2}));
}

// R3,R6
#[test]
fn hidden_cr_before_lf() {
assert_eq!(merge_three_way("bad","ok\n","猫\r",Resolution::PreferRight),Err(MergeError::CarriageReturn{side:MergeSide::Right,byte:3}));
}

// R3,R6
#[test]
fn hidden_left_cr() {
assert_eq!(merge_three_way("","x\r\n","z\r\n",Resolution::PreferRight),Err(MergeError::CarriageReturn{side:MergeSide::Left,byte:1}));
}

// R4,R6
#[test]
fn hidden_base_lf() {
assert_eq!(merge_three_way("b","l","r",Resolution::PreferRight),Err(MergeError::MissingFinalLf{side:MergeSide::Base}));
}

// R4,R6
#[test]
fn hidden_left_lf() {
assert_eq!(merge_three_way("","l","r",Resolution::PreferRight),Err(MergeError::MissingFinalLf{side:MergeSide::Left}));
}

// R4,R6
#[test]
fn hidden_right_lf() {
assert_eq!(merge_three_way("","","r",Resolution::PreferRight),Err(MergeError::MissingFinalLf{side:MergeSide::Right}));
}

// R5,R6,R44
#[test]
fn hidden_input_limits() {
let a="x\n".repeat(128); let b="x\n".repeat(129);
assert_eq!(merge_three_way(&a,&a,&a,Resolution::Mark).unwrap().text,a);
assert_eq!(merge_three_way(&a,&b,&a,Resolution::Mark),Err(MergeError::TooLarge{base_len:128,left_len:129,right_len:128}));
assert_eq!(merge_three_way(&b,&a,"bad",Resolution::PreferLeft),Err(MergeError::MissingFinalLf{side:MergeSide::Right}));
assert_eq!(merge_three_way(&b,&a,&a,Resolution::Mark),Err(MergeError::TooLarge{base_len:129,left_len:128,right_len:128}));
assert_eq!(merge_three_way(&a,&a,&b,Resolution::Mark),Err(MergeError::TooLarge{base_len:128,left_len:128,right_len:129}));
assert_eq!(a,"x\n".repeat(128));
}

// R5,R33–R43
#[test]
fn hidden_large_output() {
let b="base\n".repeat(128);let l="left\n".repeat(128);let r="right\n".repeat(128);
let m=merge_three_way(&b,&l,&r,Resolution::Mark).unwrap();
assert_eq!(m.conflicts,1);assert_eq!(m.text.lines().count(),388);
assert_eq!(m.regions,vec![Conflict{ordinal:1,base_start:0,base_len:128,output_start:0,output_len:388}]);
}

// R7–R16,R23–R29,R43,R44
#[test]
fn hidden_base_identity_exhaustive() {
let inputs=["","a\n","b\n","a\nb\n","b\na\n","a\na\n","\n","\0\n"];
for b in inputs {for other in inputs {
    for mode in [Resolution::Mark,Resolution::PreferLeft,Resolution::PreferRight] {
        assert_eq!(merge_three_way(b,b,other,mode.clone()).unwrap(),MergeResult{text:other.into(),conflicts:0,regions:vec![]});
        assert_eq!(merge_three_way(b,other,b,mode).unwrap(),MergeResult{text:other.into(),conflicts:0,regions:vec![]});
    }
}}
}

// R7,R23–R30,R37,R43
#[test]
fn hidden_exact_whitespace() {
let b="a \n\t\nA\n"; let l="a\n\t\nA\n"; let r=" a\n\t\nA\n";
let result=merge_three_way(b,l,r,Resolution::Mark).unwrap();
assert_eq!(result,MergeResult{text:"<<<<<<< LEFT 1\na\n||||||| BASE 1,1\na \n=======\n a\n>>>>>>> RIGHT 1\n\t\nA\n".into(),conflicts:1,regions:vec![Conflict{ordinal:1,base_start:0,base_len:1,output_start:0,output_len:7}]});
}
