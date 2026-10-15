// R1-R2, R9-R10
#[test]
fn public_normalize_again() {
    assert_eq!(normalize("//p/./q"), Ok("/p/q".into()));
}

// R18-R19, R23-R25
#[test]
fn public_cwd() {
    assert_eq!(resolve("../file", "/work/source", None), Ok("/work/file".into()));
    assert_eq!(resolve("/log/", "/work", None), Ok("/log/".into()));
}

// R20, R22-R25
#[test]
fn public_home() {
    assert_eq!(resolve("~/notes", "/work", Some("/users/me")), Ok("/users/me/notes".into()));
    assert_eq!(resolve("~", "/work", Some("/users/me")), Ok("/users/me".into()));
}

// R19-R21
#[test]
fn public_context_error() {
    assert_eq!(resolve("file", "work", None), Err(PathError::InvalidCwd));
    assert_eq!(resolve("~", "/", None), Err(PathError::MissingHome));
}
