// R1, R2, R14, R10, R9, R7, R8.
#[test]
fn hidden_initial_and_root() {
    let mut f=Fs::new(); assert_eq!(f.ls("/"),Ok(vec![])); assert_eq!(f.mkdir("/"),Err(Error::Exists("/".into()))); assert_eq!(f.write("/",b"!"),Err(Error::IsDir("/".into()))); assert_eq!(f.read("/"),Err(Error::IsDir("/".into())));
}

// R3, R18, R12.
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

// R4, R3.
#[test]
fn hidden_relative_invalid() {
    let mut f=Fs::new(); for p in ["name",".","..","a/b"] { assert_eq!(f.mkdir(p),Err(Error::InvalidPath(0))); }
}

// R4, R3.
#[test]
fn hidden_slash_offsets() {
    let mut f=Fs::new(); for (p,i) in [("//",1),("///q",1),("/q//r",3),("/q/",2),("/é//",4)] { assert_eq!(f.mkdir(p),Err(Error::InvalidPath(i))); }
}

// R4, R18.
#[test]
fn hidden_dot_offsets() {
    let f=Fs::new(); for (p,i) in [("/.",1),("/..",1),("/gone/./q",6),("/é/../x",4)] { assert_eq!(f.read(p),Err(Error::InvalidPath(i))); }
}

// R4, R16.
#[test]
fn hidden_dot_substrings() {
    let mut f=Fs::new(); for p in ["/...","/.x","/x..","/..x"] { f.write(p,b"v").unwrap(); assert_eq!(f.read(p),Ok(b"v".to_vec())); }
}

// R4, R18.
#[test]
fn hidden_validation_before_lookup() {
    let f=Fs::new(); assert_eq!(f.read("/missing/../file"),Err(Error::InvalidPath(9))); assert_eq!(f.ls("/missing//file"),Err(Error::InvalidPath(9)));
}

// R12, R14, R6.
#[test]
fn hidden_root_unaffected_failure() {
    let mut f=Fs::new(); assert_eq!(f.write("/a/b",b"v"),Err(Error::Missing("/a".into()))); assert_eq!(f.ls("/"),Ok(vec![])); f.mkdir("/a").unwrap(); assert_eq!(f.ls("/a"),Ok(vec![]));
}

// R6, R5, R12.
#[test]
fn hidden_parent_file_write() {
    let mut f=Fs::new(); f.write("/p",b"keep").unwrap(); assert_eq!(f.write("/p/q",b"bad"),Err(Error::NotDir("/p".into()))); assert_eq!(f.read("/p"),Ok(b"keep".to_vec()));
}

// R9, R10, R7, R5.
#[test]
fn hidden_missing_final_distinct() {
    let mut f=Fs::new(); f.mkdir("/holder").unwrap(); assert_eq!(f.read("/holder/no"),Err(Error::Missing("/holder/no".into()))); assert_eq!(f.ls("/holder/no"),Err(Error::Missing("/holder/no".into()))); f.mkdir("/holder/no").unwrap(); assert_eq!(f.ls("/holder/no"),Ok(vec![]));
}

// R3, R12.
#[test]
fn hidden_empty_all_operations() {
    let mut f=Fs::new(); assert_eq!(f.mkdir(""),Err(Error::InvalidPath(0))); assert_eq!(f.read(""),Err(Error::InvalidPath(0))); assert_eq!(f.ls(""),Err(Error::InvalidPath(0))); assert_eq!(f.write("",b"v"),Err(Error::InvalidPath(0)));
}

// R13, R8.
#[test]
fn hidden_replacement_length() {
    let mut f=Fs::new(); f.write("/length",&vec![3;1024]).unwrap(); f.write("/length",&[8]).unwrap(); assert_eq!(f.read("/length"),Ok(vec![8]));
}

// R15, R10, R7.
#[test]
fn hidden_nested_kind() {
    let mut f=Fs::new(); f.mkdir("/top").unwrap(); f.mkdir("/top/inner").unwrap(); f.write("/top/inner/x",b"x").unwrap(); assert_eq!(f.ls("/"),Ok(vec![("top".into(),Kind::Dir)])); assert_eq!(f.ls("/top"),Ok(vec![("inner".into(),Kind::Dir)]));
}

// R3.
#[test]
fn hidden_first_nul_wins() {
    let f=Fs::new(); assert_eq!(f.read("/é\0x\0"),Err(Error::InvalidPath(3))); assert_eq!(f.read("\0"),Err(Error::InvalidPath(0)));
}

// R16, R11.
#[test]
fn hidden_unicode_distinct() {
    let mut f=Fs::new(); f.write("/é",b"one").unwrap(); f.write("/e\u{301}",b"two").unwrap(); assert_eq!(f.ls("/"),Ok(vec![("e\u{301}".into(),Kind::File),("é".into(),Kind::File)]));
}
