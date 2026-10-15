// R1, R2, R3, R4, R14
#[test]
fn public_basic_static() {
    let mut r = Router::new(SlashPolicy::Strict);
    assert_eq!(r.add("status", "GET", "/status"), Ok(0));
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
    r.add("person", "GET", "/people/:id").unwrap();
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
    r.add("asset", "GET", "/assets/*tail").unwrap();
    r.add("icon", "GET", "/assets/icon").unwrap();
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

// R36, R29, R2
#[test]
fn public_build_and_conflict() {
    let mut r = Router::new(SlashPolicy::Strict);
    r.add("user", "GET", "/users/:id").unwrap();
    assert_eq!(r.build("user", &[("id", "A B")]), Ok("/users/A%20B".into()));
    assert_eq!(
        r.add("other", "GET", "/users/:other"),
        Err(Error::Conflict { with: 0 })
    );
}
