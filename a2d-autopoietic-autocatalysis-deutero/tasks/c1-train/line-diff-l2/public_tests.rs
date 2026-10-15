// R9–R12,R14,R20–R24
#[test]
fn public_replacement() {
assert_eq!(unified_diff("x\n","y\n",0),Ok("--- before\n+++ after\n@@ -1,1 +1,1 @@\n-x\n+y\n".into()));
}

// R15–R24
#[test]
fn public_context() {
assert_eq!(unified_diff("a\nb\nc\n","a\nB\nc\n",1),Ok("--- before\n+++ after\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n".into()));
}

// R13
#[test]
fn public_unchanged() {
assert_eq!(unified_diff("same\n","same\n",2),Ok("".into()));
}
