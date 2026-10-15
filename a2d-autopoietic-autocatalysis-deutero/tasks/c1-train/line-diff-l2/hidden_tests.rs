// R1,R2,R13
#[test]
fn hidden_empty() {
assert_eq!(unified_diff("","",0),Ok("".into()));
}

// R2,R14,R21–R24
#[test]
fn hidden_add_empty() {
assert_eq!(unified_diff("","\nx\n",0),Ok("--- before\n+++ after\n@@ -0,0 +1,2 @@\n+\n+x\n".into()));
}

// R2,R21–R24
#[test]
fn hidden_delete_empty() {
assert_eq!(unified_diff("\n","",0),Ok("--- before\n+++ after\n@@ -1,1 +0,0 @@\n-\n".into()));
}

// R12,R19–R24
#[test]
fn hidden_add_end() {
assert_eq!(unified_diff("q\n","q\nz\n",0),Ok("--- before\n+++ after\n@@ -1,0 +2,1 @@\n+z\n".into()));
}

// R12,R19–R24
#[test]
fn hidden_delete_end() {
assert_eq!(unified_diff("q\nz\n","q\n",0),Ok("--- before\n+++ after\n@@ -2,1 +1,0 @@\n-z\n".into()));
}

// R10,R11
#[test]
fn hidden_odd_tie() {
assert_eq!(unified_diff("p\na\n","p\nb\n",0),Ok("--- before\n+++ after\n@@ -2,1 +2,1 @@\n-a\n+b\n".into()));
}

// R9–R12
#[test]
fn hidden_crossed() {
assert_eq!(unified_diff("a\nb\n","b\na\n",0),Ok("--- before\n+++ after\n@@ -1,1 +0,0 @@\n-a\n@@ -2,0 +2,1 @@\n+a\n".into()));
}

// R15–R18
#[test]
fn hidden_touching() {
assert_eq!(unified_diff("a\nu\nv\nb\n","A\nu\nv\nB\n",1),Ok("--- before\n+++ after\n@@ -1,4 +1,4 @@\n-a\n+A\n u\n v\n-b\n+B\n".into()));
}

// R16–R19
#[test]
fn hidden_separate() {
assert_eq!(unified_diff("a\nu\nv\nw\nb\n","A\nu\nv\nw\nB\n",1),Ok("--- before\n+++ after\n@@ -1,2 +1,2 @@\n-a\n+A\n u\n@@ -4,2 +4,2 @@\n w\n-b\n+B\n".into()));
}

// R18,R19
#[test]
fn hidden_zero_context() {
assert_eq!(unified_diff("a\nu\nb\n","A\nu\nB\n",0),Ok("--- before\n+++ after\n@@ -1,1 +1,1 @@\n-a\n+A\n@@ -3,1 +3,1 @@\n-b\n+B\n".into()));
}

// R16,R17
#[test]
fn hidden_transitive() {
assert_eq!(unified_diff("a\np\nb\nq\nc\n","A\np\nB\nq\nC\n",1),Ok("--- before\n+++ after\n@@ -1,5 +1,5 @@\n-a\n+A\n p\n-b\n+B\n q\n-c\n+C\n".into()));
}

// R6,R17
#[test]
fn hidden_wide() {
assert_eq!(unified_diff("0\n1\n2\n3\n","0\n1\nZ\n3\n",8),Ok("--- before\n+++ after\n@@ -1,4 +1,4 @@\n 0\n 1\n-2\n+Z\n 3\n".into()));
}

// R2,R23
#[test]
fn hidden_blank_context() {
assert_eq!(unified_diff("\nx\n\n","\nX\n\n",1),Ok("--- before\n+++ after\n@@ -1,3 +1,3 @@\n \n-x\n+X\n \n".into()));
}

// R8,R23
#[test]
fn hidden_unicode() {
assert_eq!(unified_diff("é\n猫\na \n","é\n猫\na\n",1),Ok("--- before\n+++ after\n@@ -1,3 +1,3 @@\n-é\n+é\n 猫\n-a \n+a\n".into()));
}

// R8,R23,R24
#[test]
fn hidden_special() {
assert_eq!(unified_diff("---\n@@\n+\n\u{0000}\n","+++\n@@\n-\n\u{0000}\n",1),Ok("--- before\n+++ after\n@@ -1,4 +1,4 @@\n----\n++++\n @@\n-+\n+-\n \u{0000}\n".into()));
}

// R20,R21
#[test]
fn hidden_unequal_counts() {
assert_eq!(unified_diff("a\nb\nc\n","x\nc\n",1),Ok("--- before\n+++ after\n@@ -1,3 +1,2 @@\n-a\n+x\n-b\n c\n".into()));
}

// R9–R12
#[test]
fn hidden_repeated() {
assert_eq!(unified_diff("x\nx\ny\n","x\ny\nx\n",2),Ok("--- before\n+++ after\n@@ -1,3 +1,3 @@\n x\n-x\n y\n+x\n".into()));
}

// R6,R7
#[test]
fn hidden_context_error() {
assert_eq!(unified_diff("\r","x",9),Err(DiffError::ContextTooLarge{context:9}));
}

// R4,R7
#[test]
fn hidden_cr_old() {
assert_eq!(unified_diff("é\r\n","x\r\n",0),Err(DiffError::CarriageReturn{side:Side::Old,byte:2}));
}

// R4,R7
#[test]
fn hidden_cr_before_missing() {
assert_eq!(unified_diff("bad","猫\r",0),Err(DiffError::CarriageReturn{side:Side::New,byte:3}));
}

// R3,R7
#[test]
fn hidden_old_missing() {
assert_eq!(unified_diff("old","new",0),Err(DiffError::MissingFinalLf{side:Side::Old}));
}

// R3,R7
#[test]
fn hidden_new_missing() {
assert_eq!(unified_diff("x\n","x",0),Err(DiffError::MissingFinalLf{side:Side::New}));
}

// R7,R13
#[test]
fn hidden_identical_invalid() {
assert_eq!(unified_diff("\r\n","\r\n",0),Err(DiffError::CarriageReturn{side:Side::Old,byte:0}));
}

// R5,R25
#[test]
fn hidden_boundaries() {
let a="v
".repeat(128); assert_eq!(unified_diff(&a,&a,8),Ok(String::new())); let b="v
".repeat(129); assert_eq!(unified_diff(&a,&b,0),Err(DiffError::TooLarge{old_len:128,new_len:129})); assert_eq!(a,"v
".repeat(128)); assert_eq!(unified_diff(&b,&a,0),Err(DiffError::TooLarge{old_len:129,new_len:128}));
}

// R5,R7
#[test]
fn hidden_length_precedence() {
let a="a
".repeat(129); assert_eq!(unified_diff(&a,"b",0),Err(DiffError::MissingFinalLf{side:Side::New}));
}

// R19–R24
#[test]
fn hidden_large_headers() {
let a=(0..100).map(|i|format!("{i}
")).collect::<String>(); let b=format!("{a}end
"); assert_eq!(unified_diff(&a,&b,0),Ok("--- before
+++ after
@@ -100,0 +101,1 @@
+end
".into()));
}
