#[test]
fn public_file_roundtrip() {
    let mut f=Fs::new(); f.mkdir("/docs").unwrap(); f.write("/docs/item", b"hello").unwrap(); assert_eq!(f.read("/docs/item"), Ok(b"hello".to_vec()));
}

#[test]
fn public_follow_link() {
    let mut f=Fs::new(); f.symlink("/docs/item","/shortcut").unwrap(); f.mkdir("/docs").unwrap(); f.write("/docs/item",b"linked").unwrap(); assert_eq!(f.read("/shortcut"),Ok(b"linked".to_vec())); assert_eq!(f.read_link("/shortcut"),Ok("/docs/item".into()));
}

#[test]
fn public_link_directory_cwd() {
    let mut f=Fs::new(); f.mkdir("/place").unwrap(); f.symlink("/place","/alias").unwrap(); f.chdir("/alias").unwrap(); assert_eq!(f.cwd(),"/place"); f.write("k",b"k").unwrap(); f.rename("k","j",false).unwrap(); f.remove("j",false).unwrap(); assert_eq!(f.ls("/alias"),Ok(vec![]));
}

#[test]
fn public_size_report() {
    let mut f=Fs::new(); f.mkdir("/pkg").unwrap(); f.write("/pkg/a",b"123").unwrap(); f.mkdir("/pkg/empty").unwrap(); assert_eq!(f.du("/pkg"),Ok(vec![("/pkg".into(),3),("/pkg/a".into(),3),("/pkg/empty".into(),0)]));
}
