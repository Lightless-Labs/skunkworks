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

// R28-R30
#[test]
fn hidden_duplicate_first() {
    let links = vec![Link { path: "/k".into(), target: "x".into() }, Link { path: "/other".into(), target: "y".into() }, Link { path: "/k".into(), target: "z".into() }, Link { path: "bad".into(), target: "".into() }];
    assert_eq!(resolve_links("/", "/", None, &links, 1), Err(PathError::DuplicateLink { first: 0, second: 2 }));
}

// R24, R32-R34
#[test]
fn hidden_link_before_parent() {
    let links = vec![Link { path: "/jump".into(), target: "/deep/place".into() }];
    assert_eq!(resolve_links("/jump/../leaf", "/", None, &links, 1), Ok("/deep/leaf".into()));
}

// R35-R37
#[test]
fn hidden_self_loop() {
    let links = vec![Link { path: "/l".into(), target: "l".into() }];
    assert_eq!(resolve_links("/l", "/", None, &links, 1), Err(PathError::Loop { index: 0 }));
    assert_eq!(resolve_links("/l", "/", None, &links, 100), Err(PathError::Loop { index: 0 }));
}

// R35-R37
#[test]
fn hidden_reuse_completed_link() {
    let links = vec![Link { path: "/a".into(), target: "/real".into() }];
    assert_eq!(resolve_links("/a/../a/file", "/", None, &links, 2), Ok("/real/file".into()));
    assert_eq!(resolve_links("/a/../a/file", "/", None, &links, 1), Err(PathError::Limit { index: 0 }));
}

// R36-R37
#[test]
fn hidden_fresh_calls_and_zero() {
    let links = vec![Link { path: "/z".into(), target: "/w".into() }];
    for _ in 0..3 { assert_eq!(resolve_links("/z", "/", None, &links, 1), Ok("/w".into())); }
    assert_eq!(resolve_links("/z", "/", None, &links, 0), Err(PathError::Limit { index: 0 }));
    assert_eq!(resolve_links("/w", "/", None, &links, 0), Ok("/w".into()));
}

// R38-R41
#[test]
fn hidden_relative_inputs_share_cwd() {
    assert_eq!(relative_to("base", "leaf", "/cwd", None, &[], 0), Ok("../leaf".into()));
    assert_eq!(relative_to("~/base", "~/leaf", "/else", Some("/home"), &[], 0), Ok("../leaf".into()));
}

// R28, R38-R39
#[test]
fn hidden_base_error_first() {
    assert_eq!(relative_to("/../bad", "", "/", None, &[], 0), Err(PathError::Escape { at: 1 }));
    assert_eq!(relative_to("~user", "/bad\0", "/", None, &[], 0), Err(PathError::UnsupportedTilde { at: 1 }));
    assert_eq!(relative_to("/ok", "", "/", None, &[], 0), Err(PathError::Empty));
    for link in [Link { path: "/noncanonical/".into(), target: "x".into() }, Link { path: "/key".into(), target: "".into() }] {
        assert_eq!(relative_to("", "", "/", None, &[link], 0), Err(PathError::InvalidLink { index: 0 }));
    }
}

// R39-R41
#[test]
fn hidden_base_is_directory() {
    assert_eq!(relative_to("/dir/file", "/dir/item", "/", None, &[], 0), Ok("../item".into()));
    assert_eq!(relative_to("/dir/file/", "/dir/item", "/", None, &[], 0), Ok("../item".into()));
}

// R40-R43
#[test]
fn hidden_component_prefix_only() {
    assert_eq!(relative_to("/alpha", "/alphabet", "/", None, &[], 0), Ok("../alphabet".into()));
    let links = vec![Link { path: "/s".into(), target: "/alpha".into() }];
    assert_eq!(relative_to("/", "/alphabet/leaf", "/", None, &links, 1), Ok("alphabet/leaf".into()));
}

// R42-R46
#[test]
fn hidden_shortcut_exact_target() {
    let links = vec![Link { path: "/jump".into(), target: "/deeply/nested/destination".into() }];
    assert_eq!(relative_to("/", "/deeply/nested/destination", "/", None, &links, 1), Ok("jump".into()));
}

// R40, R42-R44
#[test]
fn hidden_shortcut_to_root() {
    let links = vec![Link { path: "/a/b/r".into(), target: "/".into() }];
    assert_eq!(relative_to("/a/b", "/", "/", None, &links, 1), Ok("r/".into()));
    assert_eq!(relative_to("/a/b", "/verylongname", "/", None, &links, 1), Ok("r/verylongname".into()));
}

// R42-R43, R46
#[test]
fn hidden_tie_lexical_order() {
    let links = vec![Link { path: "/z".into(), target: "/longdestination".into() }, Link { path: "/a".into(), target: "/longdestination".into() }];
    assert_eq!(relative_to("/", "/longdestination", "/", None, &links, 1), Ok("a".into()));
    let reverse: Vec<Link> = links.into_iter().rev().collect();
    assert_eq!(relative_to("/", "/longdestination", "/", None, &reverse, 1), Ok("a".into()));
}

// R41-R43, R46
#[test]
fn hidden_tie_bytes_not_components() {
    let links = vec![Link { path: "/b/zzzz".into(), target: "/longdestination".into() }, Link { path: "/x".into(), target: "/longdestination".into() }];
    assert_eq!(relative_to("/b", "/longdestination", "/", None, &links, 1), Ok("../x".into()));
}

// R3, R8, R46
#[test]
fn hidden_ranking_utf8_length() {
    let links = vec![Link { path: "/é".into(), target: "/destination".into() }, Link { path: "/z".into(), target: "/destination".into() }];
    assert_eq!(relative_to("/", "/destination", "/", None, &links, 1), Ok("z".into()));
    let links = vec![Link { path: "/é".into(), target: "/destination".into() }, Link { path: "/aa".into(), target: "/destination".into() }];
    assert_eq!(relative_to("/", "/destination", "/", None, &links, 1), Ok("aa".into()));
}

// R29, R37, R42
#[test]
fn hidden_probe_errors_are_skipped() {
    let links = vec![Link { path: "/cycle".into(), target: "/cycle".into() }, Link { path: "/bad".into(), target: "../..".into() }, Link { path: "/s".into(), target: "/destination".into() }];
    assert_eq!(relative_to("/", "/destination", "/", None, &links, 2), Ok("s".into()));
}

// R36, R42, R45, R48
#[test]
fn hidden_probe_budget_and_fresh_validation() {
    let links = vec![Link { path: "/s".into(), target: "/another".into() }, Link { path: "/another".into(), target: "/destination".into() }];
    assert_eq!(relative_to("/", "/destination", "/", None, &links, 1), Ok("another".into()));
    assert_eq!(relative_to("/s", "/s/child", "/", None, &links, 2), Ok("child".into()));
    assert_eq!(relative_to("/", "/destination", "/", None, &links, 2), Ok("s".into()));
}

// R15-R16, R34, R44, R48
#[test]
fn hidden_target_markers_on_shortcuts() {
    let links = vec![Link { path: "/s".into(), target: "/destination/".into() }];
    assert_eq!(relative_to("/", "/destination", "/", None, &links, 1), Ok("s".into()));
    assert_eq!(relative_to("/", "/destination/.", "/", None, &links, 1), Ok("s/".into()));
}

// R41, R44, R47-R48
#[test]
fn hidden_same_location_dot() {
    for (b,t) in [("/", "/"), ("/d/", "/d"), ("/d", "/d/"), ("/d", "/d/.")] {
        assert_eq!(relative_to(b, t, "/", None, &[], 0), Ok(".".into()));
    }
}

// R41, R44-R45, R48
#[test]
fn hidden_ancestors_marker_spelling() {
    assert_eq!(relative_to("/a/b/c", "/a", "/", None, &[], 0), Ok("../..".into()));
    assert_eq!(relative_to("/a/b/c", "/a/", "/", None, &[], 0), Ok("../../".into()));
    assert_eq!(relative_to("/a/b/c", "/", "/", None, &[], 0), Ok("../../../".into()));
}

// R32, R38-R41, R48
#[test]
fn hidden_resolved_base_and_target() {
    let links = vec![Link { path: "/b".into(), target: "/real/deep".into() }, Link { path: "/t".into(), target: "/real/peer".into() }];
    assert_eq!(relative_to("/b", "/t/leaf", "/", None, &links, 1), Ok("../../t/leaf".into()));
    assert_eq!(relative_to(".", "../leaf", "/b", None, &links, 1), Ok("../leaf".into()));
    assert_eq!(relative_to("~", "~/child", "/", Some("/b"), &links, 1), Ok("child".into()));
    let bad = vec![Link { path: "/bad".into(), target: "é/../../oops".into() }];
    assert_eq!(relative_to("/", "/bad", "/", None, &bad, 1), Err(PathError::Escape { at: 6 }));
}

// R42-R43, R45
#[test]
fn hidden_alias_key_vs_anchor() {
    let links = vec![Link { path: "/p".into(), target: "/verylongparent".into() }, Link { path: "/p/q".into(), target: "/unreachable".into() }];
    assert_eq!(relative_to("/", "/verylongparent/q/item", "/", None, &links, 1), Ok("p/q/item".into()));
    assert_eq!(relative_to("/", "/unreachable", "/", None, &links, 1), Ok("unreachable".into()));
}

// R39-R48
#[test]
fn hidden_relative_roundtrips() {
    let links = vec![Link { path: "/r".into(), target: "/long/place".into() }];
    for b in ["/", "/x/y", "/long/place", "/r/sub"] {
        for t in ["/long/place/item", "/long/place/", "/", "/r/sub"] {
            let result = relative_to(b, t, "/", None, &links, 4).unwrap();
            let base = resolve_links(b, "/", None, &links, 4).unwrap();
            let wanted = resolve_links(t, "/", None, &links, 4).unwrap();
            let got = resolve_links(&result, &base, None, &links, 4).unwrap();
            assert_eq!(got.trim_end_matches('/'), wanted.trim_end_matches('/'));
            if result != "." { assert_eq!(result.ends_with('/'), wanted.ends_with('/')); }
            assert!(!result.starts_with('/'));
            assert!(!result.starts_with("./") || result.starts_with("./~"));
        }
    }
}

// R41, R47, R49
#[test]
fn hidden_unbounded_relative_depth() {
    let base = format!("/{}last", "deep/".repeat(220));
    assert_eq!(relative_to(&base, "/z", "/", None, &[], 0), Ok(format!("{}z", "../".repeat(221))));
}

// R42-R43, R46-R49
#[test]
fn hidden_candidate_set_is_finite() {
    let links = vec![Link { path: "/longkey".into(), target: "/d".into() }];
    assert_eq!(relative_to("/", "/d/x", "/", None, &links, 1), Ok("d/x".into()));
    assert_eq!(relative_to("/", "/literal/~name", "/", Some("/literal"), &[], 0), Ok("literal/~name".into()));
    assert_eq!(relative_to("/", "/~name", "/", Some("/home"), &[], 0), Ok("./~name".into()));
    assert_eq!(relative_to("/", "/~/", "/", None, &[], 0), Ok("./~/".into()));
}

// R36, R39, R42
#[test]
fn hidden_budget_errors_propagate_for_inputs() {
    let links = vec![Link { path: "/b".into(), target: "/place".into() }];
    assert_eq!(relative_to("/b", "/", "/", None, &links, 0), Err(PathError::Limit { index: 0 }));
    assert_eq!(relative_to("/", "/b", "/", None, &links, 0), Err(PathError::Limit { index: 0 }));
}
