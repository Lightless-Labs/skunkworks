// R1-R4
#[test]
fn hidden_empty() {
    assert_eq!(normalize(""), Err(PathError::Empty));
}

// R3-R7, R12
#[test]
fn hidden_nul_priority() {
    assert_eq!(normalize("/../\t\\é\0x\0"), Err(PathError::Nul { at: 8 }));
}

// R3, R6-R7, R12
#[test]
fn hidden_backslash_priority() {
    assert_eq!(normalize("/../\u{7}λ\\z\\"), Err(PathError::Backslash { at: 7 }));
}

// R3, R7-R8
#[test]
fn hidden_control_offsets() {
    assert_eq!(normalize("木/\u{7f}end"), Err(PathError::Control { at: 4 }));
    assert_eq!(normalize("é/\n\t"), Err(PathError::Control { at: 3 }));
}

// R7-R8
#[test]
fn hidden_control_range() {
    for b in 1..=31u8 {
        let p = format!("x/{}", b as char);
        assert_eq!(normalize(&p), Err(PathError::Control { at: 2 }));
    }
    assert_eq!(normalize("\u{80}/\u{a0}"), Ok("\u{80}/\u{a0}".into()));
}

// R9, R14-R16
#[test]
fn hidden_root_collapse() {
    for p in ["/", "//", "///././", "/red/..//"] { assert_eq!(normalize(p), Ok("/".into())); }
}

// R3, R11-R12
#[test]
fn hidden_first_escape() {
    assert_eq!(normalize("/é/../../.."), Err(PathError::Escape { at: 7 }));
    assert_eq!(normalize("////../next/.."), Err(PathError::Escape { at: 4 }));
}

// R11, R13, R15-R16
#[test]
fn hidden_relative_parents() {
    assert_eq!(normalize("../../u/../../../v"), Ok("../../../../v".into()));
    assert_eq!(normalize("../../.."), Ok("../../../".into()));
}

// R10, R14-R16
#[test]
fn hidden_dot_directory() {
    assert_eq!(normalize("dir/."), Ok("dir/".into()));
    assert_eq!(normalize("dir/child/.."), Ok("dir/".into()));
    assert_eq!(normalize("../."), Ok("../".into()));
}

// R15-R16
#[test]
fn hidden_no_directory_invention() {
    assert_eq!(normalize("dir/./file"), Ok("dir/file".into()));
    assert_eq!(normalize("dir/../file"), Ok("file".into()));
}

// R10-R11, R14-R16
#[test]
fn hidden_empty_relative() {
    for p in [".", "./", "././", "child/../.", "child/..//"] { assert_eq!(normalize(p), Ok(".".into())); }
}

// R8, R10
#[test]
fn hidden_literal_names() {
    assert_eq!(normalize(" ~ /.../.x/x../%2e%2e/C: /"), Ok(" ~ /.../.x/x../%2e%2e/C: /".into()));
}

// R3, R8
#[test]
fn hidden_unicode_verbatim() {
    assert_eq!(normalize("é/e\u{301}/木/🦀"), Ok("é/e\u{301}/木/🦀".into()));
}

// R11, R13, R16
#[test]
fn hidden_parent_does_not_cancel_parent() {
    assert_eq!(normalize("../.."), Ok("../../".into()));
    assert_eq!(normalize("../r/../.."), Ok("../../".into()));
}

// R13-R16
#[test]
fn hidden_relative_trailing() {
    assert_eq!(normalize("z/../../"), Ok("../".into()));
    assert_eq!(normalize("../q//"), Ok("../q/".into()));
}

// R4-R7, R10-R12
#[test]
fn hidden_validation_before_reduction() {
    assert_eq!(normalize("bad\0/.."), Err(PathError::Nul { at: 3 }));
    assert_eq!(normalize("/../../last\t"), Err(PathError::Control { at: 11 }));
}

// R8-R9
#[test]
fn hidden_not_trimmed() {
    assert_eq!(normalize(" /a / b "), Ok(" /a / b ".into()));
    assert_eq!(normalize(" /../x"), Ok("x".into()));
}

// R8, R10-R11
#[test]
fn hidden_tilde_lexical() {
    assert_eq!(normalize("~/../~person"), Ok("~person".into()));
    assert_eq!(normalize("~"), Ok("~".into()));
}

// R9-R11, R14, R17
#[test]
fn hidden_deep_path() {
    let p = format!("/{}tail", "seg/./".repeat(800));
    assert_eq!(normalize(&p), Ok(format!("/{}tail", "seg/".repeat(800))));
}

// R9-R17
#[test]
fn hidden_idempotence() {
    for p in ["a//b/..", "../../r/..", "/a///./b/../", "//", "x/..", "..", "é/."] {
        let n = normalize(p).unwrap(); assert_eq!(normalize(&n), Ok(n));
    }
}

// R3, R11-R12
#[test]
fn hidden_escape_after_cancellation() {
    assert_eq!(normalize("/x/y/../../../z"), Err(PathError::Escape { at: 11 }));
}

// R13-R16
#[test]
fn hidden_parent_only() {
    assert_eq!(normalize(".."), Ok("../".into()));
    assert_eq!(normalize("./..//."), Ok("../".into()));
}

// R3, R5
#[test]
fn hidden_nul_first_occurrence() {
    assert_eq!(normalize("🦊\0\0"), Err(PathError::Nul { at: 4 }));
}

// R8-R10, R14
#[test]
fn hidden_slash_vs_relative() {
    assert_eq!(normalize("///n//..n//n.."), Ok("/n/..n/n..".into()));
    assert_eq!(normalize("n//..n//n.."), Ok("n/..n/n..".into()));
}
