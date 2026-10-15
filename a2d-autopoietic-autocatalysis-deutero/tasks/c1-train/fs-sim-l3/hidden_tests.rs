// R1, R2, R14, R10, R9, R7, R8.
#[test]
fn hidden_initial_and_root() {
    let mut f=Fs::new(); assert_eq!(f.ls("/"),Ok(vec![])); assert_eq!(f.mkdir("/"),Err(Error::Exists("/".into()))); assert_eq!(f.write("/",b"!"),Err(Error::IsDir("/".into()))); assert_eq!(f.read("/"),Err(Error::IsDir("/".into())));
}

// R3, R18, R12, R23.
#[test]
fn hidden_nul_priority_bytes() {
    let mut f=Fs::new(); assert_eq!(f.mkdir("é//z\0q"),Err(Error::InvalidPath(5))); assert_eq!(f.read("a\0b\0"),Err(Error::InvalidPath(1))); assert_eq!(f.write("",b"z"),Err(Error::InvalidPath(0))); assert_eq!(f.ls("/"),Ok(vec![]));
}

// R5, R6, R18.
#[test]
fn hidden_missing_before_type() {
    let mut f=Fs::new(); assert_eq!(f.mkdir("/void/deep/new"),Err(Error::Missing("/void".into()))); assert_eq!(f.write("/void/deep/file",b"z"),Err(Error::Missing("/void".into()))); f.write("/leaf",b"x").unwrap(); assert_eq!(f.read("/leaf/child"),Err(Error::NotDir("/leaf".into()))); assert_eq!(f.mkdir("/leaf/child"),Err(Error::NotDir("/leaf".into())));
}

// R7, R8, R17, R12.
#[test]
fn hidden_collisions_preserve() {
    let mut f=Fs::new(); f.write("/persist",b"stay").unwrap(); assert_eq!(f.mkdir("/persist"),Err(Error::Exists("/persist".into()))); f.mkdir("/folder").unwrap(); assert_eq!(f.mkdir("/folder"),Err(Error::Exists("/folder".into()))); assert_eq!(f.write("/folder",b"lost"),Err(Error::IsDir("/folder".into()))); assert_eq!(f.read("/persist"),Ok(b"stay".to_vec())); assert_eq!(f.ls("/folder"),Ok(vec![]));
}

// R13, R9, R12.
#[test]
fn hidden_owned_binary_truncation() {
    let mut f=Fs::new(); let mut input=vec![0,255,128,7]; f.write("/blob",&input).unwrap(); input[0]=99; let mut got=f.read("/blob").unwrap(); got[1]=1; assert_eq!(f.read("/blob"),Ok(vec![0,255,128,7])); f.write("/blob",b"").unwrap(); assert_eq!(f.read("/blob"),Ok(vec![]));
}

// R11, R15, R16, R10, R17.
#[test]
fn hidden_order_and_immediate() {
    let mut f=Fs::new(); f.mkdir("/nest").unwrap(); for name in ["é","a","Z"," ","a.b","a\\b"] { f.write(&format!("/nest/{name}"),b"d").unwrap(); } f.mkdir("/nest/sub").unwrap(); f.write("/nest/sub/hidden",b"h").unwrap(); assert_eq!(f.ls("/nest").unwrap().into_iter().map(|x|x.0).collect::<Vec<_>>(),vec![" ","Z","a","a.b","a\\b","sub","é"]); assert_eq!(f.ls("/nest/sub"),Ok(vec![("hidden".into(),Kind::File)]));
}

// R2, R12, R9.
#[test]
fn hidden_instance_isolation() {
    let mut a=Fs::new(); let b=Fs::new(); a.write("/unique",b"a").unwrap(); assert_eq!(b.read("/unique"),Err(Error::Missing("/unique".into()))); let mut listing=a.ls("/").unwrap(); listing.clear(); assert_eq!(a.ls("/").unwrap().len(),1);
}

// R5, R6, R18.
#[test]
fn hidden_deep_prefix() {
    let mut f=Fs::new(); f.mkdir("/branch").unwrap(); f.mkdir("/branch/twig").unwrap(); assert_eq!(f.read("/branch/twig/gone/end"),Err(Error::Missing("/branch/twig/gone".into()))); f.write("/branch/twig/f",b"f").unwrap(); assert_eq!(f.ls("/branch/twig/f/x"),Err(Error::NotDir("/branch/twig/f".into())));
}

// R13, R15, R10, R11.
#[test]
fn hidden_empty_file_kind() {
    let mut f=Fs::new(); f.write("/zero",&[]).unwrap(); f.mkdir("/empty").unwrap(); assert_eq!(f.ls("/"),Ok(vec![("empty".into(),Kind::Dir),("zero".into(),Kind::File)]));
}

// R9, R10, R5.
#[test]
fn hidden_read_dir_ls_file() {
    let mut f=Fs::new(); f.mkdir("/d").unwrap(); f.write("/f",b"f").unwrap(); assert_eq!(f.read("/d"),Err(Error::IsDir("/d".into()))); assert_eq!(f.ls("/f"),Err(Error::NotDir("/f".into())));
}

// R11, R15, R8, R9.
#[test]
fn hidden_large_sorted_listing() {
    let mut f=Fs::new(); f.mkdir("/bulk").unwrap(); for i in (0..128).rev() { f.write(&format!("/bulk/{i:03}"),&[i as u8]).unwrap(); } let got=f.ls("/bulk").unwrap(); assert_eq!(got.len(),128); for i in 0..128 { assert_eq!(got[i],(format!("{i:03}"),Kind::File)); assert_eq!(f.read(&format!("/bulk/{i:03}")),Ok(vec![i as u8])); }
}

// R16, R17.
#[test]
fn hidden_case_sensitive() {
    let mut f=Fs::new(); f.write("/Case",b"upper").unwrap(); f.write("/case",b"lower").unwrap(); assert_eq!(f.read("/Case"),Ok(b"upper".to_vec())); assert_eq!(f.read("/case"),Ok(b"lower".to_vec()));
}

// R19, R20, R24.
#[test]
fn hidden_cwd_and_relative() {
    let mut f=Fs::new(); assert_eq!(f.cwd(),"/"); f.mkdir("/zone").unwrap(); f.chdir("zone").unwrap(); let mut c=f.cwd(); c.clear(); f.write("value",b"v").unwrap(); f.mkdir("child").unwrap(); assert_eq!(f.read("value"),Ok(b"v".to_vec())); assert_eq!(f.ls("child"),Ok(vec![])); assert_eq!(f.cwd(),"/zone");
}

// R4, R21, R22, R18.
#[test]
fn hidden_lexical_erasure() {
    let mut f=Fs::new(); f.write("/present",b"p").unwrap(); assert_eq!(f.read("/absent/../present/"),Ok(b"p".to_vec())); assert_eq!(f.read("/present/../present"),Ok(b"p".to_vec())); assert_eq!(f.mkdir("//a/..//."),Err(Error::Exists("/".into()))); assert_eq!(f.write("../../..",b"x"),Err(Error::IsDir("/".into())));
}

// R23, R4, R5, R24.
#[test]
fn hidden_normalized_error_paths() {
    let mut f=Fs::new(); f.mkdir("/anchor").unwrap(); f.chdir("/anchor").unwrap(); assert_eq!(f.read("./missing//tail"),Err(Error::Missing("/anchor/missing".into()))); assert_eq!(f.write("../lost/../gone/x",b"v"),Err(Error::Missing("/gone".into()))); assert_eq!(f.cwd(),"/anchor");
}

// R20, R24, R12, R3.
#[test]
fn hidden_chdir_failure_atomic() {
    let mut f=Fs::new(); f.mkdir("/safe").unwrap(); f.write("/plain",b"p").unwrap(); f.chdir("/safe").unwrap(); for (p,e) in [("/plain",Error::NotDir("/plain".into())),("/gone",Error::Missing("/gone".into())),("",Error::InvalidPath(0)),("a\0",Error::InvalidPath(1))] { assert_eq!(f.chdir(p),Err(e)); assert_eq!(f.cwd(),"/safe"); }
}

// R22, R17, R7, R4.
#[test]
fn hidden_normalized_collision() {
    let mut f=Fs::new(); f.mkdir("/u").unwrap(); f.write("/u/n",b"n").unwrap(); assert_eq!(f.mkdir("/u//./n/"),Err(Error::Exists("/u/n".into()))); f.write("/u/x/../n/",b"new").unwrap(); assert_eq!(f.read("/u/n"),Ok(b"new".to_vec()));
}

// R19, R22, R4.
#[test]
fn hidden_relative_parent_saturation() {
    let mut f=Fs::new(); f.mkdir("/a").unwrap(); f.mkdir("/a/b").unwrap(); f.chdir("/a/b").unwrap(); f.write("../../../../z",b"z").unwrap(); assert_eq!(f.read("/z"),Ok(b"z".to_vec())); f.chdir("../../../.").unwrap(); assert_eq!(f.cwd(),"/");
}

// R25, R12, R3.
#[test]
fn hidden_rename_validation_priority() {
    let mut f=Fs::new(); assert_eq!(f.rename("/absent","x\0",false),Err(Error::InvalidPath(1))); assert_eq!(f.rename("\0","",false),Err(Error::InvalidPath(0))); assert_eq!(f.rename("/absent","/",true),Err(Error::Root)); assert_eq!(f.rename("/","/absent",false),Err(Error::Root));
}

// R26, R27, R34, R36.
#[test]
fn hidden_busy_before_same_overwrite() {
    let mut f=Fs::new(); f.mkdir("/live").unwrap(); f.mkdir("/live/sub").unwrap(); f.mkdir("/idle").unwrap(); f.chdir("/live/sub").unwrap(); assert_eq!(f.rename("/live","/no/target",true),Err(Error::Busy("/live".into()))); assert_eq!(f.rename("/live/sub","/live/sub",false),Err(Error::Busy("/live/sub".into()))); assert_eq!(f.rename("/idle","/live/sub",false),Err(Error::Busy("/live/sub".into()))); assert_eq!(f.remove("/live",true),Err(Error::Busy("/live".into()))); assert_eq!(f.cwd(),"/live/sub");
}

// R27, R26.
#[test]
fn hidden_same_and_missing_precedence() {
    let mut f=Fs::new(); f.write("/keep",b"k").unwrap(); assert_eq!(f.rename("/keep","//keep/",false),Ok(())); assert_eq!(f.rename("/unknown","/unknown",true),Err(Error::Missing("/unknown".into()))); assert_eq!(f.rename("/unknown","/void/x",false),Err(Error::Missing("/unknown".into())));
}

// R28, R26, R29.
#[test]
fn hidden_cycle_and_parent_order() {
    let mut f=Fs::new(); f.mkdir("/tree").unwrap(); f.mkdir("/tree/sub").unwrap(); assert_eq!(f.rename("/tree","/tree/sub",false),Err(Error::Cycle("/tree/sub".into()))); assert_eq!(f.rename("/tree","/tree/lost/x",true),Err(Error::Missing("/tree/lost".into())));
}

// R29, R30, R31, R12.
#[test]
fn hidden_overwrite_error_order() {
    let mut f=Fs::new(); f.mkdir("/d").unwrap(); f.mkdir("/e").unwrap(); f.write("/e/k",b"k").unwrap(); f.write("/v",b"v").unwrap(); assert_eq!(f.rename("/d","/v",false),Err(Error::Exists("/v".into()))); assert_eq!(f.rename("/d","/v",true),Err(Error::NotDir("/v".into()))); assert_eq!(f.rename("/v","/d",true),Err(Error::IsDir("/d".into()))); assert_eq!(f.rename("/d","/e",true),Err(Error::NotEmpty("/e".into()))); assert_eq!(f.read("/e/k"),Ok(b"k".to_vec())); assert_eq!(f.read("/v"),Ok(b"v".to_vec()));
}

// R32, R31, R36, R28.
#[test]
fn hidden_move_empty_target_prefix_boundary() {
    let mut f=Fs::new(); f.mkdir("/a").unwrap(); f.mkdir("/a/sub").unwrap(); f.write("/a/sub/p",b"p").unwrap(); f.mkdir("/ab").unwrap(); f.write("/ab/sibling",b"s").unwrap(); f.mkdir("/dest").unwrap(); f.rename("/a","/dest",true).unwrap(); assert_eq!(f.read("/dest/sub/p"),Ok(b"p".to_vec())); assert_eq!(f.read("/ab/sibling"),Ok(b"s".to_vec())); assert_eq!(f.ls("/a"),Err(Error::Missing("/a".into()))); f.rename("/dest","/ab/nested",false).unwrap(); assert_eq!(f.read("/ab/nested/sub/p"),Ok(b"p".to_vec()));
}

// R33, R35, R34, R12, R36.
#[test]
fn hidden_remove_root_and_nonempty() {
    let mut f=Fs::new(); f.mkdir("/a").unwrap(); f.write("/a/v",b"v").unwrap(); f.mkdir("/ab").unwrap(); f.write("/ab/v",b"s").unwrap(); assert_eq!(f.remove("//x/..",true),Err(Error::Root)); assert_eq!(f.remove("/a",false),Err(Error::NotEmpty("/a".into()))); assert_eq!(f.read("/a/v"),Ok(b"v".to_vec())); f.remove("/a",true).unwrap(); assert_eq!(f.read("/ab/v"),Ok(b"s".to_vec())); assert_eq!(f.remove("/gone",true),Err(Error::Missing("/gone".into()))); f.remove("/ab/v",true).unwrap(); f.remove("/ab",false).unwrap(); assert_eq!(f.ls("/"),Ok(vec![]));
}

// R26, R12.
#[test]
fn hidden_destination_parent_file() {
    let mut f=Fs::new(); f.write("/source",b"s").unwrap(); f.write("/blocked",b"b").unwrap(); assert_eq!(f.rename("/source","/blocked/x",true),Err(Error::NotDir("/blocked".into()))); assert_eq!(f.read("/source"),Ok(b"s".to_vec()));
}

// R32, R35, R19, R36.
#[test]
fn hidden_relative_mutations() {
    let mut f=Fs::new(); f.mkdir("/space").unwrap(); f.chdir("/space").unwrap(); f.write("old",b"o").unwrap(); f.rename("./old","x/../new",false).unwrap(); assert_eq!(f.read("new"),Ok(b"o".to_vec())); f.remove("new/",false).unwrap(); assert_eq!(f.cwd(),"/space");
}

// R34, R35, R36.
#[test]
fn hidden_remove_busy_before_nonempty() {
    let mut f=Fs::new(); f.mkdir("/here").unwrap(); f.mkdir("/here/deep").unwrap(); f.chdir("/here").unwrap(); assert_eq!(f.remove(".",false),Err(Error::Busy("/here".into()))); assert_eq!(f.remove(".",true),Err(Error::Busy("/here".into()))); f.remove("deep",false).unwrap(); assert_eq!(f.cwd(),"/here");
}

// R26, R5.
#[test]
fn hidden_rename_source_parent_first() {
    let mut f=Fs::new(); f.write("/block",b"b").unwrap(); assert_eq!(f.rename("/block/sub","/missing/sub",false),Err(Error::NotDir("/block".into())));
}

// R32, R29, R36.
#[test]
fn hidden_file_overwrite_and_sibling() {
    let mut f=Fs::new(); f.write("/f",b"fresh").unwrap(); f.write("/g",b"stale").unwrap(); f.write("/ff",b"sibling").unwrap(); assert_eq!(f.rename("/f","/g",false),Err(Error::Exists("/g".into()))); f.rename("/f","/g",true).unwrap(); assert_eq!(f.read("/g"),Ok(b"fresh".to_vec())); assert_eq!(f.read("/ff"),Ok(b"sibling".to_vec()));
}
