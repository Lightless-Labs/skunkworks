#[test]
fn public_file_roundtrip() {
    let mut f=Fs::new(); f.mkdir("/docs").unwrap(); f.write("/docs/item", b"hello").unwrap(); assert_eq!(f.read("/docs/item"), Ok(b"hello".to_vec()));
}

#[test]
fn public_relative_cwd() {
    let mut f=Fs::new(); f.mkdir("/work").unwrap(); f.chdir("/work").unwrap(); f.write("note",b"n").unwrap(); assert_eq!(f.cwd(),"/work"); assert_eq!(f.read("./note"),Ok(b"n".to_vec()));
}

#[test]
fn public_normalized_listing() {
    let mut f=Fs::new(); f.mkdir("//data//").unwrap(); f.write("/data/./a",b"a").unwrap(); assert_eq!(f.ls("/data//"),Ok(vec![("a".into(),Kind::File)]));
}

#[test]
fn public_chdir_parent() {
    let mut f=Fs::new(); f.mkdir("/one").unwrap(); f.chdir("/one").unwrap(); f.chdir("..").unwrap(); assert_eq!(f.cwd(),"/");
}
