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

// R18-R21, R26
#[test]
fn hidden_cwd_before_everything() {
    assert_eq!(resolve("", "relative", Some("also-relative")), Err(PathError::InvalidCwd));
    assert_eq!(resolve("/ok", "/../bad", None), Err(PathError::InvalidCwd));
    assert_eq!(resolve("/ok", "", None), Err(PathError::InvalidCwd));
}

// R19-R21, R26
#[test]
fn hidden_home_eager() {
    for h in ["", "relative", "/../oops", "/bad\0", "~/user"] {
        assert_eq!(resolve("/absolute", "/ok", Some(h)), Err(PathError::InvalidHome));
    }
    assert_eq!(resolve("", "/ok", Some("relative")), Err(PathError::InvalidHome));
}

// R19-R20, R23, R25-R26
#[test]
fn hidden_context_normalization() {
    assert_eq!(resolve("leaf", "//one/./two/../", Some("/home//me/../")), Ok("/one/leaf".into()));
    assert_eq!(resolve("~", "/cwd/", Some("/home/me/../")), Ok("/home".into()));
}

// R19-R20, R23-R25
#[test]
fn hidden_absolute_ignores_components() {
    assert_eq!(resolve("//fresh/../top", "/old/deep/", Some("/home/")), Ok("/top".into()));
}

// R3-R7, R21-R22
#[test]
fn hidden_requested_validation_priority() {
    assert_eq!(resolve("~n\\\0", "/", None), Err(PathError::Nul { at: 3 }));
    assert_eq!(resolve("~n\n\\", "/", None), Err(PathError::Backslash { at: 3 }));
    assert_eq!(resolve("~n\t", "/", None), Err(PathError::Control { at: 2 }));
}

// R21-R22
#[test]
fn hidden_tilde_user_error() {
    for p in ["~zoe", "~~", "~.", "~é"] {
        assert_eq!(resolve(p, "/", None), Err(PathError::UnsupportedTilde { at: 1 }));
        assert_eq!(resolve(p, "/", Some("/h")), Err(PathError::UnsupportedTilde { at: 1 }));
    }
}

// R22
#[test]
fn hidden_tilde_missing() {
    assert_eq!(resolve("~//folder", "/cwd", None), Err(PathError::MissingHome));
    assert_eq!(resolve("~/..", "/cwd", None), Err(PathError::MissingHome));
}

// R3, R22-R24
#[test]
fn hidden_tilde_root_offsets() {
    assert_eq!(resolve("~//..", "/cwd", Some("/")), Err(PathError::Escape { at: 3 }));
    assert_eq!(resolve("~/é/../../..", "/cwd", Some("/")), Err(PathError::Escape { at: 8 }));
}

// R3, R23-R24
#[test]
fn hidden_relative_escape_offset() {
    assert_eq!(resolve("a/../../../..", "/c", None), Err(PathError::Escape { at: 8 }));
    assert_eq!(resolve("../..", "/é", None), Err(PathError::Escape { at: 3 }));
}

// R15-R16, R25
#[test]
fn hidden_directory_from_request() {
    assert_eq!(resolve("file", "/dir/", Some("/home/")), Ok("/dir/file".into()));
    assert_eq!(resolve(".", "/dir", None), Ok("/dir/".into()));
    assert_eq!(resolve("x/..", "/dir", None), Ok("/dir/".into()));
}

// R22-R25
#[test]
fn hidden_home_directory_intent() {
    assert_eq!(resolve("~", "/cwd", Some("/h/")), Ok("/h".into()));
    assert_eq!(resolve("~/", "/cwd", Some("/h")), Ok("/h/".into()));
    assert_eq!(resolve("~/.", "/cwd", Some("/")), Ok("/".into()));
}

// R8, R22-R24
#[test]
fn hidden_tilde_not_inside() {
    assert_eq!(resolve("x/~/~some", "/cwd", None), Ok("/cwd/x/~/~some".into()));
    assert_eq!(resolve("/~/x", "/cwd", None), Ok("/~/x".into()));
}

// R9, R22-R25
#[test]
fn hidden_tilde_multiple_slashes() {
    assert_eq!(resolve("~////a/../b/", "/", Some("//h//me")), Ok("/h/me/b/".into()));
}

// R8, R20, R26
#[test]
fn hidden_literal_home_tilde() {
    assert_eq!(resolve("~/x", "/cwd", Some("/~person")), Ok("/~person/x".into()));
    assert_eq!(resolve("x", "/~/cwd", None), Ok("/~/cwd/x".into()));
}

// R4, R21
#[test]
fn hidden_resolve_empty() {
    assert_eq!(resolve("", "/", None), Err(PathError::Empty));
}

// R17-R18, R26
#[test]
fn hidden_resolve_isolation() {
    for _ in 0..3 {
        assert_eq!(resolve("../z", "/a/b", None), Ok("/a/z".into()));
        assert_eq!(resolve("../z", "/q/r", None), Ok("/q/z".into()));
    }
    assert_eq!(normalize("~/x"), Ok("~/x".into()));
}
