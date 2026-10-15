// R1, R2, R3, R18, R4
#[test]
fn hidden_empty_and_ids() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.resolve("GET", "/"), Err(Error::NotFound));
    assert_eq!(r.add("GET", "x"), Err(Error::InvalidPattern { at: 0 }));
    assert_eq!(r.add("GET", "/"), Ok(0));
    assert_eq!(
        r.resolve("GET", "/"),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/"),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
}

// R4, R12
#[test]
fn hidden_absolute_inputs() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.resolve("GET", ""), Err(Error::InvalidPath { at: 0 }));
    assert_eq!(r.resolve("GET", "abc"), Err(Error::InvalidPath { at: 0 }));
    assert_eq!(r.resolve("GET", "?ignored"), Err(Error::InvalidPath { at: 0 }));
    assert_eq!(
        r.resolve("GET", "relative/path"),
        Err(Error::InvalidPath { at: 0 })
    );
    assert_eq!(r.add("GET", ""), Err(Error::InvalidPattern { at: 0 }));
}

// R5, R8, R20
#[test]
fn hidden_suffix_is_removed_first() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/doc%3Fv%23x").unwrap();
    assert_eq!(
        r.resolve("GET", "/doc%3Fv%23x?bad=%zz#é"),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/doc%3Fv%23x#anything?x"),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
    assert_eq!(r.add("GET", "/doc?x"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("GET", "/doc#x"), Err(Error::InvalidPattern { at: 4 }));
}

// R6, R12, R13
#[test]
fn hidden_empty_segments_and_positions() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.resolve("GET", "//"), Err(Error::InvalidPath { at: 1 }));
    assert_eq!(r.resolve("GET", "/ab//cd"), Err(Error::InvalidPath { at: 4 }));
    assert_eq!(r.resolve("GET", "/ab//"), Err(Error::InvalidPath { at: 4 }));
    assert_eq!(r.resolve("GET", "///x"), Err(Error::InvalidPath { at: 1 }));
    assert_eq!(r.add("GET", "/t//v"), Err(Error::InvalidPattern { at: 3 }));
}

// R7, R20, R6
#[test]
fn hidden_case_and_dot_segments() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/X/../.").unwrap();
    assert_eq!(
        r.resolve("GET", "/X/../."),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
    assert_eq!(r.resolve("GET", "/x/../."), Err(Error::NotFound));
    assert_eq!(r.resolve("GET", "/."), Err(Error::NotFound));
    assert_eq!(r.resolve("GET", "/X"), Err(Error::NotFound));
}

// R7, R12
#[test]
fn hidden_raw_byte_positions() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.resolve("GET", "/abc d"), Err(Error::InvalidPath { at: 4 }));
    assert_eq!(r.resolve("GET", "/é"), Err(Error::InvalidPath { at: 1 }));
    assert_eq!(r.resolve("GET", "/ab/:x"), Err(Error::InvalidPath { at: 4 }));
    assert_eq!(r.resolve("GET", "/x/*"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("GET", "/a/\t"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.add("GET", "/ok/abé"), Err(Error::InvalidPattern { at: 6 }));
}

// R8, R9, R14
#[test]
fn hidden_decoded_punctuation() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/%3A/:value").unwrap();
    assert_eq!(
        r.resolve("GET", "/%3A/%20%25%3F%23%2A"),
        Ok(Match {
            route: 0,
            params: vec![("value".into(), " %?#*".into())],
            redirect: None
        })
    );
    assert_eq!(r.resolve("GET", "/%3a/x"), Err(Error::InvalidPath { at: 1 }));
}

// R8, R12
#[test]
fn hidden_escape_rejection_matrix() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.resolve("GET", "/v/%"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("GET", "/v/%2"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("GET", "/v/%0G"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("GET", "/v/%gg"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("GET", "/v/%2f"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("GET", "/v/%2F"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("GET", "/v/%41"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("GET", "/v/%7E"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("GET", "/v/%00"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("GET", "/v/%1F"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("GET", "/v/%7F"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("GET", "/v/%80"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("GET", "/v/%FF"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.add("GET", "/a/%61"), Err(Error::InvalidPattern { at: 3 }));
}

// R9, R10, R12
#[test]
fn hidden_capture_grammar() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.add("GET", "/ok/:"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("GET", "/ok/:A"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("GET", "/ok/:0a"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("GET", "/ok/:a-b"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("GET", "/ok/:a%GG"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("GET", "/ok/:a:b"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("GET", "/ok/:é"), Err(Error::InvalidPattern { at: 4 }));
    r.add("GET", "/ok/:a_9").unwrap();
    assert_eq!(
        r.resolve("GET", "/ok/z"),
        Ok(Match {
            route: 0,
            params: vec![("a_9".into(), "z".into())],
            redirect: None
        })
    );
}

// R10, R3, R11
#[test]
fn hidden_duplicate_capture_namespace() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.add("GET", "/:same/:same"), Err(Error::InvalidPattern { at: 7 }));
    assert_eq!(r.add("GET", "/:same/*same"), Err(Error::InvalidPattern { at: 7 }));
    assert_eq!(r.add("GET", "/:a/:b"), Ok(0));
}

// R12, R10, R8, R18
#[test]
fn hidden_validation_precedence() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.add("GET", "/%GG/:bad-"), Err(Error::InvalidPattern { at: 1 }));
    assert_eq!(r.add("GET", "/:bad-/%GG"), Err(Error::InvalidPattern { at: 1 }));
    assert_eq!(
        r.resolve("GET", "/unmatched/%GG/"),
        Err(Error::InvalidPath { at: 11 })
    );
    assert_eq!(r.add("GET", "/%GG/"), Err(Error::InvalidPattern { at: 1 }));
}

// R16
#[test]
fn hidden_specificity_is_lexicographic() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/:a/b/c").unwrap();
    r.add("GET", "/fixed/:b/:c").unwrap();
    assert_eq!(
        r.resolve("GET", "/fixed/b/c"),
        Ok(Match {
            route: 1,
            params: vec![("b".into(), "b".into()), ("c".into(), "c".into())],
            redirect: None
        })
    );
}

// R17, R19, R3
#[test]
fn hidden_earliest_registration() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/:z/k").unwrap();
    r.add("GET", "/:a/k").unwrap();
    r.add("GET", "/:z/k").unwrap();
    assert_eq!(
        r.resolve("GET", "/v/k"),
        Ok(Match {
            route: 0,
            params: vec![("z".into(), "v".into())],
            redirect: None
        })
    );
}

// R14, R3
#[test]
fn hidden_capture_order_and_repeated_resolution() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/:z/m/:a").unwrap();
    assert_eq!(
        r.resolve("GET", "/left/m/right"),
        Ok(Match {
            route: 0,
            params: vec![("z".into(), "left".into()), ("a".into(), "right".into())],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/other/m/end"),
        Ok(Match {
            route: 0,
            params: vec![("z".into(), "other".into()), ("a".into(), "end".into())],
            redirect: None
        })
    );
}

// R16, R18, R20
#[test]
fn hidden_whole_path_only() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/a/:v").unwrap();
    assert_eq!(r.resolve("GET", "/a"), Err(Error::NotFound));
    assert_eq!(r.resolve("GET", "/a/b/c"), Err(Error::NotFound));
    assert_eq!(r.resolve("GET", "/aa/b"), Err(Error::NotFound));
}

// R11, R20, R8
#[test]
fn hidden_escaped_markers_are_literal() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/%3Aid/%2Atail").unwrap();
    assert_eq!(
        r.resolve("GET", "/%3Aid/%2Atail"),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
    assert_eq!(r.resolve("GET", "/hello/there"), Err(Error::NotFound));
}

// R8, R20
#[test]
fn hidden_single_decoding() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/%252F/:v").unwrap();
    assert_eq!(
        r.resolve("GET", "/%252F/%2520"),
        Ok(Match {
            route: 0,
            params: vec![("v".into(), "%20".into())],
            redirect: None
        })
    );
    assert_eq!(r.resolve("GET", "/%2F/a"), Err(Error::InvalidPath { at: 1 }));
}

// R13, R22, R26, R14
#[test]
fn hidden_strict_slashes() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/end").unwrap();
    r.add("POST", "/end/").unwrap();
    assert_eq!(
        r.resolve("GET", "/end/"),
        Err(Error::MethodNotAllowed {
            allowed: vec!["POST".into()]
        })
    );
    assert_eq!(
        r.resolve("POST", "/end/"),
        Ok(Match {
            route: 1,
            params: vec![],
            redirect: None
        })
    );
    assert_eq!(r.resolve("GET", "/other/"), Err(Error::NotFound));
    r.add("GET", "/only/").unwrap();
    assert_eq!(r.resolve("GET", "/only"), Err(Error::NotFound));
    assert_eq!(
        r.resolve("GET", "/only/"),
        Ok(Match {
            route: 2,
            params: vec![],
            redirect: None
        })
    );
}

// R11, R10, R12
#[test]
fn hidden_wildcard_syntax() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.add("GET", "/a/*rest/z"), Err(Error::InvalidPattern { at: 3 }));
    assert_eq!(r.add("GET", "/a/*R"), Err(Error::InvalidPattern { at: 3 }));
    assert_eq!(r.add("GET", "/a/*/z"), Err(Error::InvalidPattern { at: 3 }));
    assert_eq!(r.add("GET", "/*rest/%GG"), Err(Error::InvalidPattern { at: 1 }));
}

// R3, R16, R18
#[test]
fn hidden_larger_route_table() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/table/0").unwrap();
    r.add("GET", "/table/1").unwrap();
    r.add("GET", "/table/2").unwrap();
    r.add("GET", "/table/3").unwrap();
    r.add("GET", "/table/4").unwrap();
    r.add("GET", "/table/5").unwrap();
    r.add("GET", "/table/6").unwrap();
    r.add("GET", "/table/7").unwrap();
    r.add("GET", "/table/8").unwrap();
    r.add("GET", "/table/9").unwrap();
    r.add("GET", "/table/10").unwrap();
    r.add("GET", "/table/11").unwrap();
    r.add("GET", "/table/12").unwrap();
    r.add("GET", "/table/13").unwrap();
    r.add("GET", "/table/14").unwrap();
    r.add("GET", "/table/15").unwrap();
    r.add("GET", "/table/16").unwrap();
    r.add("GET", "/table/17").unwrap();
    r.add("GET", "/table/18").unwrap();
    r.add("GET", "/table/19").unwrap();
    r.add("GET", "/table/20").unwrap();
    r.add("GET", "/table/21").unwrap();
    r.add("GET", "/table/22").unwrap();
    r.add("GET", "/table/23").unwrap();
    r.add("GET", "/table/24").unwrap();
    r.add("GET", "/table/25").unwrap();
    r.add("GET", "/table/26").unwrap();
    r.add("GET", "/table/27").unwrap();
    r.add("GET", "/table/28").unwrap();
    r.add("GET", "/table/29").unwrap();
    r.add("GET", "/table/30").unwrap();
    r.add("GET", "/table/31").unwrap();
    r.add("GET", "/table/32").unwrap();
    r.add("GET", "/table/33").unwrap();
    r.add("GET", "/table/34").unwrap();
    r.add("GET", "/table/35").unwrap();
    r.add("GET", "/table/36").unwrap();
    r.add("GET", "/table/37").unwrap();
    r.add("GET", "/table/38").unwrap();
    r.add("GET", "/table/39").unwrap();
    r.add("GET", "/table/40").unwrap();
    r.add("GET", "/table/41").unwrap();
    r.add("GET", "/table/42").unwrap();
    r.add("GET", "/table/43").unwrap();
    r.add("GET", "/table/44").unwrap();
    r.add("GET", "/table/45").unwrap();
    r.add("GET", "/table/46").unwrap();
    r.add("GET", "/table/47").unwrap();
    r.add("GET", "/table/:n").unwrap();
    assert_eq!(
        r.resolve("GET", "/table/37"),
        Ok(Match {
            route: 37,
            params: vec![],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/table/unknown"),
        Ok(Match {
            route: 48,
            params: vec![("n".into(), "unknown".into())],
            redirect: None
        })
    );
}

// R15, R16, R14
#[test]
fn hidden_wildcard_zero_and_many() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/tree/*rest").unwrap();
    r.add("GET", "/tree").unwrap();
    assert_eq!(
        r.resolve("GET", "/tree"),
        Ok(Match {
            route: 1,
            params: vec![],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/tree/a%20b/c"),
        Ok(Match {
            route: 0,
            params: vec![("rest".into(), "a b/c".into())],
            redirect: None
        })
    );
    assert_eq!(r.resolve("GET", "/trees"), Err(Error::NotFound));
}

// R16, R15, R17
#[test]
fn hidden_wildcard_vs_parameter() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/x/*tail").unwrap();
    r.add("GET", "/x/:id").unwrap();
    assert_eq!(
        r.resolve("GET", "/x/q"),
        Ok(Match {
            route: 1,
            params: vec![("id".into(), "q".into())],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/x"),
        Ok(Match {
            route: 0,
            params: vec![("tail".into(), "".into())],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/x/q/r"),
        Ok(Match {
            route: 0,
            params: vec![("tail".into(), "q/r".into())],
            redirect: None
        })
    );
}

// R16, R15
#[test]
fn hidden_wildcard_early_specificity() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/:v/fixed/end").unwrap();
    r.add("GET", "/root/*all").unwrap();
    r.add("GET", "/*any").unwrap();
    assert_eq!(
        r.resolve("GET", "/root/fixed/end"),
        Ok(Match {
            route: 1,
            params: vec![("all".into(), "fixed/end".into())],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/"),
        Ok(Match {
            route: 2,
            params: vec![("any".into(), "".into())],
            redirect: None
        })
    );
}

// R21, R12
#[test]
fn hidden_method_validation_order() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.resolve("", "bad"), Err(Error::InvalidMethod));
    assert_eq!(r.resolve("get", "bad"), Err(Error::InvalidMethod));
    assert_eq!(r.resolve("GET1", "bad"), Err(Error::InvalidMethod));
    assert_eq!(r.resolve("GÉT", "bad"), Err(Error::InvalidMethod));
    assert_eq!(r.resolve("GET-", "bad"), Err(Error::InvalidMethod));
    assert_eq!(r.add("get", "bad"), Err(Error::InvalidMethod));
    r.add("CUSTOM", "/x").unwrap();
    assert_eq!(
        r.resolve("CUSTOM", "/x"),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
}

// R25, R27, R16
#[test]
fn hidden_method_before_rank_and_head() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("POST", "/fixed").unwrap();
    r.add("GET", "/:id").unwrap();
    assert_eq!(
        r.resolve("GET", "/fixed"),
        Ok(Match {
            route: 1,
            params: vec![("id".into(), "fixed".into())],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("HEAD", "/fixed"),
        Ok(Match {
            route: 1,
            params: vec![("id".into(), "fixed".into())],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("POST", "/fixed"),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
}

// R25, R27
#[test]
fn hidden_explicit_head_is_global() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/fixed").unwrap();
    r.add("HEAD", "/:key").unwrap();
    assert_eq!(
        r.resolve("HEAD", "/fixed"),
        Ok(Match {
            route: 1,
            params: vec![("key".into(), "fixed".into())],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/fixed"),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/absent"),
        Err(Error::MethodNotAllowed {
            allowed: vec!["HEAD".into()]
        })
    );
}

// R26, R25, R18
#[test]
fn hidden_allowed_is_complete_sorted_set() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("POST", "/v").unwrap();
    r.add("DELETE", "/:p").unwrap();
    r.add("GET", "/v").unwrap();
    r.add("HEAD", "/v").unwrap();
    r.add("PATCH", "/v").unwrap();
    assert_eq!(
        r.resolve("OPTIONS", "/v"),
        Err(Error::MethodNotAllowed {
            allowed: vec![
                "DELETE".into(),
                "GET".into(),
                "HEAD".into(),
                "PATCH".into(),
                "POST".into()
            ]
        })
    );
    assert_eq!(r.resolve("PUT", "/no/route"), Err(Error::NotFound));
}

// R23, R13, R16
#[test]
fn hidden_ignore_policy() {
    let mut r = Router::new(SlashPolicy::Ignore);
    r.add("GET", "/v/").unwrap();
    assert_eq!(
        r.resolve("GET", "/v"),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/v/"),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
    assert_eq!(r.resolve("GET", "/v//"), Err(Error::InvalidPath { at: 3 }));
}

// R24, R5, R8, R16
#[test]
fn hidden_redirect_policy() {
    let mut r = Router::new(SlashPolicy::Redirect);
    r.add("GET", "/%3A/:v/").unwrap();
    r.add("GET", "/flat").unwrap();
    r.add("GET", "/").unwrap();
    assert_eq!(
        r.resolve("GET", "/%3A/a%20b?x=2"),
        Ok(Match {
            route: 0,
            params: vec![("v".into(), "a b".into())],
            redirect: Some("/%3A/a%20b/".into())
        })
    );
    assert_eq!(
        r.resolve("GET", "/%3A/a%20b/"),
        Ok(Match {
            route: 0,
            params: vec![("v".into(), "a b".into())],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/flat/#f"),
        Ok(Match {
            route: 1,
            params: vec![],
            redirect: Some("/flat".into())
        })
    );
    assert_eq!(
        r.resolve("GET", "/"),
        Ok(Match {
            route: 2,
            params: vec![],
            redirect: None
        })
    );
}

// R24, R26, R25, R15
#[test]
fn hidden_redirect_winner_and_errors() {
    let mut r = Router::new(SlashPolicy::Redirect);
    r.add("GET", "/a/*w/").unwrap();
    r.add("POST", "/a/fixed").unwrap();
    assert_eq!(
        r.resolve("GET", "/a/fixed"),
        Ok(Match {
            route: 0,
            params: vec![("w".into(), "fixed".into())],
            redirect: Some("/a/fixed/".into())
        })
    );
    assert_eq!(
        r.resolve("PUT", "/a/fixed"),
        Err(Error::MethodNotAllowed {
            allowed: vec!["GET".into(), "HEAD".into(), "POST".into()]
        })
    );
    r.add("GET", "/*root/").unwrap();
    assert_eq!(
        r.resolve("GET", "/"),
        Ok(Match {
            route: 2,
            params: vec![("root".into(), "".into())],
            redirect: None
        })
    );
    r.add("GET", "/a/fixed").unwrap();
    assert_eq!(
        r.resolve("GET", "/a/fixed/"),
        Ok(Match {
            route: 3,
            params: vec![],
            redirect: Some("/a/fixed".into())
        })
    );
}
