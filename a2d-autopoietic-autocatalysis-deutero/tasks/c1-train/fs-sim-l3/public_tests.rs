#[test]
fn public_file_roundtrip() {
    let mut f=Fs::new(); f.mkdir("/docs").unwrap(); f.write("/docs/item", b"hello").unwrap(); assert_eq!(f.read("/docs/item"), Ok(b"hello".to_vec()));
}

#[test]
fn public_move_tree() {
    let mut f=Fs::new(); f.mkdir("/old").unwrap(); f.write("/old/n",b"n").unwrap(); f.rename("/old","/new",false).unwrap(); assert_eq!(f.read("/new/n"),Ok(b"n".to_vec())); assert_eq!(f.ls("/old"),Err(Error::Missing("/old".into())));
}

#[test]
fn public_overwrite_file() {
    let mut f=Fs::new(); f.write("/x",b"x").unwrap(); f.write("/y",b"y").unwrap(); f.rename("/x","/y",true).unwrap(); assert_eq!(f.read("/y"),Ok(b"x".to_vec()));
}

#[test]
fn public_recursive_remove() {
    let mut f=Fs::new(); f.mkdir("/work").unwrap(); f.chdir("/work").unwrap(); assert_eq!(f.cwd(),"/work"); f.mkdir("sub").unwrap(); f.write("sub/v",b"v").unwrap(); f.remove("sub",true).unwrap(); assert_eq!(f.ls("."),Ok(vec![]));
}
