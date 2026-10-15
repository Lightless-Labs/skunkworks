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

// R23, R16, R19.
#[test]
fn hidden_whitespace_names() {
    let mut f=Fs::new(); f.write(" ",b"space").unwrap(); f.write("\\x",b"slash").unwrap(); assert_eq!(f.read("/ "),Ok(b"space".to_vec())); assert_eq!(f.read("/\\x"),Ok(b"slash".to_vec()));
}

// R4, R11, R15, R19.
#[test]
fn hidden_deep_relative_listing() {
    let mut f=Fs::new(); f.mkdir("/rooted").unwrap(); f.mkdir("/rooted/deep").unwrap(); f.chdir("/rooted/deep").unwrap(); f.write("../Q",b"q").unwrap(); f.write("../a",b"a").unwrap(); assert_eq!(f.ls(".."),Ok(vec![("Q".into(),Kind::File),("a".into(),Kind::File),("deep".into(),Kind::Dir)]));
}

// R21, R9, R10.
#[test]
fn hidden_final_slash_file() {
    let mut f=Fs::new(); f.write("/f",b"f").unwrap(); assert_eq!(f.read("/f////"),Ok(b"f".to_vec())); assert_eq!(f.ls("/f/"),Err(Error::NotDir("/f".into())));
}
