// R1-R26
#[test]
fn public_earlier_apis() {
    assert_eq!(normalize("//x/./y"), Ok("/x/y".into()));
    assert_eq!(resolve("~/x", "/cwd", Some("/home")), Ok("/home/x".into()));
}

// R27-R33
#[test]
fn public_absolute_link() {
    let links = vec![Link { path: "/short".into(), target: "/real/dir".into() }];
    assert_eq!(resolve_links("/short/file", "/", None, &links, 5), Ok("/real/dir/file".into()));
}

// R30-R34
#[test]
fn public_relative_link() {
    let links = vec![Link { path: "/work/current".into(), target: "../archive".into() }];
    assert_eq!(resolve_links("current/note/", "/work", None, &links, 5), Ok("/archive/note/".into()));
}

// R27, R36
#[test]
fn public_link_limit() {
    let links = vec![Link { path: "/shortcut".into(), target: "/destination".into() }];
    assert_eq!(resolve_links("/shortcut", "/", None, &links, 0), Err(PathError::Limit { index: 0 }));
    assert_eq!(resolve_links("/plain", "/", None, &links, 0), Ok("/plain".into()));
}
