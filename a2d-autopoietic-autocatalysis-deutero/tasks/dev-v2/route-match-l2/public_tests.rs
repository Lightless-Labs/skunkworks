// R1, R2, R3, R4, R14
#[test]
fn public_basic_static() {
    let mut r = Router::new();
    assert_eq!(r.add("/status"), Ok(0));
    assert_eq!(
        r.resolve("/status"),
        Ok(Match {
            route: 0,
            params: vec![]
        })
    );
}

// R9, R14, R18
#[test]
fn public_basic_parameter() {
    let mut r = Router::new();
    r.add("/people/:id").unwrap();
    assert_eq!(
        r.resolve("/people/42"),
        Ok(Match {
            route: 0,
            params: vec![("id".into(), "42".into())]
        })
    );
    assert_eq!(r.resolve("/nowhere"), Err(Error::NotFound));
}

// R11, R15, R16
#[test]
fn public_wildcard_basic() {
    let mut r = Router::new();
    r.add("/assets/*tail").unwrap();
    r.add("/assets/icon").unwrap();
    assert_eq!(
        r.resolve("/assets/a/b"),
        Ok(Match {
            route: 0,
            params: vec![("tail".into(), "a/b".into())]
        })
    );
    assert_eq!(
        r.resolve("/assets/icon"),
        Ok(Match {
            route: 1,
            params: vec![]
        })
    );
}

// R8
#[test]
fn public_decoded_value() {
    let mut r = Router::new();
    r.add("/space/:x").unwrap();
    assert_eq!(
        r.resolve("/space/a%20b"),
        Ok(Match {
            route: 0,
            params: vec![("x".into(), "a b".into())]
        })
    );
}
