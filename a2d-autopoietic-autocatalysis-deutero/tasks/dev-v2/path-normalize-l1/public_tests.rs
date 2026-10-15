// R1-R3, R8-R10, R14
#[test]
fn public_cleanup() {
    assert_eq!(normalize("//box///leaf/./file"), Ok("/box/leaf/file".into()));
    assert_eq!(normalize("one/../two"), Ok("two".into()));
}

// R14-R16
#[test]
fn public_directories() {
    assert_eq!(normalize("docs///"), Ok("docs/".into()));
    assert_eq!(normalize("////"), Ok("/".into()));
}

// R11, R13-R14
#[test]
fn public_relative() {
    assert_eq!(normalize("../notes/file"), Ok("../notes/file".into()));
    assert_eq!(normalize("a/.."), Ok(".".into()));
}

// R4-R6
#[test]
fn public_invalid() {
    assert_eq!(normalize(""), Err(PathError::Empty));
    assert_eq!(normalize("bad\\name"), Err(PathError::Backslash { at: 3 }));
}
