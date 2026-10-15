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

// R37, R38, R12, R3.
#[test]
fn hidden_symlink_validation_order() {
    let mut f=Fs::new(); assert_eq!(f.symlink("","x\0"),Err(Error::InvalidPath(1))); assert_eq!(f.symlink("é\0","/absent/x"),Err(Error::InvalidTarget(2))); assert_eq!(f.symlink("","/absent/x"),Err(Error::InvalidTarget(0))); assert_eq!(f.symlink("/z","/"),Err(Error::Exists("/".into()))); f.symlink("/z","/l").unwrap(); assert_eq!(f.symlink("/q","/l"),Err(Error::Exists("/l".into()))); assert_eq!(f.read_link("/l"),Ok("/z".into()));
}

// R38, R43, R42.
#[test]
fn hidden_raw_target_and_readlink_types() {
    let mut f=Fs::new(); f.symlink(" ../x//./é/ ","/raw").unwrap(); assert_eq!(f.read_link("/raw"),Ok(" ../x//./é/ ".into())); f.write("/f",b"f").unwrap(); assert_eq!(f.read_link("/f"),Err(Error::InvalidTarget(0))); assert_eq!(f.read_link("/"),Err(Error::InvalidTarget(0))); assert_eq!(f.read_link("/missing"),Err(Error::Missing("/missing".into())));
}

// R39, R42, R20, R45.
#[test]
fn hidden_relative_target_physical_parent() {
    let mut f=Fs::new(); f.mkdir("/physical").unwrap(); f.mkdir("/physical/sub").unwrap(); f.write("/physical/v",b"v").unwrap(); f.symlink("../v","/physical/sub/l").unwrap(); f.symlink("/physical/sub","/alias").unwrap(); f.chdir("/alias").unwrap(); assert_eq!(f.cwd(),"/physical/sub"); assert_eq!(f.read("/alias/l"),Ok(b"v".to_vec())); assert_eq!(f.read_link("/alias/l"),Ok("../v".into())); f.mkdir("/alias/new").unwrap(); assert_eq!(f.ls("/physical/sub/new"),Ok(vec![]));
}

// R44, R39, R8, R12.
#[test]
fn hidden_dangling_write_and_intermediate() {
    let mut f=Fs::new(); f.mkdir("/real").unwrap(); f.symlink("/real/new","/dangling").unwrap(); assert_eq!(f.read("/dangling"),Err(Error::Missing("/real/new".into()))); f.write("/dangling",b"made").unwrap(); assert_eq!(f.read("/real/new"),Ok(b"made".to_vec())); assert_eq!(f.read_link("/dangling"),Ok("/real/new".into())); f.symlink("/missing/deep","/bad").unwrap(); assert_eq!(f.write("/bad",b"x"),Err(Error::Missing("/missing".into()))); assert_eq!(f.read("/dangling/x"),Err(Error::NotDir("/real/new".into())));
}

// R40, R45, R43, R35, R15, R50.
#[test]
fn hidden_loop_and_nofollow_cleanup() {
    let mut f=Fs::new(); f.symlink("/second","/first").unwrap(); f.symlink("/first","/second").unwrap(); assert_eq!(f.read("/first"),Err(Error::Loop("/first".into()))); assert_eq!(f.read_link("/first"),Ok("/second".into())); assert_eq!(f.symlink("/new","/first"),Err(Error::Exists("/first".into()))); assert_eq!(f.mkdir("/first"),Err(Error::Exists("/first".into()))); assert_eq!(f.ls("/"),Ok(vec![("first".into(),Kind::Link),("second".into(),Kind::Link)])); f.remove("/first",true).unwrap(); assert_eq!(f.read_link("/second"),Ok("/first".into())); f.write("/plain",b"p").unwrap(); assert_eq!(f.read("/plain"),Ok(b"p".to_vec()));
}

// R41, R40, R50.
#[test]
fn hidden_exact_eight_and_ninth() {
    let mut f=Fs::new(); f.write("/end",b"ok").unwrap(); for i in (0..9).rev() { let target=if i==8 {"/end".to_owned()} else {format!("/l{}",i+1)}; f.symlink(&target,&format!("/l{i}")).unwrap(); } assert_eq!(f.read("/l1"),Ok(b"ok".to_vec())); assert_eq!(f.read("/l0"),Err(Error::Depth("/l8".into()))); assert_eq!(f.du("/l0"),Err(Error::Depth("/l8".into()))); assert_eq!(f.read("/l1"),Ok(b"ok".to_vec()));
}

// R41, R40.
#[test]
fn hidden_repeat_before_depth() {
    let mut f=Fs::new(); for i in 0..8 { f.symlink(&format!("/c{}",(i+1)%8),&format!("/c{i}")).unwrap(); } assert_eq!(f.read("/c0"),Err(Error::Loop("/c0".into())));
}

// R40, R39, R21.
#[test]
fn hidden_revisit_in_suffix() {
    let mut f=Fs::new(); f.mkdir("/d").unwrap(); f.write("/d/f",b"f").unwrap(); f.symlink("/d","/d/up").unwrap(); assert_eq!(f.read("/d/up/up/f"),Err(Error::Loop("/d/up".into()))); assert_eq!(f.read("/d/up/f"),Ok(b"f".to_vec())); assert_eq!(f.read("/d/up/../f"),Ok(b"f".to_vec()));
}

// R42, R41, R45, R39.
#[test]
fn hidden_link_parent_budget_and_file() {
    let mut f=Fs::new(); f.mkdir("/real").unwrap(); f.write("/real/f",b"f").unwrap(); for i in (0..9).rev() { let t=if i==8 {"/real".to_owned()} else {format!("/p{}",i+1)}; f.symlink(&t,&format!("/p{i}")).unwrap(); } assert_eq!(f.mkdir("/p0/new"),Err(Error::Depth("/p8".into()))); f.mkdir("/p1/new").unwrap(); assert_eq!(f.symlink("/x","/p1/f/child"),Err(Error::NotDir("/real/f".into()))); assert_eq!(f.read_link("/p1/f"),Err(Error::InvalidTarget(0)));
}

// R32, R30, R45, R42.
#[test]
fn hidden_move_relative_link_overwrite() {
    let mut f=Fs::new(); f.mkdir("/left").unwrap(); f.mkdir("/right").unwrap(); f.write("/left/v",b"L").unwrap(); f.write("/right/v",b"R").unwrap(); f.symlink("v","/left/l").unwrap(); f.write("/right/l",b"old").unwrap(); f.rename("/left/l","/right/l",true).unwrap(); assert_eq!(f.read("/right/l"),Ok(b"R".to_vec())); assert_eq!(f.read_link("/right/l"),Ok("v".into())); f.write("/replacement",b"new").unwrap(); f.rename("/replacement","/right/l",true).unwrap(); assert_eq!(f.read("/right/l"),Ok(b"new".to_vec())); assert_eq!(f.read("/right/v"),Ok(b"R".to_vec()));
}

// R42, R27, R26, R35.
#[test]
fn hidden_parent_alias_mutations() {
    let mut f=Fs::new(); f.mkdir("/store").unwrap(); f.write("/store/f",b"f").unwrap(); f.symlink("/store","/alias").unwrap(); assert_eq!(f.rename("/alias/f","/store/f",false),Ok(())); f.rename("/alias/f","/alias/g",false).unwrap(); f.remove("/alias/g",false).unwrap(); assert_eq!(f.ls("/store"),Ok(vec![])); assert_eq!(f.read_link("/alias"),Ok("/store".into()));
}

// R46, R47, R48, R15.
#[test]
fn hidden_du_physical_links_utf8() {
    let mut f=Fs::new(); f.mkdir("/d").unwrap(); f.mkdir("/d/empty").unwrap(); f.write("/d/f",&[0,255]).unwrap(); f.symlink("é","/d/l").unwrap(); f.symlink("/d","/d/self").unwrap(); assert_eq!(f.du("/d"),Ok(vec![("/d".into(),6),("/d/empty".into(),0),("/d/f".into(),2),("/d/l".into(),2),("/d/self".into(),2)]));
}

// R46, R47, R48, R49, R32.
#[test]
fn hidden_du_alias_root_live() {
    let mut f=Fs::new(); f.mkdir("/n").unwrap(); f.write("/n/f",b"12345").unwrap(); f.write("/nn",b"zz").unwrap(); f.symlink("/n","/alias").unwrap(); assert_eq!(f.du("/alias"),Ok(vec![("/n".into(),5),("/n/f".into(),5)])); assert_eq!(f.du("/nn"),Ok(vec![("/nn".into(),2)])); f.write("/n/f",b"").unwrap(); f.rename("/n/f","/n/g",false).unwrap(); assert_eq!(f.du("/"),Ok(vec![("/".into(),4),("/alias".into(),2),("/n".into(),0),("/n/g".into(),0),("/nn".into(),2)])); f.remove("/alias",false).unwrap(); assert_eq!(f.du("/").unwrap()[0],("/".into(),2));
}

// R49, R50, R40, R20, R12.
#[test]
fn hidden_link_error_atomic_and_du_errors() {
    let mut f=Fs::new(); f.mkdir("/safe").unwrap(); f.chdir("/safe").unwrap(); f.symlink("/loop","/loop").unwrap(); f.symlink("/absent/x","/broken").unwrap(); assert_eq!(f.chdir("/loop"),Err(Error::Loop("/loop".into()))); assert_eq!(f.write("/loop",b"bad"),Err(Error::Loop("/loop".into()))); assert_eq!(f.rename("/broken/x","/new",false),Err(Error::Missing("/absent".into()))); assert_eq!(f.remove("/loop/x",true),Err(Error::Loop("/loop".into()))); assert_eq!(f.du("/loop"),Err(Error::Loop("/loop".into()))); assert_eq!(f.du("/broken"),Err(Error::Missing("/absent".into()))); assert_eq!(f.cwd(),"/safe"); assert_eq!(f.read_link("/loop"),Ok("/loop".into())); f.write("ok",b"yes").unwrap(); assert_eq!(f.read("ok"),Ok(b"yes".to_vec()));
}

// R39, R40, R38, R32.
#[test]
fn hidden_expand_normalize_and_independent_parents() {
    let mut f=Fs::new(); f.mkdir("/actual").unwrap(); f.write("/actual/f",b"f").unwrap(); f.symlink("/gone/..//actual/.","/a").unwrap(); assert_eq!(f.read("/a/f"),Ok(b"f".to_vec())); f.rename("/a/f","/a/g",false).unwrap(); assert_eq!(f.read("/actual/g"),Ok(b"f".to_vec())); f.symlink(" ","/space").unwrap(); f.write("/ ",b"s").unwrap(); assert_eq!(f.read("/space"),Ok(b"s".to_vec()));
}

// R46, R47, R48, R49, R42, R14.
#[test]
fn hidden_du_nested_root_alias() {
    let mut f=Fs::new(); f.mkdir("/bundle").unwrap(); f.mkdir("/bundle/deep").unwrap(); f.write("/bundle/deep/data",b"1234567").unwrap(); f.write("/bundle/direct",b"xx").unwrap(); f.symlink("/bundle/deep/data","/bundle/copy").unwrap(); f.symlink("/","/root_alias").unwrap(); assert_eq!(f.du("/bundle"),Ok(vec![("/bundle".into(),26),("/bundle/copy".into(),17),("/bundle/deep".into(),7),("/bundle/deep/data".into(),7),("/bundle/direct".into(),2)])); assert_eq!(f.du("/root_alias").unwrap()[0],("/".into(),27)); assert_eq!(f.write("/root_alias",b"bad"),Err(Error::IsDir("/".into()))); f.remove("/bundle/deep",true).unwrap(); assert_eq!(f.du("/bundle").unwrap()[0],("/bundle".into(),19));
}
