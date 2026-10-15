#[test]
fn spec_r1_api_surface() {
    let mut fs = Fs::new();
    assert_eq!(fs.cwd(), "/");
    assert_eq!(fs.mkdir("/api"), Ok(()));
    assert_eq!(fs.write("/api/file", b"x"), Ok(()));
    assert_eq!(fs.read("/api/file"), Ok(b"x".to_vec()));
    assert_eq!(fs.ls("/api"), Ok(vec![("file".to_string(), Kind::File)]));
    assert_eq!(fs.chdir("/api"), Ok(()));
}

#[test]
fn spec_r2_root_and_independent_instances() {
    let mut first = Fs::new();
    let second = Fs::new();

    assert_eq!(first.ls("/"), Ok(vec![]));
    first.mkdir("/only-first").unwrap();
    assert_eq!(second.ls("/"), Ok(vec![]));
    assert_eq!(second.ls("/only-first"), Err(Error::Missing("/only-first".into())));
}

#[test]
fn spec_r3_nul_precedes_other_path_defects() {
    let mut fs = Fs::new();

    assert_eq!(fs.read(""), Err(Error::InvalidPath(0)));
    assert_eq!(fs.read("bad/\0"), Err(Error::InvalidPath(4)));
    assert_eq!(fs.read("\0"), Err(Error::InvalidPath(0)));
}

#[test]
fn spec_r4_lexical_normalization() {
    let mut fs = Fs::new();
    fs.mkdir("/a").unwrap();
    fs.mkdir("/a/b").unwrap();
    fs.write("/a/file", b"data").unwrap();

    assert_eq!(fs.read("//a///b/../file/"), Ok(b"data".to_vec()));
    assert_eq!(fs.read("/../../a/./file"), Ok(b"data".to_vec()));
    assert_eq!(fs.mkdir("////"), Err(Error::Exists("/".into())));
}

#[test]
fn spec_r5_lookup_error_prefixes_and_precedence() {
    let mut fs = Fs::new();
    fs.write("/file", b"x").unwrap();

    assert_eq!(fs.read("/absent/child"), Err(Error::Missing("/absent".into())));
    assert_eq!(fs.read("/file/child"), Err(Error::NotDir("/file".into())));
    assert_eq!(fs.mkdir("/file/child"), Err(Error::NotDir("/file".into())));
}

#[test]
fn spec_r6_creation_requires_existing_directory_parent() {
    let mut fs = Fs::new();
    fs.write("/parent-file", b"x").unwrap();

    assert_eq!(fs.write("/missing/child", b"x"), Err(Error::Missing("/missing".into())));
    assert_eq!(fs.mkdir("/missing/child"), Err(Error::Missing("/missing".into())));
    assert_eq!(fs.write("/parent-file/child", b"x"), Err(Error::NotDir("/parent-file".into())));
}

#[test]
fn spec_r7_mkdir_creates_one_directory_and_rejects_existing_entries() {
    let mut fs = Fs::new();
    fs.mkdir("/dir").unwrap();
    fs.write("/file", b"x").unwrap();

    assert_eq!(fs.mkdir("/dir"), Err(Error::Exists("/dir".into())));
    assert_eq!(fs.mkdir("/file"), Err(Error::Exists("/file".into())));
    assert_eq!(fs.ls("/dir"), Ok(vec![]));
}

#[test]
fn spec_r8_write_replaces_files_and_rejects_directories() {
    let mut fs = Fs::new();
    fs.mkdir("/dir").unwrap();
    fs.write("/file", b"long").unwrap();

    assert_eq!(fs.write("/file", b"new"), Ok(()));
    assert_eq!(fs.read("/file"), Ok(b"new".to_vec()));
    assert_eq!(fs.write("/dir", b"x"), Err(Error::IsDir("/dir".into())));
}

#[test]
fn spec_r9_read_bytes_and_directory_error() {
    let mut fs = Fs::new();
    let bytes = [0, 0xff, 0x80, b'a'];
    fs.write("/bytes", &bytes).unwrap();
    fs.mkdir("/dir").unwrap();

    assert_eq!(fs.read("/bytes"), Ok(bytes.to_vec()));
    assert_eq!(fs.read("/dir"), Err(Error::IsDir("/dir".into())));
}

#[test]
fn spec_r10_ls_requires_directory_and_allows_empty() {
    let mut fs = Fs::new();
    fs.mkdir("/empty").unwrap();
    fs.write("/file", b"x").unwrap();

    assert_eq!(fs.ls("/empty"), Ok(vec![]));
    assert_eq!(fs.ls("/file"), Err(Error::NotDir("/file".into())));
}

#[test]
fn spec_r11_ls_sorts_by_string_order() {
    let mut fs = Fs::new();
    fs.write("/z", b"").unwrap();
    fs.mkdir("/A").unwrap();
    fs.write("/a", b"").unwrap();
    fs.mkdir("/b",).unwrap();

    assert_eq!(
        fs.ls("/"),
        Ok(vec![
            ("A".into(), Kind::Dir),
            ("a".into(), Kind::File),
            ("b".into(), Kind::Dir),
            ("z".into(), Kind::File),
        ])
    );
}

#[test]
fn spec_r12_failures_preserve_state_and_results_are_owned() {
    let mut fs = Fs::new();
    fs.mkdir("/dir").unwrap();
    fs.write("/dir/file", b"original").unwrap();
    fs.chdir("/dir").unwrap();

    assert_eq!(fs.write("/dir", b"replacement"), Err(Error::IsDir("/dir".into())));
    assert_eq!(fs.mkdir("/dir/file/child"), Err(Error::NotDir("/dir/file".into())));
    assert_eq!(fs.cwd(), "/dir");
    assert_eq!(fs.read("/dir/file"), Ok(b"original".to_vec()));

    let mut bytes = fs.read("/dir/file").unwrap();
    bytes[0] = b'X';
    assert_eq!(fs.read("/dir/file"), Ok(b"original".to_vec()));

    let mut listing = fs.ls("/dir").unwrap();
    listing[0].0.push_str("-changed");
    assert_eq!(fs.ls("/dir"), Ok(vec![("file".into(), Kind::File)]));

    let mut cwd = fs.cwd();
    cwd.push_str("/changed");
    assert_eq!(fs.cwd(), "/dir");

    assert_eq!(fs.write("/bad\0path", b"x"), Err(Error::InvalidPath(4)));
    assert_eq!(fs.cwd(), "/dir");
}

#[test]
fn spec_r13_empty_files_and_truncating_replacements() {
    let mut fs = Fs::new();
    fs.write("/file", b"long contents").unwrap();
    fs.write("/file", b"xy").unwrap();
    assert_eq!(fs.read("/file"), Ok(b"xy".to_vec()));

    fs.write("/file", b"").unwrap();
    assert_eq!(fs.read("/file"), Ok(vec![]));
}

#[test]
fn spec_r14_root_operation_rules() {
    let mut fs = Fs::new();
    fs.mkdir("/child").unwrap();

    assert_eq!(fs.mkdir("/"), Err(Error::Exists("/".into())));
    assert_eq!(fs.write("/", b"x"), Err(Error::IsDir("/".into())));
    assert_eq!(fs.read("/"), Err(Error::IsDir("/".into())));
    assert_eq!(fs.ls("/"), Ok(vec![("child".into(), Kind::Dir)]));
    assert_eq!(fs.write("/child/..", b"x"), Err(Error::IsDir("/".into())));
}

#[test]
fn spec_r15_ls_contains_only_immediate_children() {
    let mut fs = Fs::new();
    fs.mkdir("/parent").unwrap();
    fs.mkdir("/parent/child").unwrap();
    fs.write("/parent/child/grandchild-file", b"x").unwrap();
    fs.write("/parent/sibling", b"x").unwrap();

    assert_eq!(
        fs.ls("/parent"),
        Ok(vec![
            ("child".into(), Kind::Dir),
            ("sibling".into(), Kind::File),
        ])
    );
}

#[test]
fn spec_r16_component_names_are_verbatim_and_case_sensitive() {
    let mut fs = Fs::new();
    for name in ["with space", "雪", "back\\slash", "dot.name", "a..b", "Case", "case"] {
        fs.write(&format!("/{name}"), name.as_bytes()).unwrap();
        assert_eq!(fs.read(&format!("/{name}")), Ok(name.as_bytes().to_vec()));
    }

    assert_eq!(
        fs.ls("/"),
        Ok(vec![
            ("Case".into(), Kind::File),
            ("a..b".into(), Kind::File),
            ("back\\slash".into(), Kind::File),
            ("case".into(), Kind::File),
            ("dot.name".into(), Kind::File),
            ("with space".into(), Kind::File),
            ("雪".into(), Kind::File),
        ])
    );
    assert_eq!(fs.read("/Case"), Ok(b"Case".to_vec()));
    assert_eq!(fs.read("/case"), Ok(b"case".to_vec()));
}

#[test]
fn spec_r17_operations_do_not_change_entry_kinds() {
    let mut fs = Fs::new();
    fs.mkdir("/dir").unwrap();
    fs.write("/file", b"kept").unwrap();

    assert_eq!(fs.mkdir("/file"), Err(Error::Exists("/file".into())));
    assert_eq!(fs.write("/dir", b"x"), Err(Error::IsDir("/dir".into())));
    assert_eq!(fs.ls("/dir"), Ok(vec![]));
    assert_eq!(fs.read("/file"), Ok(b"kept".to_vec()));
}

#[test]
fn spec_r18_errors_use_canonical_paths_after_normalization() {
    let mut fs = Fs::new();
    fs.write("/file", b"x").unwrap();

    assert_eq!(fs.read("//missing///child/"), Err(Error::Missing("/missing".into())));
    assert_eq!(fs.read("/file/./child"), Err(Error::NotDir("/file".into())));
    assert_eq!(fs.mkdir("/file/../file/"), Err(Error::Exists("/file".into())));
}

#[test]
fn spec_r19_cwd_is_owned_and_relative_paths_use_operation_entry_cwd() {
    let mut fs = Fs::new();
    fs.mkdir("/work").unwrap();
    fs.mkdir("/work/sub").unwrap();
    fs.chdir("/work").unwrap();

    assert_eq!(fs.write("sub/../note", b"n"), Ok(()));
    assert_eq!(fs.read("./note"), Ok(b"n".to_vec()));
    assert_eq!(fs.cwd(), "/work");
}

#[test]
fn spec_r20_chdir_normalizes_and_requires_directory() {
    let mut fs = Fs::new();
    fs.mkdir("/a").unwrap();
    fs.write("/file", b"x").unwrap();

    assert_eq!(fs.chdir("/a/./"), Ok(()));
    assert_eq!(fs.cwd(), "/a");
    assert_eq!(fs.chdir("/file/"), Err(Error::NotDir("/file".into())));
    assert_eq!(fs.cwd(), "/a");
    assert_eq!(fs.chdir("/missing/child"), Err(Error::Missing("/missing".into())));
    assert_eq!(fs.cwd(), "/a");
}

#[test]
fn spec_r21_dot_components_are_removed_before_lookup_and_trailing_slash_is_neutral() {
    let mut fs = Fs::new();
    fs.mkdir("/ok").unwrap();
    fs.write("/file", b"x").unwrap();

    assert_eq!(fs.read("/absent/../ok/"), Err(Error::IsDir("/ok".into())));
    assert_eq!(fs.read("/file/"), Ok(b"x".to_vec()));
    assert_eq!(fs.read("/absent/../file"), Ok(b"x".to_vec()));
}

#[test]
fn spec_r22_root_normalization_and_relative_dot_paths() {
    let mut fs = Fs::new();
    fs.mkdir("/child").unwrap();
    fs.chdir("/child").unwrap();

    assert_eq!(fs.ls("."), Ok(vec![]));
    assert_eq!(fs.mkdir(".."), Err(Error::Exists("/".into())));
    assert_eq!(fs.write("../..", b"x"), Err(Error::IsDir("/".into())));
    assert_eq!(fs.chdir("../../.."), Ok(()));
    assert_eq!(fs.cwd(), "/");
}

#[test]
fn spec_r23_empty_whitespace_and_nul_offsets() {
    let mut fs = Fs::new();

    assert_eq!(fs.read(""), Err(Error::InvalidPath(0)));
    assert_eq!(fs.write("   ", b"x"), Ok(()));
    assert_eq!(fs.read("   "), Ok(b"x".to_vec()));
    assert_eq!(fs.read("雪\0x"), Err(Error::InvalidPath(3)));
    assert_eq!(fs.read("///\0"), Err(Error::InvalidPath(3)));
}

#[test]
fn spec_r24_non_chdir_operations_preserve_cwd_and_failed_chdir_preserves_cwd() {
    let mut fs = Fs::new();
    fs.mkdir("/work").unwrap();
    fs.mkdir("/work/dir").unwrap();
    fs.write("/work/file", b"x").unwrap();
    fs.chdir("/work").unwrap();

    assert_eq!(fs.mkdir("new"), Ok(()));
    assert_eq!(fs.write("new-file", b"y"), Ok(()));
    assert_eq!(fs.read("file"), Ok(b"x".to_vec()));
    assert_eq!(fs.ls("."), Ok(vec![
        ("dir".into(), Kind::Dir),
        ("file".into(), Kind::File),
        ("new".into(), Kind::Dir),
        ("new-file".into(), Kind::File),
    ]));
    assert_eq!(fs.cwd(), "/work");

    assert_eq!(fs.chdir("missing"), Err(Error::Missing("/work/missing".into())));
    assert_eq!(fs.cwd(), "/work");
    assert_eq!(fs.chdir("file/child"), Err(Error::NotDir("/work/file".into())));
    assert_eq!(fs.cwd(), "/work");
    assert_eq!(fs.chdir("bad\0path"), Err(Error::InvalidPath(3)));
    assert_eq!(fs.cwd(), "/work");
}
