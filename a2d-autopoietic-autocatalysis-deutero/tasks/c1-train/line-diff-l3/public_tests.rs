// R8–R14,R22,R33,R37
#[test]
fn public_replace() {
assert_eq!(apply_unified("a\nb\n","--- before\n+++ after\n@@ -2,1 +2,1 @@\n-b\n+B\n",PatchOptions{max_offset:0,fuzz:0}),Ok("a\nB\n".into()));
}

// R10,R11,R25,R33
#[test]
fn public_insert() {
assert_eq!(apply_unified("","--- before\n+++ after\n@@ -0,0 +1,1 @@\n+first\n",PatchOptions{max_offset:0,fuzz:0}),Ok("first\n".into()));
}

// R26–R29
#[test]
fn public_offset() {
assert_eq!(apply_unified("z\na\nb\n","--- before\n+++ after\n@@ -1,1 +1,1 @@\n-a\n+A\n",PatchOptions{max_offset:1,fuzz:0}),Ok("z\nA\nb\n".into()));
}

// R4,R20,R37
#[test]
fn public_identity() {
assert_eq!(apply_unified("x\n","",PatchOptions{max_offset:0,fuzz:0}),Ok("x\n".into()));
}
