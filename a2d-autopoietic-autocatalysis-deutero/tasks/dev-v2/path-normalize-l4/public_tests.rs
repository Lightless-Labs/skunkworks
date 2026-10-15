// R1-R37
#[test]
fn public_retained_apis() {
    assert_eq!(normalize("//p/./q"), Ok("/p/q".into()));
    assert_eq!(resolve("~/x", "/cwd", Some("/home")), Ok("/home/x".into()));
    let links = vec![Link { path: "/short".into(), target: "/real".into() }];
    assert_eq!(resolve_links("/short/file", "/", None, &links, 2), Ok("/real/file".into()));
}

// R38-R41
#[test]
fn public_relative_direct() {
    assert_eq!(relative_to("/work/code", "/work/docs/readme", "/", None, &[], 0), Ok("../docs/readme".into()));
    assert_eq!(relative_to("/same", "/same", "/", None, &[], 0), Ok(".".into()));
}

// R42-R46
#[test]
fn public_relative_shortcut() {
    let links = vec![Link { path: "/s".into(), target: "/long/destination".into() }];
    assert_eq!(relative_to("/", "/long/destination/file", "/", None, &links, 2), Ok("s/file".into()));
}

// R44, R47-R48
#[test]
fn public_relative_directory() {
    assert_eq!(relative_to("/a/b", "/a/c/", "/", None, &[], 0), Ok("../c/".into()));
}
