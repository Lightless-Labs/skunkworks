// R1, R2, R3, R17, R4
#[test]
fn hidden_empty_and_ids() {
    let mut r = Router::new();
    assert_eq!(r.resolve("/"), Err(Error::NotFound));
    assert_eq!(r.add("x"), Err(Error::InvalidPattern { at: 0 }));
    assert_eq!(r.add("/"), Ok(0));
    assert_eq!(
        r.resolve("/"),
        Ok(Match {
            route: 0,
            params: vec![]
        })
    );
    assert_eq!(
        r.resolve("/"),
        Ok(Match {
            route: 0,
            params: vec![]
        })
    );
}

// R4, R12
#[test]
fn hidden_absolute_inputs() {
    let mut r = Router::new();
    assert_eq!(r.resolve(""), Err(Error::InvalidPath { at: 0 }));
    assert_eq!(r.resolve("abc"), Err(Error::InvalidPath { at: 0 }));
    assert_eq!(r.resolve("?ignored"), Err(Error::InvalidPath { at: 0 }));
    assert_eq!(r.resolve("relative/path"), Err(Error::InvalidPath { at: 0 }));
    assert_eq!(r.add(""), Err(Error::InvalidPattern { at: 0 }));
}

// R5, R8, R19
#[test]
fn hidden_suffix_is_removed_first() {
    let mut r = Router::new();
    r.add("/doc%3Fv%23x").unwrap();
    assert_eq!(
        r.resolve("/doc%3Fv%23x?bad=%zz#é"),
        Ok(Match {
            route: 0,
            params: vec![]
        })
    );
    assert_eq!(
        r.resolve("/doc%3Fv%23x#anything?x"),
        Ok(Match {
            route: 0,
            params: vec![]
        })
    );
    assert_eq!(r.add("/doc?x"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("/doc#x"), Err(Error::InvalidPattern { at: 4 }));
}

// R6, R12, R13
#[test]
fn hidden_empty_segments_and_positions() {
    let mut r = Router::new();
    assert_eq!(r.resolve("//"), Err(Error::InvalidPath { at: 1 }));
    assert_eq!(r.resolve("/ab//cd"), Err(Error::InvalidPath { at: 4 }));
    assert_eq!(r.resolve("/ab//"), Err(Error::InvalidPath { at: 4 }));
    assert_eq!(r.resolve("///x"), Err(Error::InvalidPath { at: 1 }));
    assert_eq!(r.add("/t//v"), Err(Error::InvalidPattern { at: 3 }));
}

// R7, R19, R6
#[test]
fn hidden_case_and_dot_segments() {
    let mut r = Router::new();
    r.add("/X/../.").unwrap();
    assert_eq!(
        r.resolve("/X/../."),
        Ok(Match {
            route: 0,
            params: vec![]
        })
    );
    assert_eq!(r.resolve("/x/../."), Err(Error::NotFound));
    assert_eq!(r.resolve("/."), Err(Error::NotFound));
    assert_eq!(r.resolve("/X"), Err(Error::NotFound));
}

// R7, R12
#[test]
fn hidden_raw_byte_positions() {
    let mut r = Router::new();
    assert_eq!(r.resolve("/abc d"), Err(Error::InvalidPath { at: 4 }));
    assert_eq!(r.resolve("/é"), Err(Error::InvalidPath { at: 1 }));
    assert_eq!(r.resolve("/ab/:x"), Err(Error::InvalidPath { at: 4 }));
    assert_eq!(r.resolve("/x/*"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("/a/\t"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.add("/ok/abé"), Err(Error::InvalidPattern { at: 6 }));
}

// R8, R9, R14
#[test]
fn hidden_decoded_punctuation() {
    let mut r = Router::new();
    r.add("/%3A/:value").unwrap();
    assert_eq!(
        r.resolve("/%3A/%20%25%3F%23%2A"),
        Ok(Match {
            route: 0,
            params: vec![("value".into(), " %?#*".into())]
        })
    );
    assert_eq!(r.resolve("/%3a/x"), Err(Error::InvalidPath { at: 1 }));
}

// R8, R12
#[test]
fn hidden_escape_rejection_matrix() {
    let mut r = Router::new();
    assert_eq!(r.resolve("/v/%"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("/v/%2"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("/v/%0G"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("/v/%gg"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("/v/%2f"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("/v/%2F"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("/v/%41"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("/v/%7E"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("/v/%00"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("/v/%1F"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("/v/%7F"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("/v/%80"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.resolve("/v/%FF"), Err(Error::InvalidPath { at: 3 }));
    assert_eq!(r.add("/a/%61"), Err(Error::InvalidPattern { at: 3 }));
}

// R9, R10, R12
#[test]
fn hidden_capture_grammar() {
    let mut r = Router::new();
    assert_eq!(r.add("/ok/:"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("/ok/:A"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("/ok/:0a"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("/ok/:a-b"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("/ok/:a%GG"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("/ok/:a:b"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.add("/ok/:é"), Err(Error::InvalidPattern { at: 4 }));
    r.add("/ok/:a_9").unwrap();
    assert_eq!(
        r.resolve("/ok/z"),
        Ok(Match {
            route: 0,
            params: vec![("a_9".into(), "z".into())]
        })
    );
}

// R10, R3
#[test]
fn hidden_duplicate_capture_namespace() {
    let mut r = Router::new();
    assert_eq!(r.add("/:same/:same"), Err(Error::InvalidPattern { at: 7 }));
    assert_eq!(r.add("/:a/:b"), Ok(0));
}

// R12, R10, R8, R17
#[test]
fn hidden_validation_precedence() {
    let mut r = Router::new();
    assert_eq!(r.add("/%GG/:bad-"), Err(Error::InvalidPattern { at: 1 }));
    assert_eq!(r.add("/:bad-/%GG"), Err(Error::InvalidPattern { at: 1 }));
    assert_eq!(r.resolve("/unmatched/%GG/"), Err(Error::InvalidPath { at: 11 }));
    assert_eq!(r.add("/%GG/"), Err(Error::InvalidPattern { at: 1 }));
}

// R15
#[test]
fn hidden_specificity_is_lexicographic() {
    let mut r = Router::new();
    r.add("/:a/b/c").unwrap();
    r.add("/fixed/:b/:c").unwrap();
    assert_eq!(
        r.resolve("/fixed/b/c"),
        Ok(Match {
            route: 1,
            params: vec![("b".into(), "b".into()), ("c".into(), "c".into())]
        })
    );
}

// R16, R18, R3
#[test]
fn hidden_earliest_registration() {
    let mut r = Router::new();
    r.add("/:z/k").unwrap();
    r.add("/:a/k").unwrap();
    r.add("/:z/k").unwrap();
    assert_eq!(
        r.resolve("/v/k"),
        Ok(Match {
            route: 0,
            params: vec![("z".into(), "v".into())]
        })
    );
}

// R14, R3
#[test]
fn hidden_capture_order_and_repeated_resolution() {
    let mut r = Router::new();
    r.add("/:z/m/:a").unwrap();
    assert_eq!(
        r.resolve("/left/m/right"),
        Ok(Match {
            route: 0,
            params: vec![("z".into(), "left".into()), ("a".into(), "right".into())]
        })
    );
    assert_eq!(
        r.resolve("/other/m/end"),
        Ok(Match {
            route: 0,
            params: vec![("z".into(), "other".into()), ("a".into(), "end".into())]
        })
    );
}

// R15, R17, R19
#[test]
fn hidden_whole_path_only() {
    let mut r = Router::new();
    r.add("/a/:v").unwrap();
    assert_eq!(r.resolve("/a"), Err(Error::NotFound));
    assert_eq!(r.resolve("/a/b/c"), Err(Error::NotFound));
    assert_eq!(r.resolve("/aa/b"), Err(Error::NotFound));
}

// R11, R19, R8
#[test]
fn hidden_escaped_markers_are_literal() {
    let mut r = Router::new();
    r.add("/%3Aid/%2Atail").unwrap();
    assert_eq!(
        r.resolve("/%3Aid/%2Atail"),
        Ok(Match {
            route: 0,
            params: vec![]
        })
    );
    assert_eq!(r.resolve("/hello/there"), Err(Error::NotFound));
}

// R8, R19
#[test]
fn hidden_single_decoding() {
    let mut r = Router::new();
    r.add("/%252F/:v").unwrap();
    assert_eq!(
        r.resolve("/%252F/%2520"),
        Ok(Match {
            route: 0,
            params: vec![("v".into(), "%20".into())]
        })
    );
    assert_eq!(r.resolve("/%2F/a"), Err(Error::InvalidPath { at: 1 }));
}

// R13, R12
#[test]
fn hidden_trailing_slashes() {
    let mut r = Router::new();
    assert_eq!(r.add("/end/"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(r.resolve("/end/"), Err(Error::InvalidPath { at: 4 }));
    r.add("/").unwrap();
    assert_eq!(
        r.resolve("/?query"),
        Ok(Match {
            route: 0,
            params: vec![]
        })
    );
}

// R11, R10
#[test]
fn hidden_wildcards_rejected() {
    let mut r = Router::new();
    assert_eq!(r.add("/a/*rest"), Err(Error::InvalidPattern { at: 3 }));
    assert_eq!(r.add("/*"), Err(Error::InvalidPattern { at: 1 }));
    assert_eq!(r.add("/*x/a"), Err(Error::InvalidPattern { at: 1 }));
}

// R3, R15, R17
#[test]
fn hidden_larger_route_table() {
    let mut r = Router::new();
    r.add("/table/0").unwrap();
    r.add("/table/1").unwrap();
    r.add("/table/2").unwrap();
    r.add("/table/3").unwrap();
    r.add("/table/4").unwrap();
    r.add("/table/5").unwrap();
    r.add("/table/6").unwrap();
    r.add("/table/7").unwrap();
    r.add("/table/8").unwrap();
    r.add("/table/9").unwrap();
    r.add("/table/10").unwrap();
    r.add("/table/11").unwrap();
    r.add("/table/12").unwrap();
    r.add("/table/13").unwrap();
    r.add("/table/14").unwrap();
    r.add("/table/15").unwrap();
    r.add("/table/16").unwrap();
    r.add("/table/17").unwrap();
    r.add("/table/18").unwrap();
    r.add("/table/19").unwrap();
    r.add("/table/20").unwrap();
    r.add("/table/21").unwrap();
    r.add("/table/22").unwrap();
    r.add("/table/23").unwrap();
    r.add("/table/24").unwrap();
    r.add("/table/25").unwrap();
    r.add("/table/26").unwrap();
    r.add("/table/27").unwrap();
    r.add("/table/28").unwrap();
    r.add("/table/29").unwrap();
    r.add("/table/30").unwrap();
    r.add("/table/31").unwrap();
    r.add("/table/32").unwrap();
    r.add("/table/33").unwrap();
    r.add("/table/34").unwrap();
    r.add("/table/35").unwrap();
    r.add("/table/36").unwrap();
    r.add("/table/37").unwrap();
    r.add("/table/38").unwrap();
    r.add("/table/39").unwrap();
    r.add("/table/40").unwrap();
    r.add("/table/41").unwrap();
    r.add("/table/42").unwrap();
    r.add("/table/43").unwrap();
    r.add("/table/44").unwrap();
    r.add("/table/45").unwrap();
    r.add("/table/46").unwrap();
    r.add("/table/47").unwrap();
    r.add("/table/:n").unwrap();
    assert_eq!(
        r.resolve("/table/37"),
        Ok(Match {
            route: 37,
            params: vec![]
        })
    );
    assert_eq!(
        r.resolve("/table/unknown"),
        Ok(Match {
            route: 48,
            params: vec![("n".into(), "unknown".into())]
        })
    );
}
