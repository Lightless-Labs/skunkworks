// R23,R24,R28,R33
#[test]
fn hidden_fuzz_both() {
assert_eq!(apply_unified("X\nold\nY\n","--- before\n+++ after\n@@ -1,3 +1,3 @@\n a\n-old\n+new\n b\n",PatchOptions{max_offset:0,fuzz:1}),Ok("X\nnew\nY\n".into()));
}

// R23,R30
#[test]
fn hidden_fuzz_not_delete() {
assert_eq!(apply_unified("a\nx\nb\n","--- before\n+++ after\n@@ -1,3 +1,3 @@\n a\n-old\n+new\n b\n",PatchOptions{max_offset:0,fuzz:2}),Err(ApplyError::NoMatch{hunk:1}));
}

// R24,R25
#[test]
fn hidden_fuzz_missing_context() {
assert_eq!(apply_unified("old\n","--- before\n+++ after\n@@ -1,2 +1,2 @@\n a\n-old\n+new\n",PatchOptions{max_offset:2,fuzz:1}),Err(ApplyError::NoMatch{hunk:1}));
}

// R3,R27,R29,R31
#[test]
fn hidden_ambiguous() {
assert_eq!(apply_unified("hit\ngap\nhit\n","--- before\n+++ after\n@@ -2,1 +2,1 @@\n-hit\n+new\n",PatchOptions{max_offset:1,fuzz:0}),Err(ApplyError::Ambiguous{hunk:1,first:0,second:2}));
}

// R29
#[test]
fn hidden_nearest() {
assert_eq!(apply_unified("hit\ngap\nhit\nhit\n","--- before\n+++ after\n@@ -3,1 +3,1 @@\n-hit\n+new\n",PatchOptions{max_offset:3,fuzz:0}),Ok("hit\ngap\nnew\nhit\n".into()));
}

// R23,R29
#[test]
fn hidden_exact_beats_fuzz() {
assert_eq!(apply_unified("X\nold\na\nold\n","--- before\n+++ after\n@@ -1,2 +1,2 @@\n a\n-old\n+new\n",PatchOptions{max_offset:2,fuzz:1}),Ok("X\nold\na\nnew\n".into()));
}

// R23,R24,R32,R33
#[test]
fn hidden_drop_two() {
assert_eq!(apply_unified("X\nY\nold\nb\nb\n","--- before\n+++ after\n@@ -1,5 +1,5 @@\n a\n a\n-old\n+new\n b\n b\n",PatchOptions{max_offset:0,fuzz:2}),Ok("X\nY\nnew\nb\nb\n".into()));
}

// R26,R35
#[test]
fn hidden_drift_carried() {
assert_eq!(apply_unified("extra\na\nb\nextra2\nc\n","--- before\n+++ after\n@@ -1,1 +1,1 @@\n-a\n+A\n@@ -3,1 +3,1 @@\n-c\n+C\n",PatchOptions{max_offset:1,fuzz:0}),Ok("extra\nA\nb\nextra2\nC\n".into()));
}

// R17,R26,R35
#[test]
fn hidden_net_carried() {
assert_eq!(apply_unified("a\nb\nc\n","--- before\n+++ after\n@@ -1,1 +1,2 @@\n-a\n+A\n+AA\n@@ -3,1 +4,1 @@\n-c\n+C\n",PatchOptions{max_offset:0,fuzz:0}),Ok("A\nAA\nb\nC\n".into()));
}

// R16,R17,R25,R35
#[test]
fn hidden_same_anchor() {
assert_eq!(apply_unified("x\n","--- before\n+++ after\n@@ -0,0 +1,1 @@\n+a\n@@ -0,0 +2,1 @@\n+b\n",PatchOptions{max_offset:0,fuzz:0}),Ok("a\nb\nx\n".into()));
}

// R10,R33,R37
#[test]
fn hidden_delete_all() {
assert_eq!(apply_unified("a\n","--- before\n+++ after\n@@ -1,1 +0,0 @@\n-a\n",PatchOptions{max_offset:0,fuzz:0}),Ok("".into()));
}

// R13,R20,R33
#[test]
fn hidden_blank_payload() {
assert_eq!(apply_unified("\n","--- before\n+++ after\n@@ -1,1 +1,2 @@\n \n+\n",PatchOptions{max_offset:0,fuzz:0}),Ok("\n\n".into()));
}

// R22,R37,R38
#[test]
fn hidden_unicode() {
assert_eq!(apply_unified("猫\n","--- before\n+++ after\n@@ -1,1 +1,1 @@\n-猫\n+é\n",PatchOptions{max_offset:0,fuzz:0}),Ok("é\n".into()));
}

// R2
#[test]
fn hidden_options_first() {
assert_eq!(apply_unified("\r","bad",PatchOptions{max_offset:17,fuzz:3}),Err(ApplyError::InvalidOptions));
}

// R3,R5
#[test]
fn hidden_missing_patch_lf() {
assert_eq!(apply_unified("","--- before\n+++ after",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::BadHeader{line:2}));
}

// R6
#[test]
fn hidden_file_header() {
assert_eq!(apply_unified("","--- wrong\n+++ wrong\n",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::BadHeader{line:1}));
}

// R6
#[test]
fn hidden_second_header() {
assert_eq!(apply_unified("","--- before\n",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::BadHeader{line:2}));
}

// R7
#[test]
fn hidden_no_hunks() {
assert_eq!(apply_unified("","--- before\n+++ after\n",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::BadHunkHeader{line:3}));
}

// R8,R9
#[test]
fn hidden_canonical_header() {
assert_eq!(apply_unified("","--- before\n+++ after\n@@ -00,0 +1,1 @@\n+x\n",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::BadHunkHeader{line:3}));
}

// R10
#[test]
fn hidden_positive_zero() {
assert_eq!(apply_unified("","--- before\n+++ after\n@@ -0,1 +1,1 @@\n-x\n+y\n",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::BadHunkHeader{line:3}));
}

// R13,R14
#[test]
fn hidden_bad_body_first() {
assert_eq!(apply_unified("","--- before\n+++ after\n@@ -1,2 +1,2 @@\n-x\n?bad\n",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::BadBody{line:5}));
}

// R14
#[test]
fn hidden_count_error() {
assert_eq!(apply_unified("","--- before\n+++ after\n@@ -1,2 +1,1 @@\n-x\n+y\n",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::CountMismatch{line:3}));
}

// R15
#[test]
fn hidden_no_change() {
assert_eq!(apply_unified("a\n","--- before\n+++ after\n@@ -1,1 +1,1 @@\n a\n",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::BadBody{line:3}));
}

// R16
#[test]
fn hidden_ordering() {
assert_eq!(apply_unified("a\nb\n","--- before\n+++ after\n@@ -2,1 +2,1 @@\n-b\n+B\n@@ -1,1 +1,1 @@\n-a\n+A\n",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::Misordered{line:6}));
}

// R17
#[test]
fn hidden_new_coordinate() {
assert_eq!(apply_unified("a\n","--- before\n+++ after\n@@ -1,1 +2,1 @@\n-a\n+A\n",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::Misordered{line:3}));
}

// R12,R19
#[test]
fn hidden_parse_before_match() {
assert_eq!(apply_unified("wrong\r","--- before\n+++ after\n@@ -1,1 +1,1 @@\n-a\n+A\n@@ broken\n",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::BadHunkHeader{line:6}));
}

// R20,R21
#[test]
fn hidden_source_validation() {
assert_eq!(apply_unified("猫\r","",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::SourceFormat{byte:3}));
}

// R20,R21
#[test]
fn hidden_source_missing() {
assert_eq!(apply_unified("x","",PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::SourceMissingLf));
}

// R1,R18,R21,R34
#[test]
fn hidden_limits() {
let opt=PatchOptions{max_offset:0,fuzz:0};
assert_eq!(apply_unified(&"x\n".repeat(129),"",opt.clone()),Err(ApplyError::TooLarge));
assert_eq!(apply_unified(&"x\n".repeat(128),"--- before\n+++ after\n@@ -0,0 +1,1 @@\n+x\n",opt.clone()),Err(ApplyError::TooLarge));
let mut p="--- before\n+++ after\n".to_string();
for i in 0..17 {p.push_str(&format!("@@ -0,0 +{},1 @@\n+x\n",i+1));}
assert_eq!(apply_unified("",&p,opt),Err(ApplyError::TooLarge));
}

// R27,R35,R36,R38
#[test]
fn hidden_revisit_and_signed() {
let src="a\nb\n";let saved=src.to_string();
let p="--- before\n+++ after\n@@ -1,1 +1,1 @@\n-a\n+x\n@@ -2,1 +2,1 @@\n-x\n+y\n";
assert_eq!(apply_unified(src,p,PatchOptions{max_offset:1,fuzz:0}),Ok("y\nb\n".into())); assert_eq!(src,saved);
let p="--- before\n+++ after\n@@ -4,1 +4,1 @@\n-a\n+z\n";
assert_eq!(apply_unified(src,p,PatchOptions{max_offset:2,fuzz:0}),Err(ApplyError::NoMatch{hunk:1}));
assert_eq!(apply_unified(src,p,PatchOptions{max_offset:3,fuzz:0}),Ok("z\nb\n".into()));
}

// R8–R11,R12,R13
#[test]
fn hidden_strict_headers() {
let opt=PatchOptions{max_offset:0,fuzz:0};
for header in ["@@ -1 +1,1 @@", "@@ -1,1  +1,1 @@", "@@ -+1,1 +1,1 @@", "@@ -1,1 +01,1 @@", "@@ -129,1 +1,1 @@", "@@ -1,1 +1,1 @@ ", "@@ -1,1 +1,1 @@\r", "@@ -١,1 +1,1 @@"] {
    let p=format!("--- before\n+++ after\n{header}\n-x\n+y\n");
    assert_eq!(apply_unified("x\n",&p,opt.clone()),Err(ApplyError::BadHunkHeader{line:3}));
}
assert_eq!(apply_unified("x\n","--- before\n+++ after\n@@ -1,1 +1,1 @@\n-x\n+y\r\n",opt.clone()),Err(ApplyError::BadBody{line:5}));
assert_eq!(apply_unified("x\n","--- before\n+++ after\n@@ -1,1 +1,1 @@\n\n",opt),Err(ApplyError::BadBody{line:4}));
}

// R2,R4,R18,R20,R21,R27,R30,R34
#[test]
fn hidden_option_and_hunk_boundaries() {
assert_eq!(apply_unified("","",PatchOptions{max_offset:0,fuzz:0}),Ok(String::new()));
assert_eq!(apply_unified("\n","",PatchOptions{max_offset:0,fuzz:0}),Ok("\n".into()));
let mut p="--- before\n+++ after\n".to_string();
for i in 0..16 {p.push_str(&format!("@@ -0,0 +{},1 @@\n+z\n",i+1));}
assert_eq!(apply_unified("",&p,PatchOptions{max_offset:16,fuzz:2}),Ok("z\n".repeat(16)));
assert_eq!(apply_unified("","",PatchOptions{max_offset:0,fuzz:3}),Err(ApplyError::InvalidOptions));
let p="--- before\n+++ after\n@@ -2,1 +2,1 @@\n-a\n+b\n";
assert_eq!(apply_unified("a\n",p,PatchOptions{max_offset:0,fuzz:0}),Err(ApplyError::NoMatch{hunk:1}));
assert_eq!(apply_unified("a\n",p,PatchOptions{max_offset:1,fuzz:0}),Ok("b\n".into()));
}

// R12,R13,R22,R33,R37,R38
#[test]
fn hidden_literal_prefix_payloads() {
assert_eq!(apply_unified("@@ raw\n+++ before\n","--- before\n+++ after\n@@ -1,2 +1,2 @@\n-@@ raw\n+--- after\n +++ before\n",PatchOptions{max_offset:0,fuzz:0}),Ok("--- after\n+++ before\n".into()));
}
