#[test]
fn spec_r1_public_api_surface() {
    let mut f = Fs::new();
    assert_eq!(f.cwd(), "/");
    assert_eq!(f.ls("/"), Ok(vec![]));
}

#[test]
fn spec_r2_fresh_independent_instances() {
    let mut first = Fs::new();
    let second = Fs::new();
    first.mkdir("/only-here").unwrap();
    assert_eq!(second.ls("/"), Ok(vec![]));
}

#[test]
fn spec_r3_nul_precedence_and_byte_offset() {
    let mut f = Fs::new();
    assert_eq!(f.mkdir("/missing/\0"), Err(Error::InvalidPath(9)));
    assert_eq!(f.write("", b""), Err(Error::InvalidPath(0)));
    assert_eq!(f.read("/bad\0/"), Err(Error::InvalidPath(4)));
}

#[test]
fn spec_r4_lexical_normalization() {
    let mut f = Fs::new();
    f.mkdir("/a").unwrap();
    f.write("/a/x", b"ok").unwrap();
    assert_eq!(f.read("//a///./b/../x/"), Ok(b"ok".to_vec()));
    assert_eq!(f.read("../../a/x"), Ok(b"ok".to_vec()));
}

#[test]
fn spec_r5_first_lookup_error_prefix() {
    let mut f = Fs::new();
    assert_eq!(f.read("/missing/child"), Err(Error::Missing("/missing".into())));
    f.write("/file", b"").unwrap();
    assert_eq!(f.read("/file/child"), Err(Error::NotDir("/file".into())));
}

#[test]
fn spec_r6_creation_requires_existing_directory_parent() {
    let mut f = Fs::new();
    assert_eq!(f.mkdir("/absent/child"), Err(Error::Missing("/absent".into())));
    f.write("/file", b"").unwrap();
    assert_eq!(f.write("/file/child", b"x"), Err(Error::NotDir("/file".into())));
}

#[test]
fn spec_r7_mkdir_existing_entry_errors() {
    let mut f = Fs::new();
    f.mkdir("/dir").unwrap();
    f.write("/file", b"").unwrap();
    assert_eq!(f.mkdir("/dir"), Err(Error::Exists("/dir".into())));
    assert_eq!(f.mkdir("/file"), Err(Error::Exists("/file".into())));
}

#[test]
fn spec_r8_write_file_or_directory_error() {
    let mut f = Fs::new();
    f.mkdir("/dir").unwrap();
    assert_eq!(f.write("/dir", b"x"), Err(Error::IsDir("/dir".into())));
    f.write("/new", b"first").unwrap();
    f.write("/new", b"second").unwrap();
    assert_eq!(f.read("/new"), Ok(b"second".to_vec()));
}

#[test]
fn spec_r9_read_bytes_and_directory_error() {
    let mut f = Fs::new();
    f.write("/binary", &[0, 255, 128]).unwrap();
    assert_eq!(f.read("/binary"), Ok(vec![0, 255, 128]));
    f.mkdir("/dir").unwrap();
    assert_eq!(f.read("/dir"), Err(Error::IsDir("/dir".into())));
}

#[test]
fn spec_r10_ls_requires_directory() {
    let mut f = Fs::new();
    f.mkdir("/empty").unwrap();
    f.write("/file", b"").unwrap();
    assert_eq!(f.ls("/empty"), Ok(vec![]));
    assert_eq!(f.ls("/file"), Err(Error::NotDir("/file".into())));
}

#[test]
fn spec_r11_ls_sorted_string_order() {
    let mut f = Fs::new();
    for name in ["b", "A", "a", "é", "Z"] {
        f.write(&format!("/{name}"), b"").unwrap();
    }
    assert_eq!(
        f.ls("/"),
        Ok(vec![
            ("A".into(), Kind::File),
            ("Z".into(), Kind::File),
            ("a".into(), Kind::File),
            ("b".into(), Kind::File),
            ("é".into(), Kind::File),
        ])
    );
}

#[test]
fn spec_r12_failed_mutations_preserve_state_and_results_are_owned() {
    let mut f = Fs::new();
    f.mkdir("/dir").unwrap();
    f.write("/dir/file", b"original").unwrap();
    f.chdir("/dir").unwrap();
    assert_eq!(f.rename("/dir", "/new", true), Err(Error::Busy("/dir".into())));
    assert_eq!(f.cwd(), "/dir");
    assert_eq!(f.read("file"), Ok(b"original".to_vec()));
    let mut contents = f.read("file").unwrap();
    contents[0] = b'X';
    let mut listing = f.ls(".").unwrap();
    listing[0].0.clear();
    let mut cwd = f.cwd();
    cwd.clear();
    assert_eq!(f.read("file"), Ok(b"original".to_vec()));
    assert_eq!(f.ls("."), Ok(vec![("file".into(), Kind::File)]));
    assert_eq!(f.cwd(), "/dir");
}

#[test]
fn spec_r13_empty_files_and_truncating_replacement() {
    let mut f = Fs::new();
    f.write("/f", b"longer").unwrap();
    f.write("/f", b"").unwrap();
    assert_eq!(f.read("/f"), Ok(vec![]));
    f.write("/f", b"ab").unwrap();
    f.write("/f", b"x").unwrap();
    assert_eq!(f.read("/f"), Ok(b"x".to_vec()));
}

#[test]
fn spec_r14_root_operation_rules() {
    let mut f = Fs::new();
    assert_eq!(f.mkdir("/"), Err(Error::Exists("/".into())));
    assert_eq!(f.write("/", b"data"), Err(Error::IsDir("/".into())));
    assert_eq!(f.read("/"), Err(Error::IsDir("/".into())));
    f.mkdir("/child").unwrap();
    assert_eq!(f.ls("/"), Ok(vec![("child".into(), Kind::Dir)]));
}

#[test]
fn spec_r15_ls_only_immediate_children_and_stored_kinds() {
    let mut f = Fs::new();
    f.mkdir("/d").unwrap();
    f.mkdir("/d/sub").unwrap();
    f.write("/d/file", b"").unwrap();
    f.write("/d/sub/nested", b"").unwrap();
    assert_eq!(
        f.ls("/d"),
        Ok(vec![
            ("file".into(), Kind::File),
            ("sub".into(), Kind::Dir),
        ])
    );
}

#[test]
fn spec_r16_verbatim_names_case_spaces_unicode_backslash_and_dots() {
    let mut f = Fs::new();
    for name in ["A", "a", "two words", "雪", r"a\b", "a.b", "..."] {
        f.write(&format!("/{name}"), name.as_bytes()).unwrap();
    }
    for name in ["A", "a", "two words", "雪", r"a\b", "a.b", "..."] {
        assert_eq!(f.read(&format!("/{name}")), Ok(name.as_bytes().to_vec()));
    }
}

#[test]
fn spec_r17_mkdir_and_write_do_not_change_entry_kind() {
    let mut f = Fs::new();
    f.write("/file", b"bytes").unwrap();
    assert_eq!(f.mkdir("/file"), Err(Error::Exists("/file".into())));
    f.mkdir("/dir").unwrap();
    assert_eq!(f.write("/dir", b"bytes"), Err(Error::IsDir("/dir".into())));
    f.write("/unrelated", b"stay").unwrap();
    f.write("/new", b"created").unwrap();
    assert_eq!(f.read("/unrelated"), Ok(b"stay".to_vec()));
}

#[test]
fn spec_r18_normalized_errors_and_parent_traversal_precedence() {
    let mut f = Fs::new();
    f.write("/file", b"").unwrap();
    assert_eq!(f.mkdir("/file/../file/child"), Err(Error::NotDir("/file".into())));
    assert_eq!(f.write("/absent/../new", b"x"), Ok(()));
    assert_eq!(f.read("/absent"), Err(Error::Missing("/absent".into())));
}

#[test]
fn spec_r19_cwd_owned_and_relative_paths_use_current_directory() {
    let mut f = Fs::new();
    f.mkdir("/work").unwrap();
    f.chdir("/work").unwrap();
    f.write("item", b"v").unwrap();
    assert_eq!(f.read("/work/item"), Ok(b"v".to_vec()));
    assert_eq!(f.cwd(), "/work");
}

#[test]
fn spec_r20_chdir_requires_directory_and_preserves_cwd_on_failure() {
    let mut f = Fs::new();
    f.mkdir("/dir").unwrap();
    f.write("/file", b"").unwrap();
    f.chdir("/dir").unwrap();
    assert_eq!(f.chdir("/file"), Err(Error::NotDir("/file".into())));
    assert_eq!(f.chdir("/missing"), Err(Error::Missing("/missing".into())));
    assert_eq!(f.cwd(), "/dir");
    f.chdir("/").unwrap();
    assert_eq!(f.cwd(), "/");
}

#[test]
fn spec_r21_dot_components_removed_before_lookup_and_trailing_slash_allowed() {
    let mut f = Fs::new();
    f.mkdir("/ok").unwrap();
    f.write("/ok/file", b"v").unwrap();
    assert_eq!(f.read("/absent/../ok/./file/"), Ok(b"v".to_vec()));
}

#[test]
fn spec_r22_root_clamping_and_relative_dot_paths() {
    let mut f = Fs::new();
    assert_eq!(f.ls("../../.."), Ok(vec![]));
    assert_eq!(f.mkdir(".."), Err(Error::Exists("/".into())));
    assert_eq!(f.write(".", b""), Err(Error::IsDir("/".into())));
    assert_eq!(f.read("../../../"), Err(Error::IsDir("/".into())));
}

#[test]
fn spec_r23_whitespace_paths_and_normalized_error_names() {
    let mut f = Fs::new();
    f.write("/   ", b"space").unwrap();
    assert_eq!(f.read("/   "), Ok(b"space".to_vec()));
    assert_eq!(f.read("/missing/../absent/child"), Err(Error::Missing("/absent".into())));
    assert_eq!(f.mkdir(""), Err(Error::InvalidPath(0)));
}

#[test]
fn spec_r24_one_path_operations_preserve_cwd() {
    let mut f = Fs::new();
    f.mkdir("/d").unwrap();
    f.chdir("/d").unwrap();
    f.write("../file", b"").unwrap();
    let _ = f.read(".");
    let _ = f.ls(".");
    let _ = f.mkdir("sub");
    let _ = f.write("sub/file", b"x");
    assert_eq!(f.cwd(), "/d");
}

#[test]
fn spec_r25_rename_validates_both_endpoints_then_root_precedence() {
    let mut f = Fs::new();
    assert_eq!(f.rename("/missing", "/bad\0", false), Err(Error::InvalidPath(4)));
    assert_eq!(f.rename("/missing", "/", false), Err(Error::Root));
    assert_eq!(f.rename("/", "/missing", false), Err(Error::Root));
}

#[test]
fn spec_r26_rename_busy_and_destination_parent_precedence() {
    let mut f = Fs::new();
    f.mkdir("/dir").unwrap();
    f.mkdir("/dir/sub").unwrap();
    f.chdir("/dir/sub").unwrap();
    assert_eq!(f.rename("/dir", "/absent/target", true), Err(Error::Busy("/dir".into())));
    f.chdir("/").unwrap();
    f.mkdir("/cwd-parent").unwrap();
    f.mkdir("/cwd-parent/cwd").unwrap();
    f.chdir("/cwd-parent/cwd").unwrap();
    assert_eq!(f.rename("/missing", "/cwd-parent", true), Err(Error::Missing("/missing".into())));
    f.chdir("/").unwrap();
    f.write("/file", b"").unwrap();
    assert_eq!(f.rename("/file", "/file/child", true), Err(Error::NotDir("/file".into())));
}

#[test]
fn spec_r27_rename_same_path_and_busy_cwd_source() {
    let mut f = Fs::new();
    f.write("/file", b"data").unwrap();
    assert_eq!(f.rename("/file", "/file", false), Ok(()));
    assert_eq!(f.read("/file"), Ok(b"data".to_vec()));
    f.mkdir("/dir").unwrap();
    f.chdir("/dir").unwrap();
    assert_eq!(f.rename("/dir", "/dir", true), Err(Error::Busy("/dir".into())));
}

#[test]
fn spec_r28_rename_cycle_uses_component_boundaries_and_parent_precedence() {
    let mut f = Fs::new();
    f.mkdir("/a").unwrap();
    f.mkdir("/ab").unwrap();
    f.mkdir("/a/child").unwrap();
    assert_eq!(f.rename("/a", "/a/child/new", true), Err(Error::Missing("/a/child/new".into())));
    assert_eq!(f.rename("/a", "/a/new", true), Err(Error::Cycle("/a/new".into())));
    assert_eq!(f.rename("/a", "/ab", false), Err(Error::Exists("/ab".into())));
}

#[test]
fn spec_r29_rename_existing_destination_without_overwrite() {
    let mut f = Fs::new();
    f.write("/source", b"s").unwrap();
    f.mkdir("/destination").unwrap();
    assert_eq!(
        f.rename("/source", "/destination", false),
        Err(Error::Exists("/destination".into()))
    );
}

#[test]
fn spec_r30_rename_overwrite_type_mismatches() {
    let mut f = Fs::new();
    f.mkdir("/source-dir").unwrap();
    f.write("/source-file", b"").unwrap();
    f.write("/dest-file", b"").unwrap();
    f.mkdir("/dest-dir").unwrap();
    assert_eq!(
        f.rename("/source-dir", "/dest-file", true),
        Err(Error::NotDir("/dest-file".into()))
    );
    assert_eq!(
        f.rename("/source-file", "/dest-dir", true),
        Err(Error::IsDir("/dest-dir".into()))
    );
}

#[test]
fn spec_r31_rename_overwrite_directory_requires_empty_destination() {
    let mut f = Fs::new();
    f.mkdir("/source").unwrap();
    f.write("/source/data", b"tree").unwrap();
    f.mkdir("/destination").unwrap();
    f.write("/destination/child", b"").unwrap();
    assert_eq!(
        f.rename("/source", "/destination", true),
        Err(Error::NotEmpty("/destination".into()))
    );
    f.remove("/destination/child", false).unwrap();
    f.rename("/source", "/destination", true).unwrap();
    assert_eq!(f.read("/destination/data"), Ok(b"tree".to_vec()));
}

#[test]
fn spec_r32_successful_rename_moves_tree_and_preserves_unrelated_entries() {
    let mut f = Fs::new();
    f.mkdir("/src").unwrap();
    f.mkdir("/src/sub").unwrap();
    f.write("/src/sub/data", &[0, 255]).unwrap();
    f.write("/src-sibling", b"keep").unwrap();
    f.rename("/src", "/dst", false).unwrap();
    assert_eq!(f.read("/dst/sub/data"), Ok(vec![0, 255]));
    assert_eq!(f.ls("/"), Ok(vec![
        ("dst".into(), Kind::Dir),
        ("src-sibling".into(), Kind::File),
    ]));
    assert_eq!(f.ls("/src"), Err(Error::Missing("/src".into())));
}

#[test]
fn spec_r33_remove_root_and_missing_entry() {
    let mut f = Fs::new();
    f.write("/child", b"").unwrap();
    assert_eq!(f.remove("/", false), Err(Error::Root));
    assert_eq!(f.remove("/../../", true), Err(Error::Root));
    assert_eq!(f.remove("/missing", false), Err(Error::Missing("/missing".into())));
}

#[test]
fn spec_r34_remove_busy_directory_precedes_nonempty_check() {
    let mut f = Fs::new();
    f.mkdir("/dir").unwrap();
    f.write("/dir/file", b"").unwrap();
    f.chdir("/dir").unwrap();
    assert_eq!(f.remove("/dir", false), Err(Error::Busy("/dir".into())));
    assert_eq!(f.remove("/dir", true), Err(Error::Busy("/dir".into())));
}

#[test]
fn spec_r35_remove_recursive_and_nonrecursive_directory_rules() {
    let mut f = Fs::new();
    f.mkdir("/dir").unwrap();
    f.write("/dir/file", b"").unwrap();
    assert_eq!(f.remove("/dir", false), Err(Error::NotEmpty("/dir".into())));
    assert_eq!(f.remove("/dir/file", true), Ok(()));
    assert_eq!(f.remove("/dir", false), Ok(()));
    f.mkdir("/tree").unwrap();
    f.mkdir("/tree/sub").unwrap();
    f.write("/tree/sub/file", b"").unwrap();
    f.remove("/tree", true).unwrap();
    assert_eq!(f.ls("/tree"), Err(Error::Missing("/tree".into())));
}

#[test]
fn spec_r36_remove_and_rename_preserve_cwd_and_prefix_siblings() {
    let mut f = Fs::new();
    f.mkdir("/a").unwrap();
    f.mkdir("/ab").unwrap();
    f.write("/ab/file", b"keep").unwrap();
    f.write("/a/file", b"remove").unwrap();
    f.remove("/a", true).unwrap();
    assert_eq!(f.read("/ab/file"), Ok(b"keep".to_vec()));
    f.mkdir("/work").unwrap();
    f.chdir("/work").unwrap();
    f.write("/other", b"v").unwrap();
    f.rename("/other", "/moved", false).unwrap();
    assert_eq!(f.cwd(), "/work");
    assert_eq!(f.read("/moved"), Ok(b"v".to_vec()));
}
