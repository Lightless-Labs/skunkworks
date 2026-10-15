#[test]
fn public_file_roundtrip() {
    let mut f=Fs::new(); f.mkdir("/docs").unwrap(); f.write("/docs/item", b"hello").unwrap(); assert_eq!(f.read("/docs/item"), Ok(b"hello".to_vec()));
}

#[test]
fn public_sorted_children() {
    let mut f=Fs::new(); f.mkdir("/box").unwrap(); f.write("/box/z", b"z").unwrap(); f.mkdir("/box/a").unwrap(); assert_eq!(f.ls("/box"), Ok(vec![("a".into(),Kind::Dir),("z".into(),Kind::File)]));
}

#[test]
fn public_replace_bytes() {
    let mut f=Fs::new(); f.write("/memo", b"abcdef").unwrap(); f.write("/memo", b"xy").unwrap(); assert_eq!(f.read("/memo"), Ok(b"xy".to_vec()));
}

#[test]
fn public_lookup_errors() {
    let mut f=Fs::new(); assert_eq!(f.read("/absent"), Err(Error::Missing("/absent".into()))); f.write("/plain",b"p").unwrap(); assert_eq!(f.ls("/plain"),Err(Error::NotDir("/plain".into())));
}
