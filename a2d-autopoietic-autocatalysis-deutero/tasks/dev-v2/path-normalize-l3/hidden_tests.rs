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

// R21-R22
#[test]
fn hidden_tilde_user_error() {
    for p in ["~zoe", "~~", "~.", "~é"] {
        assert_eq!(resolve(p, "/", None), Err(PathError::UnsupportedTilde { at: 1 }));
        assert_eq!(resolve(p, "/", Some("/h")), Err(PathError::UnsupportedTilde { at: 1 }));
    }
}

// R3, R22-R24
#[test]
fn hidden_tilde_root_offsets() {
    assert_eq!(resolve("~//..", "/cwd", Some("/")), Err(PathError::Escape { at: 3 }));
    assert_eq!(resolve("~/é/../../..", "/cwd", Some("/")), Err(PathError::Escape { at: 8 }));
}

// R27-R29
#[test]
fn hidden_link_validation_order() {
    let links = vec![Link { path: "relative".into(), target: "ok".into() }];
    assert_eq!(resolve_links("", "bad", Some("bad"), &links, 0), Err(PathError::InvalidCwd));
    assert_eq!(resolve_links("", "/", Some("bad"), &links, 0), Err(PathError::InvalidHome));
    assert_eq!(resolve_links("", "/", None, &links, 0), Err(PathError::InvalidLink { index: 0 }));
}

// R28-R29
#[test]
fn hidden_noncanonical_keys() {
    for key in ["/", "//a", "/a/", "/a/.", "/a/b/..", "/../a", "", "/a\\b", "/x\0"] {
        let links = vec![Link { path: key.into(), target: "valid".into() }];
        assert_eq!(resolve_links("/unused", "/", None, &links, 9), Err(PathError::InvalidLink { index: 0 }), "{key:?}");
    }
}

// R28-R29
#[test]
fn hidden_target_validation() {
    for target in ["", "x\0", "x\\", "x\t", "x\u{7f}"] {
        let links = vec![Link { path: "/unused".into(), target: target.into() }];
        assert_eq!(resolve_links("~someone", "/", None, &links, 5), Err(PathError::InvalidLink { index: 0 }));
    }
}

// R28-R30
#[test]
fn hidden_duplicate_first() {
    let links = vec![Link { path: "/k".into(), target: "x".into() }, Link { path: "/other".into(), target: "y".into() }, Link { path: "/k".into(), target: "z".into() }, Link { path: "bad".into(), target: "".into() }];
    assert_eq!(resolve_links("/", "/", None, &links, 1), Err(PathError::DuplicateLink { first: 0, second: 2 }));
}

// R29-R30
#[test]
fn hidden_invalid_duplicate_entry() {
    let links = vec![Link { path: "/k".into(), target: "x".into() }, Link { path: "/k".into(), target: "".into() }];
    assert_eq!(resolve_links("/", "/", None, &links, 1), Err(PathError::InvalidLink { index: 1 }));
}

// R24, R32-R34
#[test]
fn hidden_link_before_parent() {
    let links = vec![Link { path: "/jump".into(), target: "/deep/place".into() }];
    assert_eq!(resolve_links("/jump/../leaf", "/", None, &links, 1), Ok("/deep/leaf".into()));
}

// R19-R20, R23, R31-R33
#[test]
fn hidden_contexts_are_traversed() {
    let links = vec![Link { path: "/alias".into(), target: "/storage/location".into() }];
    assert_eq!(resolve_links("../leaf", "/alias/sub", None, &links, 2), Ok("/storage/location/leaf".into()));
    assert_eq!(resolve_links("~/leaf", "/", Some("/alias"), &links, 2), Ok("/storage/location/leaf".into()));
    assert_eq!(resolve_links("/leaf", "/alias", None, &links, 0), Ok("/leaf".into()));
}

// R32-R33
#[test]
fn hidden_relative_target_parent() {
    let links = vec![Link { path: "/a/b/door".into(), target: "../../vault/./x".into() }];
    assert_eq!(resolve_links("/a/b/door/../end", "/", None, &links, 2), Ok("/vault/end".into()));
}

// R32-R35
#[test]
fn hidden_nested_target_suffix() {
    let links = vec![Link { path: "/one".into(), target: "/two/inside".into() }, Link { path: "/two".into(), target: "/real".into() }];
    assert_eq!(resolve_links("/one/end", "/", None, &links, 2), Ok("/real/inside/end".into()));
}

// R3, R12, R33-R34
#[test]
fn hidden_target_offset() {
    let links = vec![Link { path: "/bad".into(), target: "é/../../oops".into() }];
    assert_eq!(resolve_links("/bad", "/", None, &links, 2), Err(PathError::Escape { at: 6 }));
    let links = vec![Link { path: "/gone".into(), target: "/".into() }];
    assert_eq!(resolve_links("/gone/../z", "/", None, &links, 2), Err(PathError::Escape { at: 6 }));
}

// R22, R33
#[test]
fn hidden_target_tilde_literal() {
    let links = vec![Link { path: "/l".into(), target: "~/folder".into() }];
    assert_eq!(resolve_links("/l", "/", Some("/home"), &links, 1), Ok("/~/folder".into()));
}

// R15-R16, R25, R34
#[test]
fn hidden_target_directory_ignored() {
    let links = vec![Link { path: "/l".into(), target: "/actual/".into() }];
    assert_eq!(resolve_links("/l", "/", None, &links, 1), Ok("/actual".into()));
    assert_eq!(resolve_links("/l/.", "/", None, &links, 1), Ok("/actual/".into()));
}

// R35-R37
#[test]
fn hidden_self_loop() {
    let links = vec![Link { path: "/l".into(), target: "l".into() }];
    assert_eq!(resolve_links("/l", "/", None, &links, 1), Err(PathError::Loop { index: 0 }));
    assert_eq!(resolve_links("/l", "/", None, &links, 100), Err(PathError::Loop { index: 0 }));
}

// R35-R36
#[test]
fn hidden_indirect_loop_index() {
    let links = vec![Link { path: "/b".into(), target: "/a".into() }, Link { path: "/a".into(), target: "/b".into() }];
    assert_eq!(resolve_links("/a", "/", None, &links, 2), Err(PathError::Loop { index: 1 }));
    assert_eq!(resolve_links("/a", "/", None, &links, 1), Err(PathError::Limit { index: 0 }));
}

// R35-R37
#[test]
fn hidden_reuse_completed_link() {
    let links = vec![Link { path: "/a".into(), target: "/real".into() }];
    assert_eq!(resolve_links("/a/../a/file", "/", None, &links, 2), Ok("/real/file".into()));
    assert_eq!(resolve_links("/a/../a/file", "/", None, &links, 1), Err(PathError::Limit { index: 0 }));
}

// R32, R35-R36
#[test]
fn hidden_active_until_suffix_done() {
    let links = vec![Link { path: "/a".into(), target: "/b/../a".into() }, Link { path: "/b".into(), target: "/x".into() }];
    assert_eq!(resolve_links("/a", "/", None, &links, 2), Err(PathError::Loop { index: 0 }));
}

// R29, R36-R37
#[test]
fn hidden_unreachable_bad_targets() {
    let links = vec![Link { path: "/bad".into(), target: "/../../oops".into() }, Link { path: "/cycle".into(), target: "/cycle".into() }];
    assert_eq!(resolve_links("/ordinary", "/", None, &links, 0), Ok("/ordinary".into()));
    assert_eq!(resolve_links("/bad", "/", None, &links, 1), Err(PathError::Escape { at: 1 }));
}

// R8, R27, R32
#[test]
fn hidden_lookup_components_only() {
    let links = vec![Link { path: "/ab".into(), target: "/dest".into() }, Link { path: "/ab/child".into(), target: "/unreachable".into() }];
    assert_eq!(resolve_links("/abc/child", "/", None, &links, 0), Ok("/abc/child".into()));
    assert_eq!(resolve_links("/ab/child", "/", None, &links, 1), Ok("/dest/child".into()));
}

// R36-R37
#[test]
fn hidden_fresh_calls_and_zero() {
    let links = vec![Link { path: "/z".into(), target: "/w".into() }];
    for _ in 0..3 { assert_eq!(resolve_links("/z", "/", None, &links, 1), Ok("/w".into())); }
    assert_eq!(resolve_links("/z", "/", None, &links, 0), Err(PathError::Limit { index: 0 }));
    assert_eq!(resolve_links("/w", "/", None, &links, 0), Ok("/w".into()));
}

// R27, R32-R33, R36-R37
#[test]
fn hidden_links_deep_chain() {
    let links: Vec<Link> = (0..80).map(|i| Link { path: format!("/k{i}"), target: if i == 79 { "/done".into() } else { format!("/k{}", i+1) } }).collect();
    assert_eq!(resolve_links("/k0/tail", "/", None, &links, 80), Ok("/done/tail".into()));
    assert_eq!(resolve_links("/k0", "/", None, &links, 79), Err(PathError::Limit { index: 79 }));
}

// R8-R10, R14-R18, R21-R31
#[test]
fn hidden_earlier_contracts() {
    for p in ["../q/..", "//u///v/.", "~name", "../.."] {
        let n = normalize(p).unwrap(); assert_eq!(normalize(&n), Ok(n));
    }
    assert_eq!(resolve("leaf", "/c/", None), Ok("/c/leaf".into()));
    assert_eq!(resolve("~", "/c", Some("/h/")), Ok("/h".into()));
    assert_eq!(resolve_links("~", "/c", None, &[], 0), Err(PathError::MissingHome));
    assert_eq!(resolve_links("", "/c", None, &[], 0), Err(PathError::Empty));
    assert_eq!(resolve_links("~user\\\\\0", "/c", None, &[], 0), Err(PathError::Nul { at: 7 }));
    assert_eq!(resolve("~/.", "/c", Some("/h")), Ok("/h/".into()));
}
