#[test]
fn spec_r1_public_api_surface() {
    let mut f = Fs::new();
    assert_eq!(f.cwd(), "/");
}

#[test]
fn spec_r2_empty_independent_instances() {
    let mut a = Fs::new();
    let b = Fs::new();
    a.mkdir("/only-a").unwrap();
    assert_eq!(b.ls("/"), Ok(vec![]));
}

#[test]
fn spec_r3_nul_precedes_empty_and_format_errors() {
    let mut f = Fs::new();
    assert_eq!(f.read(""), Err(Error::InvalidPath(0)));
    assert_eq!(f.read("/missing\0/"), Err(Error::InvalidPath(8)));
    assert_eq!(f.read("a\0b\0"), Err(Error::InvalidPath(1)));
}

#[test]
fn spec_r4_lexical_normalization() {
    let mut f = Fs::new();
    f.mkdir("/a").unwrap();
    f.mkdir("/a/b").unwrap();
    f.write("/a/file", b"x").unwrap();
    f.chdir("/a/b").unwrap();
    assert_eq!(f.read("../file"), Ok(b"x".to_vec()));
    assert_eq!(f.ls("../../../../"), Ok(vec![("a".into(), Kind::Dir)]));
    assert_eq!(f.read("///a//file///"), Ok(b"x".to_vec()));
}

#[test]
fn spec_r5_first_missing_and_nondirectory_prefix() {
    let mut f = Fs::new();
    f.write("/file", b"x").unwrap();
    assert_eq!(f.read("/absent/child"), Err(Error::Missing("/absent".into())));
    assert_eq!(f.read("/file/child"), Err(Error::NotDir("/file".into())));
}

#[test]
fn spec_r6_creation_requires_existing_directory_parent() {
    let mut f = Fs::new();
    assert_eq!(f.mkdir("/missing/child"), Err(Error::Missing("/missing".into())));
    f.write("/file", b"x").unwrap();
    assert_eq!(f.write("/file/child", b"x"), Err(Error::NotDir("/file".into())));
    assert_eq!(f.ls("/"), Ok(vec![("file".into(), Kind::File)]));
}

#[test]
fn spec_r7_mkdir_checks_final_entry_without_following_it() {
    let mut f = Fs::new();
    f.mkdir("/dir").unwrap();
    f.symlink("/missing", "/dangling").unwrap();
    assert_eq!(f.mkdir("/dangling"), Err(Error::Exists("/dangling".into())));
    assert_eq!(f.mkdir("/dir"), Err(Error::Exists("/dir".into())));
}

#[test]
fn spec_r8_write_follows_links_and_creates_dangling_target() {
    let mut f = Fs::new();
    f.mkdir("/target").unwrap();
    f.symlink("/target/new", "/shortcut").unwrap();
    f.write("/shortcut", b"data").unwrap();
    assert_eq!(f.read("/target/new"), Ok(b"data".to_vec()));
    assert_eq!(
        f.ls("/"),
        Ok(vec![
            ("shortcut".into(), Kind::Link),
            ("target".into(), Kind::Dir)
        ])
    );
}

#[test]
fn spec_r9_read_copies_arbitrary_file_bytes() {
    let mut f = Fs::new();
    let bytes = vec![0, 255, 128, b'a'];
    f.write("/bytes", &bytes).unwrap();
    let mut got = f.read("/bytes").unwrap();
    got[0] = 9;
    assert_eq!(f.read("/bytes"), Ok(bytes));
}

#[test]
fn spec_r10_ls_requires_resolved_directory() {
    let mut f = Fs::new();
    f.write("/file", b"x").unwrap();
    assert_eq!(f.ls("/file"), Err(Error::NotDir("/file".into())));
    assert_eq!(f.ls("/absent"), Err(Error::Missing("/absent".into())));
}

#[test]
fn spec_r11_ls_is_sorted_by_string_order() {
    let mut f = Fs::new();
    for name in ["z", "A", "a", "é"] {
        f.write(&format!("/{name}"), b"").unwrap();
    }
    assert_eq!(
        f.ls("/"),
        Ok(vec![
            ("A".into(), Kind::File),
            ("a".into(), Kind::File),
            ("z".into(), Kind::File),
            ("é".into(), Kind::File)
        ])
    );
}

#[test]
fn spec_r12_failed_mutations_preserve_entries_and_cwd() {
    let mut f = Fs::new();
    f.mkdir("/d").unwrap();
    f.write("/d/file", b"original").unwrap();
    f.chdir("/d").unwrap();
    assert_eq!(f.write("/d", b"changed"), Err(Error::IsDir("/d".into())));
    assert_eq!(f.read("/d/file"), Ok(b"original".to_vec()));
    assert_eq!(f.cwd(), "/d");
}

#[test]
fn spec_r13_empty_and_truncating_writes() {
    let mut f = Fs::new();
    f.write("/f", b"longer").unwrap();
    f.write("/f", b"").unwrap();
    assert_eq!(f.read("/f"), Ok(vec![]));
    let source = vec![1, 2, 3];
    f.write("/f", &source).unwrap();
    assert_eq!(f.read("/f"), Ok(vec![1, 2, 3]));
}

#[test]
fn spec_r14_root_operation_rules() {
    let mut f = Fs::new();
    assert_eq!(f.mkdir("/"), Err(Error::Exists("/".into())));
    assert_eq!(f.write("/", b"x"), Err(Error::IsDir("/".into())));
    assert_eq!(f.read("/"), Err(Error::IsDir("/".into())));
    f.mkdir("/child").unwrap();
    assert_eq!(f.ls("/"), Ok(vec![("child".into(), Kind::Dir)]));
}

#[test]
fn spec_r15_ls_lists_only_stored_immediate_kinds() {
    let mut f = Fs::new();
    f.mkdir("/dir").unwrap();
    f.write("/dir/child", b"x").unwrap();
    f.symlink("/dir", "/link").unwrap();
    assert_eq!(
        f.ls("/"),
        Ok(vec![
            ("dir".into(), Kind::Dir),
            ("link".into(), Kind::Link)
        ])
    );
    assert_eq!(f.ls("/dir"), Ok(vec![("child".into(), Kind::File)]));
}

#[test]
fn spec_r16_names_are_verbatim_and_case_sensitive() {
    let mut f = Fs::new();
    for name in [" ", "a\\b", "a.b", "...", "猫", "Name", "name"] {
        f.write(&format!("/{name}"), name.as_bytes()).unwrap();
    }
    assert_eq!(f.read("/a\\b"), Ok(b"a\\b".to_vec()));
    assert_eq!(f.read("/Name"), Ok(b"Name".to_vec()));
    assert_eq!(f.read("/name"), Ok(b"name".to_vec()));
    assert_eq!(f.read("/ "), Ok(b" ".to_vec()));
}

#[test]
fn spec_r17_mkdir_and_write_do_not_change_entry_kinds() {
    let mut f = Fs::new();
    f.write("/file", b"x").unwrap();
    assert_eq!(f.mkdir("/file"), Err(Error::Exists("/file".into())));
    f.mkdir("/dir").unwrap();
    assert_eq!(f.write("/dir", b"x"), Err(Error::IsDir("/dir".into())));
    assert_eq!(
        f.ls("/"),
        Ok(vec![
            ("dir".into(), Kind::Dir),
            ("file".into(), Kind::File)
        ])
    );
}

#[test]
fn spec_r18_normalized_error_paths_and_parent_precedence() {
    let mut f = Fs::new();
    f.write("/file", b"").unwrap();
    assert_eq!(f.read("/file/child/../leaf"), Err(Error::NotDir("/file".into())));
    assert_eq!(f.read("/missing/../other"), Err(Error::Missing("/other".into())));
}

#[test]
fn spec_r19_relative_paths_use_owned_cwd() {
    let mut f = Fs::new();
    f.mkdir("/a").unwrap();
    f.mkdir("/b").unwrap();
    f.chdir("/a").unwrap();
    f.write("item", b"a").unwrap();
    f.chdir("/b").unwrap();
    f.write("item", b"b").unwrap();
    let mut cwd = f.cwd();
    cwd.clear();
    assert_eq!(f.cwd(), "/b");
    assert_eq!(f.read("item"), Ok(b"b".to_vec()));
}

#[test]
fn spec_r20_chdir_stores_physical_resolved_path() {
    let mut f = Fs::new();
    f.mkdir("/real").unwrap();
    f.symlink("/real", "/alias").unwrap();
    f.chdir("/alias").unwrap();
    assert_eq!(f.cwd(), "/real");
    assert_eq!(f.chdir("/missing"), Err(Error::Missing("/missing".into())));
    assert_eq!(f.cwd(), "/real");
}

#[test]
fn spec_r21_dots_eliminated_before_lookup_and_trailing_slash_allowed() {
    let mut f = Fs::new();
    f.write("/ok", b"ok").unwrap();
    f.write("/file", b"file").unwrap();
    assert_eq!(f.read("/absent/../ok"), Ok(b"ok".to_vec()));
    assert_eq!(f.read("/file/"), Ok(b"file".to_vec()));
}

#[test]
fn spec_r22_root_after_clamping_and_relative_dot_paths() {
    let mut f = Fs::new();
    assert_eq!(f.ls("../../.."), Ok(vec![]));
    assert_eq!(f.ls("."), Ok(vec![]));
    assert_eq!(f.mkdir(".."), Err(Error::Exists("/".into())));
}

#[test]
fn spec_r23_empty_differs_from_whitespace_and_errors_are_canonical() {
    let mut f = Fs::new();
    assert_eq!(f.read(""), Err(Error::InvalidPath(0)));
    f.write("/ ", b"space").unwrap();
    assert_eq!(f.read("/ "), Ok(b"space".to_vec()));
    assert_eq!(f.read("///absent///"), Err(Error::Missing("/absent".into())));
    assert_eq!(f.read("/ab\0cd"), Err(Error::InvalidPath(3)));
}

#[test]
fn spec_r24_read_and_creation_operations_preserve_cwd() {
    let mut f = Fs::new();
    f.mkdir("/d").unwrap();
    f.chdir("/d").unwrap();
    let before = f.cwd();
    let _ = f.read("missing");
    let _ = f.ls(".");
    let _ = f.mkdir("new");
    let _ = f.write("new", b"x");
    assert_eq!(f.cwd(), before);
}

#[test]
fn spec_r25_rename_validates_both_paths_before_lookup() {
    let mut f = Fs::new();
    assert_eq!(
        f.rename("/missing", "/bad\0path", false),
        Err(Error::InvalidPath(4))
    );
    assert_eq!(f.rename("/missing", "/", false), Err(Error::Root));
}

#[test]
fn spec_r26_rename_busy_checks_precede_destination_checks() {
    let mut f = Fs::new();
    f.mkdir("/d").unwrap();
    f.mkdir("/d/sub").unwrap();
    f.write("/dest", b"x").unwrap();
    f.chdir("/d/sub").unwrap();
    assert_eq!(f.rename("/d", "/dest", false), Err(Error::Busy("/d".into())));
    assert_eq!(f.rename("/d", "/d2", false), Err(Error::Busy("/d".into())));
}

#[test]
fn spec_r27_rename_same_path_and_cwd_busy() {
    let mut f = Fs::new();
    f.write("/file", b"x").unwrap();
    assert_eq!(f.rename("/file", "/file", false), Ok(()));
    f.mkdir("/d").unwrap();
    f.chdir("/d").unwrap();
    assert_eq!(f.rename("/d", "/d", true), Err(Error::Busy("/d".into())));
    assert_eq!(f.rename("/missing", "/missing", true), Err(Error::Missing("/missing".into())));
}

#[test]
fn spec_r28_rename_directory_into_descendant_detects_component_cycle() {
    let mut f = Fs::new();
    f.mkdir("/a").unwrap();
    f.mkdir("/a/sub").unwrap();
    assert_eq!(f.rename("/a", "/a/sub/new", true), Err(Error::Cycle("/a/sub/new".into())));
    f.mkdir("/ab").unwrap();
    assert_eq!(f.rename("/a", "/ab/moved", false), Ok(()));
}

#[test]
fn spec_r29_rename_no_overwrite_fails_after_prechecks() {
    let mut f = Fs::new();
    f.write("/src", b"s").unwrap();
    f.mkdir("/dir").unwrap();
    assert_eq!(f.rename("/src", "/dir", false), Err(Error::Exists("/dir".into())));
}

#[test]
fn spec_r30_rename_overwrite_type_mismatches() {
    let mut f = Fs::new();
    f.mkdir("/dir").unwrap();
    f.write("/file", b"x").unwrap();
    assert_eq!(f.rename("/dir", "/file", true), Err(Error::NotDir("/file".into())));
    assert_eq!(f.rename("/file", "/dir", true), Err(Error::IsDir("/dir".into())));
}

#[test]
fn spec_r31_rename_directory_over_empty_directory_only() {
    let mut f = Fs::new();
    f.mkdir("/src").unwrap();
    f.write("/src/data", b"x").unwrap();
    f.mkdir("/dst").unwrap();
    assert_eq!(f.rename("/src", "/dst", true), Err(Error::NotEmpty("/dst".into())));
    f.remove("/src/data", false).unwrap();
    assert_eq!(f.rename("/src", "/dst", true), Ok(()));
    assert_eq!(f.ls("/dst"), Ok(vec![]));
}

#[test]
fn spec_r32_rename_preserves_tree_bytes_and_link_text() {
    let mut f = Fs::new();
    f.mkdir("/old").unwrap();
    f.write("/old/file", b"bytes").unwrap();
    f.symlink("../somewhere//.", "/old/link").unwrap();
    f.rename("/old", "/new", false).unwrap();
    assert_eq!(f.read("/new/file"), Ok(b"bytes".to_vec()));
    assert_eq!(f.read_link("/new/link"), Ok("../somewhere//.".into()));
    assert_eq!(f.ls("/"), Ok(vec![("new".into(), Kind::Dir)]));
}

#[test]
fn spec_r33_remove_root_and_final_link_behavior() {
    let mut f = Fs::new();
    f.symlink("/missing", "/link").unwrap();
    assert_eq!(f.remove("/", true), Err(Error::Root));
    assert_eq!(f.remove("/link", false), Ok(()));
    assert_eq!(f.ls("/"), Ok(vec![]));
}

#[test]
fn spec_r34_remove_busy_precedes_nonempty() {
    let mut f = Fs::new();
    f.mkdir("/d").unwrap();
    f.write("/d/file", b"x").unwrap();
    f.chdir("/d").unwrap();
    assert_eq!(f.remove("/d", false), Err(Error::Busy("/d".into())));
    assert_eq!(f.remove("/d", true), Err(Error::Busy("/d".into())));
}

#[test]
fn spec_r35_remove_recursive_subtree_and_links_without_following() {
    let mut f = Fs::new();
    f.mkdir("/d").unwrap();
    f.write("/d/file", b"x").unwrap();
    f.symlink("/missing", "/dangling").unwrap();
    assert_eq!(f.remove("/d", false), Err(Error::NotEmpty("/d".into())));
    assert_eq!(f.remove("/dangling", false), Ok(()));
    f.remove("/d", true).unwrap();
    assert_eq!(f.ls("/"), Ok(vec![]));
}

#[test]
fn spec_r36_rename_remove_preserve_cwd_and_prefix_siblings() {
    let mut f = Fs::new();
    f.mkdir("/a").unwrap();
    f.mkdir("/ab").unwrap();
    f.write("/a/file", b"x").unwrap();
    f.chdir("/ab").unwrap();
    f.rename("/a", "/c", false).unwrap();
    f.remove("/c", true).unwrap();
    assert_eq!(f.cwd(), "/ab");
    assert_eq!(f.ls("/ab"), Ok(vec![]));
}

#[test]
fn spec_r37_symlink_validation_order_and_target_errors() {
    let mut f = Fs::new();
    assert_eq!(
        f.symlink("\0target", "/bad\0link"),
        Err(Error::InvalidPath(4))
    );
    assert_eq!(f.symlink("", "/link"), Err(Error::InvalidTarget(0)));
    assert_eq!(f.symlink("ab\0cd", "/link"), Err(Error::InvalidTarget(2)));
    assert_eq!(f.symlink("", "/missing/link"), Err(Error::InvalidTarget(0)));
}

#[test]
fn spec_r38_symlink_stores_verbatim_target_and_collision() {
    let mut f = Fs::new();
    f.symlink(" ./some//../target ", "/link").unwrap();
    assert_eq!(f.read_link("/link"), Ok(" ./some//../target ".into()));
    assert_eq!(f.symlink("elsewhere", "/link"), Err(Error::Exists("/link".into())));
    assert_eq!(f.symlink("target", "/"), Err(Error::Exists("/".into())));
}

#[test]
fn spec_r39_link_expansion_uses_physical_parent_and_suffix() {
    let mut f = Fs::new();
    f.mkdir("/base").unwrap();
    f.mkdir("/base/dir").unwrap();
    f.write("/base/dir/file", b"x").unwrap();
    f.symlink("dir", "/base/relative").unwrap();
    assert_eq!(f.read("/base/relative/file"), Ok(b"x".to_vec()));
}

#[test]
fn spec_r40_revisiting_link_returns_loop_per_lookup() {
    let mut f = Fs::new();
    f.symlink("/b", "/a").unwrap();
    f.symlink("/a", "/b").unwrap();
    assert_eq!(f.read("/a"), Err(Error::Loop("/a".into())));
    assert_eq!(f.read("/a"), Err(Error::Loop("/a".into())));
}

#[test]
fn spec_r41_eight_expansions_allowed_and_ninth_depth_limited() {
    let mut f = Fs::new();
    f.write("/end", b"ok").unwrap();
    for i in (1..=8).rev() {
        let target = if i == 8 {
            "/end".to_string()
        } else {
            format!("/l{}", i + 1)
        };
        f.symlink(&target, &format!("/l{i}")).unwrap();
    }
    assert_eq!(f.read("/l1"), Ok(b"ok".to_vec()));

    let mut g = Fs::new();
    g.write("/end", b"ok").unwrap();
    for i in (1..=9).rev() {
        let target = if i == 9 {
            "/end".to_string()
        } else {
            format!("/l{}", i + 1)
        };
        g.symlink(&target, &format!("/l{i}")).unwrap();
    }
    assert_eq!(g.read("/l1"), Err(Error::Depth("/l9".into())));
}

#[test]
fn spec_r42_following_policy_differs_by_operation() {
    let mut f = Fs::new();
    f.mkdir("/target").unwrap();
    f.symlink("/target", "/alias").unwrap();
    assert_eq!(f.mkdir("/alias"), Err(Error::Exists("/alias".into())));
    assert_eq!(f.read_link("/alias"), Ok("/target".into()));
    f.mkdir("/target/child").unwrap();
    f.symlink("/target/child", "/other").unwrap();
    assert_eq!(f.rename("/other", "/moved", false), Ok(()));
    assert_eq!(f.read_link("/moved"), Ok("/target/child".into()));
}

#[test]
fn spec_r43_read_link_reads_final_link_without_following() {
    let mut f = Fs::new();
    f.symlink("/missing//.", "/link").unwrap();
    assert_eq!(f.read_link("/link"), Ok("/missing//.".into()));
    assert_eq!(f.read_link("/missing"), Err(Error::Missing("/missing".into())));
    f.write("/file", b"x").unwrap();
    assert_eq!(f.read_link("/file"), Err(Error::InvalidTarget(0)));
    assert_eq!(f.read_link("/"), Err(Error::InvalidTarget(0)));
}

#[test]
fn spec_r44_dangling_links_and_file_suffix_errors() {
    let mut f = Fs::new();
    f.symlink("/missing/target", "/dangling").unwrap();
    assert_eq!(f.read("/dangling"), Err(Error::Missing("/missing".into())));
    assert_eq!(f.read("/dangling/child"), Err(Error::Missing("/missing".into())));
    f.write("/file", b"x").unwrap();
    f.symlink("/file", "/file-link").unwrap();
    assert_eq!(f.read("/file-link/child"), Err(Error::NotDir("/file".into())));
}

#[test]
fn spec_r45_final_links_not_followed_by_endpoint_operations() {
    let mut f = Fs::new();
    f.symlink("/self", "/self").unwrap();
    assert_eq!(f.read_link("/self"), Ok("/self".into()));
    assert_eq!(f.remove("/self", false), Ok(()));
    f.symlink("/missing", "/dangling").unwrap();
    assert_eq!(f.rename("/dangling", "/moved", false), Ok(()));
    assert_eq!(f.read_link("/moved"), Ok("/missing".into()));
}

#[test]
fn spec_r46_du_reports_resolved_entry_and_descendants_sorted() {
    let mut f = Fs::new();
    f.mkdir("/dir").unwrap();
    f.write("/dir/z", b"123").unwrap();
    f.write("/dir/a", b"1").unwrap();
    f.symlink("/dir", "/alias").unwrap();
    assert_eq!(
        f.du("/alias"),
        Ok(vec![
            ("/dir".into(), 4),
            ("/dir/a".into(), 1),
            ("/dir/z".into(), 3)
        ])
    );
    assert_eq!(f.du("/dir/a"), Ok(vec![("/dir/a".into(), 1)]));
}

#[test]
fn spec_r47_du_sizes_files_links_and_directories() {
    let mut f = Fs::new();
    f.mkdir("/d").unwrap();
    f.write("/d/file", b"abc").unwrap();
    f.symlink("éx", "/d/link").unwrap();
    assert_eq!(
        f.du("/d"),
        Ok(vec![
            ("/d".into(), 5),
            ("/d/file".into(), 3),
            ("/d/link".into(), 3)
        ])
    );
}

#[test]
fn spec_r48_du_counts_stored_links_once_without_following() {
    let mut f = Fs::new();
    f.mkdir("/a").unwrap();
    f.write("/a/file", b"data").unwrap();
    f.symlink("/a", "/a/loop").unwrap();
    f.symlink("/missing", "/dangling").unwrap();
    assert_eq!(
        f.du("/"),
        Ok(vec![
            ("/".into(), 4),
            ("/a".into(), 4),
            ("/a/file".into(), 4),
            ("/a/loop".into(), 2),
            ("/dangling".into(), 8)
        ])
    );
}

#[test]
fn spec_r49_du_reflects_current_state_and_reports_resolution_errors() {
    let mut f = Fs::new();
    f.mkdir("/d").unwrap();
    f.write("/d/file", b"long").unwrap();
    assert_eq!(f.du("/d")[0], ("/d".into(), 4));
    f.write("/d/file", b"x").unwrap();
    assert_eq!(f.du("/d")[0], ("/d".into(), 1));
    f.symlink("/loop", "/loop").unwrap();
    assert_eq!(f.du("/loop"), Err(Error::Loop("/loop".into())));
}

#[test]
fn spec_r50_failed_link_mutations_preserve_state_and_tracking_is_local() {
    let mut f = Fs::new();
    f.write("/data", b"original").unwrap();
    f.symlink("/loop", "/loop").unwrap();
    f.mkdir("/d").unwrap();
    f.chdir("/d").unwrap();
    assert_eq!(f.write("/loop", b"changed"), Err(Error::Loop("/loop".into())));
    assert_eq!(f.read("/data"), Ok(b"original".to_vec()));
    assert_eq!(f.cwd(), "/d");
    f.remove("/loop", false).unwrap();
    assert_eq!(f.read("/data"), Ok(b"original".to_vec()));
}
