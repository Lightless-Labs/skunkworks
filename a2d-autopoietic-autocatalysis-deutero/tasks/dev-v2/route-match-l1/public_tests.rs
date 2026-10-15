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

// R9, R14, R17
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

// R15, R12
#[test]
fn public_static_priority() {
    let mut r = Router::new();
    r.add("/item/:id").unwrap();
    r.add("/item/new").unwrap();
    assert_eq!(
        r.resolve("/item/new"),
        Ok(Match {
            route: 1,
            params: vec![]
        })
    );
    assert_eq!(r.add("bad"), Err(Error::InvalidPattern { at: 0 }));
}
