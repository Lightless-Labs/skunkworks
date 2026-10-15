// R1, R2, R3, R18, R4
#[test]
fn hidden_empty_and_ids() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.resolve("GET", "/"), Err(Error::NotFound));
    assert_eq!(r.add("n6", "GET", "x"), Err(Error::InvalidPattern { at: 0 }));
    assert_eq!(r.add("n7", "GET", "/"), Ok(0));
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
    assert_eq!(r.add("n8", "GET", ""), Err(Error::InvalidPattern { at: 0 }));

    {
        // R7, R12
        let mut r = Router::new(SlashPolicy::Strict);
        assert_eq!(r.resolve("GET", "/abc d"), Err(Error::InvalidPath { at: 4 }));
        assert_eq!(r.resolve("GET", "/é"), Err(Error::InvalidPath { at: 1 }));
        assert_eq!(r.resolve("GET", "/ab/:x"), Err(Error::InvalidPath { at: 4 }));
        assert_eq!(r.resolve("GET", "/x/*"), Err(Error::InvalidPath { at: 3 }));
        assert_eq!(r.resolve("GET", "/a/\t"), Err(Error::InvalidPath { at: 3 }));
        assert_eq!(
            r.add("n14", "GET", "/ok/abé"),
            Err(Error::InvalidPattern { at: 6 })
        );
    }
}

// R5, R8, R19
#[test]
fn hidden_suffix_is_removed_first() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("n9", "GET", "/doc%3Fv%23x").unwrap();
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
    assert_eq!(
        r.add("n10", "GET", "/doc?x"),
        Err(Error::InvalidPattern { at: 4 })
    );
    assert_eq!(
        r.add("n11", "GET", "/doc#x"),
        Err(Error::InvalidPattern { at: 4 })
    );
}

// R6, R12, R13
#[test]
fn hidden_empty_segments_and_positions() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.resolve("GET", "//"), Err(Error::InvalidPath { at: 1 }));
    assert_eq!(r.resolve("GET", "/ab//cd"), Err(Error::InvalidPath { at: 4 }));
    assert_eq!(r.resolve("GET", "/ab//"), Err(Error::InvalidPath { at: 4 }));
    assert_eq!(r.resolve("GET", "///x"), Err(Error::InvalidPath { at: 1 }));
    assert_eq!(r.add("n12", "GET", "/t//v"), Err(Error::InvalidPattern { at: 3 }));
}

// R7, R19, R6
#[test]
fn hidden_case_and_dot_segments() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("n13", "GET", "/X/../.").unwrap();
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

    {
        // R16, R18, R19
        let mut r = Router::new(SlashPolicy::Strict);
        r.add("n37", "GET", "/a/:v").unwrap();
        assert_eq!(r.resolve("GET", "/a"), Err(Error::NotFound));
        assert_eq!(r.resolve("GET", "/a/b/c"), Err(Error::NotFound));
        assert_eq!(r.resolve("GET", "/aa/b"), Err(Error::NotFound));
    }
}

// R8, R9, R14
#[test]
fn hidden_decoded_punctuation() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("n15", "GET", "/%3A/:value").unwrap();
    assert_eq!(
        r.resolve("GET", "/%3A/%20%25%3F%23%2A"),
        Ok(Match {
            route: 0,
            params: vec![("value".into(), " %?#*".into())],
            redirect: None
        })
    );
    assert_eq!(r.resolve("GET", "/%3a/x"), Err(Error::InvalidPath { at: 1 }));

    {
        // R8, R19
        let mut r = Router::new(SlashPolicy::Strict);
        r.add("n39", "GET", "/%252F/:v").unwrap();
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
    assert_eq!(
        r.add("n16", "GET", "/a/%61"),
        Err(Error::InvalidPattern { at: 3 })
    );
}

// R9, R10, R12
#[test]
fn hidden_capture_grammar() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.add("n17", "GET", "/ok/:"), Err(Error::InvalidPattern { at: 4 }));
    assert_eq!(
        r.add("n18", "GET", "/ok/:A"),
        Err(Error::InvalidPattern { at: 4 })
    );
    assert_eq!(
        r.add("n19", "GET", "/ok/:0a"),
        Err(Error::InvalidPattern { at: 4 })
    );
    assert_eq!(
        r.add("n20", "GET", "/ok/:a-b"),
        Err(Error::InvalidPattern { at: 4 })
    );
    assert_eq!(
        r.add("n21", "GET", "/ok/:a%GG"),
        Err(Error::InvalidPattern { at: 4 })
    );
    assert_eq!(
        r.add("n22", "GET", "/ok/:a:b"),
        Err(Error::InvalidPattern { at: 4 })
    );
    assert_eq!(
        r.add("n23", "GET", "/ok/:é"),
        Err(Error::InvalidPattern { at: 4 })
    );
    r.add("n24", "GET", "/ok/:a_9").unwrap();
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
    assert_eq!(
        r.add("n25", "GET", "/:same/:same"),
        Err(Error::InvalidPattern { at: 7 })
    );
    assert_eq!(
        r.add("n26", "GET", "/:same/*same"),
        Err(Error::InvalidPattern { at: 7 })
    );
    assert_eq!(r.add("n27", "GET", "/:a/:b"), Ok(0));
}

// R12, R10, R8, R18
#[test]
fn hidden_validation_precedence() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(
        r.add("n28", "GET", "/%GG/:bad-"),
        Err(Error::InvalidPattern { at: 1 })
    );
    assert_eq!(
        r.add("n29", "GET", "/:bad-/%GG"),
        Err(Error::InvalidPattern { at: 1 })
    );
    assert_eq!(
        r.resolve("GET", "/unmatched/%GG/"),
        Err(Error::InvalidPath { at: 11 })
    );
    assert_eq!(r.add("n30", "GET", "/%GG/"), Err(Error::InvalidPattern { at: 1 }));
}

// R16
#[test]
fn hidden_specificity_is_lexicographic() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("n31", "GET", "/:a/b/c").unwrap();
    r.add("n32", "GET", "/fixed/:b/:c").unwrap();
    assert_eq!(
        r.resolve("GET", "/fixed/b/c"),
        Ok(Match {
            route: 1,
            params: vec![("b".into(), "b".into()), ("c".into(), "c".into())],
            redirect: None
        })
    );
}

// R17, R3, R29, R30
#[test]
fn hidden_atomic_conflicts_and_ties() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("first", "GET", "/:z/k").unwrap();
    assert_eq!(r.add("second", "GET", "/:a/k"), Err(Error::Conflict { with: 0 }));
    assert_eq!(r.add("second", "GET", "/fresh"), Ok(1));
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
    r.add("n36", "GET", "/:z/m/:a").unwrap();
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

// R13, R21, R25, R14
#[test]
fn hidden_strict_slashes() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("n40", "GET", "/end").unwrap();
    r.add("n41", "POST", "/end/").unwrap();
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
    r.add("n42", "GET", "/only/").unwrap();
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
    assert_eq!(
        r.add("n43", "GET", "/a/*rest/z"),
        Err(Error::InvalidPattern { at: 3 })
    );
    assert_eq!(r.add("n44", "GET", "/a/*R"), Err(Error::InvalidPattern { at: 3 }));
    assert_eq!(
        r.add("n45", "GET", "/a/*/z"),
        Err(Error::InvalidPattern { at: 3 })
    );
    assert_eq!(
        r.add("n46", "GET", "/*rest/%GG"),
        Err(Error::InvalidPattern { at: 1 })
    );

    {
        // R11, R19, R8
        let mut r = Router::new(SlashPolicy::Strict);
        r.add("n38", "GET", "/%3Aid/%2Atail").unwrap();
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
}

// R3, R16, R18
#[test]
fn hidden_larger_route_table() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("n47", "GET", "/table/0").unwrap();
    r.add("n48", "GET", "/table/1").unwrap();
    r.add("n49", "GET", "/table/2").unwrap();
    r.add("n50", "GET", "/table/3").unwrap();
    r.add("n51", "GET", "/table/4").unwrap();
    r.add("n52", "GET", "/table/5").unwrap();
    r.add("n53", "GET", "/table/6").unwrap();
    r.add("n54", "GET", "/table/7").unwrap();
    r.add("n55", "GET", "/table/8").unwrap();
    r.add("n56", "GET", "/table/9").unwrap();
    r.add("n57", "GET", "/table/10").unwrap();
    r.add("n58", "GET", "/table/11").unwrap();
    r.add("n59", "GET", "/table/12").unwrap();
    r.add("n60", "GET", "/table/13").unwrap();
    r.add("n61", "GET", "/table/14").unwrap();
    r.add("n62", "GET", "/table/15").unwrap();
    r.add("n63", "GET", "/table/16").unwrap();
    r.add("n64", "GET", "/table/17").unwrap();
    r.add("n65", "GET", "/table/18").unwrap();
    r.add("n66", "GET", "/table/19").unwrap();
    r.add("n67", "GET", "/table/20").unwrap();
    r.add("n68", "GET", "/table/21").unwrap();
    r.add("n69", "GET", "/table/22").unwrap();
    r.add("n70", "GET", "/table/23").unwrap();
    r.add("n71", "GET", "/table/24").unwrap();
    r.add("n72", "GET", "/table/25").unwrap();
    r.add("n73", "GET", "/table/26").unwrap();
    r.add("n74", "GET", "/table/27").unwrap();
    r.add("n75", "GET", "/table/28").unwrap();
    r.add("n76", "GET", "/table/29").unwrap();
    r.add("n77", "GET", "/table/30").unwrap();
    r.add("n78", "GET", "/table/31").unwrap();
    r.add("n79", "GET", "/table/32").unwrap();
    r.add("n80", "GET", "/table/33").unwrap();
    r.add("n81", "GET", "/table/34").unwrap();
    r.add("n82", "GET", "/table/35").unwrap();
    r.add("n83", "GET", "/table/36").unwrap();
    r.add("n84", "GET", "/table/37").unwrap();
    r.add("n85", "GET", "/table/38").unwrap();
    r.add("n86", "GET", "/table/39").unwrap();
    r.add("n87", "GET", "/table/40").unwrap();
    r.add("n88", "GET", "/table/41").unwrap();
    r.add("n89", "GET", "/table/42").unwrap();
    r.add("n90", "GET", "/table/43").unwrap();
    r.add("n91", "GET", "/table/44").unwrap();
    r.add("n92", "GET", "/table/45").unwrap();
    r.add("n93", "GET", "/table/46").unwrap();
    r.add("n94", "GET", "/table/47").unwrap();
    r.add("n95", "GET", "/table/:n").unwrap();
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
    r.add("n96", "GET", "/tree/*rest").unwrap();
    r.add("n97", "GET", "/tree").unwrap();
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
    r.add("n98", "GET", "/x/*tail").unwrap();
    r.add("n99", "GET", "/x/:id").unwrap();
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
    r.add("n100", "GET", "/:v/fixed/end").unwrap();
    r.add("n101", "GET", "/root/*all").unwrap();
    r.add("n102", "GET", "/*any").unwrap();
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

// R20, R12
#[test]
fn hidden_method_validation_order() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.resolve("", "bad"), Err(Error::InvalidMethod));
    assert_eq!(r.resolve("get", "bad"), Err(Error::InvalidMethod));
    assert_eq!(r.resolve("GET1", "bad"), Err(Error::InvalidMethod));
    assert_eq!(r.resolve("GÉT", "bad"), Err(Error::InvalidMethod));
    assert_eq!(r.resolve("GET-", "bad"), Err(Error::InvalidMethod));
    assert_eq!(r.add("n103", "get", "bad"), Err(Error::InvalidMethod));
    r.add("n104", "CUSTOM", "/x").unwrap();
    assert_eq!(
        r.resolve("CUSTOM", "/x"),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
}

// R24, R26, R16
#[test]
fn hidden_method_before_rank_and_head() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("n105", "POST", "/fixed").unwrap();
    r.add("n106", "GET", "/:id").unwrap();
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

// R24, R26
#[test]
fn hidden_explicit_head_is_global() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("n107", "GET", "/fixed").unwrap();
    r.add("n108", "HEAD", "/:key").unwrap();
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

// R25, R24, R18
#[test]
fn hidden_allowed_is_complete_sorted_set() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("n109", "POST", "/v").unwrap();
    r.add("n110", "DELETE", "/:p").unwrap();
    r.add("n111", "GET", "/v").unwrap();
    r.add("n112", "HEAD", "/v").unwrap();
    r.add("n113", "PATCH", "/v").unwrap();
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

// R22, R13, R16
#[test]
fn hidden_ignore_policy() {
    let mut r = Router::new(SlashPolicy::Ignore);
    r.add("n114", "GET", "/v/").unwrap();
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

// R23, R5, R8, R16
#[test]
fn hidden_redirect_policy() {
    let mut r = Router::new(SlashPolicy::Redirect);
    r.add("n115", "GET", "/%3A/:v/").unwrap();
    r.add("n116", "GET", "/flat").unwrap();
    r.add("n117", "GET", "/").unwrap();
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

// R23, R25, R24, R15
#[test]
fn hidden_redirect_winner_and_errors() {
    let mut r = Router::new(SlashPolicy::Redirect);
    r.add("n118", "GET", "/a/*w/").unwrap();
    r.add("n119", "POST", "/a/fixed").unwrap();
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
    r.add("n120", "GET", "/*root/").unwrap();
    assert_eq!(
        r.resolve("GET", "/"),
        Ok(Match {
            route: 2,
            params: vec![("root".into(), "".into())],
            redirect: None
        })
    );
    r.add("n121", "GET", "/a/fixed").unwrap();
    assert_eq!(
        r.resolve("GET", "/a/fixed/"),
        Ok(Match {
            route: 3,
            params: vec![],
            redirect: Some("/a/fixed".into())
        })
    );
}

// R20, R27, R28, R3
#[test]
fn hidden_registration_error_precedence() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("taken", "GET", "/taken").unwrap();
    assert_eq!(r.add("Bad", "get", "bad"), Err(Error::InvalidMethod));
    assert_eq!(r.add("Bad", "GET", "bad"), Err(Error::InvalidName));
    assert_eq!(r.add("taken", "GET", "bad"), Err(Error::InvalidPattern { at: 0 }));
    assert_eq!(r.add("taken", "GET", "/taken"), Err(Error::DuplicateName));
    assert_eq!(r.add("taken", "POST", "/other"), Err(Error::DuplicateName));
    assert_eq!(r.add("", "GET", "/x"), Err(Error::InvalidName));
    assert_eq!(r.add("9x", "GET", "/x"), Err(Error::InvalidName));
    assert_eq!(r.add("A", "GET", "/x"), Err(Error::InvalidName));
    assert_eq!(r.add("a.b", "GET", "/x"), Err(Error::InvalidName));
    assert_eq!(r.add("a b", "GET", "/x"), Err(Error::InvalidName));
    assert_eq!(r.add("é", "GET", "/x"), Err(Error::InvalidName));
    assert_eq!(r.add("a_9-z", "GET", "/free"), Ok(1));
}

// R29, R30, R3, R27, R28
#[test]
fn hidden_structural_conflicts_and_policies() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("one", "GET", "/a/:x").unwrap();
    assert_eq!(r.add("two", "GET", "/a/:y"), Err(Error::Conflict { with: 0 }));
    r.add("post", "POST", "/a/:x").unwrap();
    r.add("slash", "GET", "/a/:x/").unwrap();
    r.add("wild", "GET", "/a/*x").unwrap();
    r.add("static", "GET", "/a/fixed").unwrap();
    r.add("head", "HEAD", "/a/:x").unwrap();
    assert_eq!(
        r.resolve("GET", "/a/fixed"),
        Ok(Match {
            route: 4,
            params: vec![],
            redirect: None
        })
    );
    let mut r = Router::new(SlashPolicy::Ignore);
    r.add("tail", "GET", "/b/*tail/").unwrap();
    assert_eq!(r.add("rest", "GET", "/b/*rest"), Err(Error::Conflict { with: 0 }));
    let mut r = Router::new(SlashPolicy::Redirect);
    r.add("escaped", "GET", "/%3A/:x").unwrap();
    assert_eq!(
        r.add("again", "GET", "/%3A/:z/"),
        Err(Error::Conflict { with: 0 })
    );
}

// R31, R32, R33, R34, R35
#[test]
fn hidden_build_error_precedence() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("pair", "GET", "/:z/:a").unwrap();
    assert_eq!(r.build("BAD", &[("q", ""), ("q", "")]), Err(Error::UnknownName));
    assert_eq!(
        r.build("pair", &[("q", ""), ("q", "")]),
        Err(Error::DuplicateParam { name: "q".into() })
    );
    assert_eq!(
        r.build("pair", &[]),
        Err(Error::MissingParam { name: "z".into() })
    );
    assert_eq!(
        r.build("pair", &[("q", ""), ("a", "")]),
        Err(Error::MissingParam { name: "z".into() })
    );
    assert_eq!(
        r.build("pair", &[("z", ""), ("a", "x"), ("zz", ""), ("bb", "")]),
        Err(Error::ExtraParam { name: "bb".into() })
    );
    assert_eq!(
        r.build("pair", &[("a", "/"), ("z", "")]),
        Err(Error::InvalidValue { name: "z".into() })
    );
    assert_eq!(
        r.build("pair", &[("z", "v"), ("a", "x"), ("z", "v"), ("a", "x")]),
        Err(Error::DuplicateParam { name: "z".into() })
    );
}

// R36, R35, R37, R14
#[test]
fn hidden_build_encoding_and_input_order() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("url", "GET", "/%3A/:z/*a/").unwrap();
    assert_eq!(
        r.build("url", &[("a", "q r/%2F/.."), ("z", "?#%*")]),
        Ok("/%3A/%3F%23%25%2A/q%20r/%252F/../".into())
    );
    let path = r.build("url", &[("z", "?#%*"), ("a", "q r/%2F/..")]).unwrap();
    let found = r.resolve("GET", &path).unwrap();
    assert_eq!(
        found.params,
        vec![("z".into(), "?#%*".into()), ("a".into(), "q r/%2F/..".into())]
    );
}

// R35, R33, R36, R37, R22
#[test]
fn hidden_build_values_and_empty_tail() {
    let mut r = Router::new(SlashPolicy::Ignore);
    r.add("mixed", "GET", "/a/:v/*tail/").unwrap();
    r.add("exact", "GET", "/a/x/").unwrap();
    for value in ["", "/", "a/b", "é", "\n", "\u{7f}"] {
        assert_eq!(
            r.build("mixed", &[("v", value), ("tail", "")]),
            Err(Error::InvalidValue { name: "v".into() })
        );
    }
    for value in ["/x", "x/", "x//y", "\t", "é"] {
        assert_eq!(
            r.build("mixed", &[("v", "x"), ("tail", value)]),
            Err(Error::InvalidValue { name: "tail".into() })
        );
    }
    assert_eq!(
        r.build("mixed", &[("v", "x")]),
        Err(Error::MissingParam { name: "tail".into() })
    );
    assert_eq!(r.build("mixed", &[("v", "x"), ("tail", "")]), Ok("/a/x/".into()));
    assert_eq!(r.resolve("GET", "/a/x").unwrap().route, 1);
}

// R34, R36, R37, R31, R3
#[test]
fn hidden_build_static_root_and_method_independence() {
    let mut r = Router::new(SlashPolicy::Redirect);
    r.add("root_wild", "POST", "/*tail/").unwrap();
    r.add("root", "GET", "/").unwrap();
    r.add("static", "GET", "/%20._~-09AZaz/").unwrap();
    assert_eq!(r.build("root_wild", &[("tail", "")]), Ok("/".into()));
    assert_eq!(r.build("root", &[]), Ok("/".into()));
    assert_eq!(r.build("static", &[]), Ok("/%20._~-09AZaz/".into()));
    assert_eq!(
        r.build("root", &[("z", ""), ("", "")]),
        Err(Error::ExtraParam { name: "".into() })
    );
    assert_eq!(r.build("Root", &[]), Err(Error::UnknownName));
    assert_eq!(r.resolve("POST", "/").unwrap().route, 0);
    assert_eq!(r.resolve("GET", "/").unwrap().route, 1);
    assert_eq!(r.build("root", &[]), Ok("/".into()));
}
