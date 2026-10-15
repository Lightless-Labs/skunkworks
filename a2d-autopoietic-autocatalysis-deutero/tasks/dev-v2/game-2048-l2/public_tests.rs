fn b(w:usize,h:usize,c:&[u32])->Board { Board {width:w,height:h,cells:c.to_vec()} }
fn mg(a:usize,b:usize,to:usize,value:u32)->Merge { Merge {from:[a,b],to,value} }
// R7-R20
#[test]
fn public_board_left() {
let x=b(2,2,&[2,2,0,4]);let r=move_board(&x,Direction::Left).unwrap();assert_eq!(r,MoveResult {board:b(2,2,&[4,0,4,0]),score:4,changed:true,merges:vec![mg(0,1,0,4)]});
}

// R8,R14,R18-R20
#[test]
fn public_board_up() {
let r=move_board(&b(2,2,&[0,2,4,2]),Direction::Up).unwrap();assert_eq!(r.board.cells,vec![4,4,0,0]);assert_eq!(r.score,4);
}

// R4,R21-R24
#[test]
fn public_board_legality() {
assert_eq!(legal_moves(&b(2,2,&[2,0,0,0])).unwrap(),vec![Direction::Down,Direction::Right]);assert_eq!(move_board(&b(0,2,&[]),Direction::Left),Err(Error::Shape));
}
