// R1, R2, R3, R4, R14
#[test]
fn public_basic_static() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.add("GET", "/status"), Ok(0));
    assert_eq!(
        r.resolve("GET", "/status"),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
}

// R9, R14, R18
#[test]
fn public_basic_parameter() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/people/:id").unwrap();
    assert_eq!(
        r.resolve("GET", "/people/42"),
        Ok(Match {
            route: 0,
            params: vec![("id".into(), "42".into())],
            redirect: None
        })
    );
    assert_eq!(r.resolve("GET", "/nowhere"), Err(Error::NotFound));
}

// R11, R15, R16
#[test]
fn public_wildcard_basic() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("GET", "/assets/*tail").unwrap();
    r.add("GET", "/assets/icon").unwrap();
    assert_eq!(
        r.resolve("GET", "/assets/a/b"),
        Ok(Match {
            route: 0,
            params: vec![("tail".into(), "a/b".into())],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/assets/icon"),
        Ok(Match {
            route: 1,
            params: vec![],
            redirect: None
        })
    );
}

// R21, R23, R26
#[test]
fn public_method_and_slash() {
    let mut r = Router::new(SlashPolicy::Ignore);
    r.add("POST", "/submit/").unwrap();
    assert_eq!(
        r.resolve("POST", "/submit"),
        Ok(Match {
            route: 0,
            params: vec![],
            redirect: None
        })
    );
    assert_eq!(
        r.resolve("GET", "/submit"),
        Err(Error::MethodNotAllowed {
            allowed: vec!["POST".into()]
        })
    );
}
