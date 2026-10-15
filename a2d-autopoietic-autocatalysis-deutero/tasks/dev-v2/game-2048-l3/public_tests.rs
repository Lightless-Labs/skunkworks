fn b(w:usize,h:usize,c:&[u32])->Board { Board {width:w,height:h,cells:c.to_vec()} }
fn mg(a:usize,b:usize,to:usize,value:u32)->Merge { Merge {from:[a,b],to,value} }
// R7-R20
#[test]
fn public_board_left() {
let x=b(2,2,&[2,2,0,4]);let r=move_board(&x,Direction::Left).unwrap();assert_eq!(r,MoveResult {board:b(2,2,&[4,0,4,0]),score:4,changed:true,merges:vec![mg(0,1,0,4)]});
}

// R4,R21-R24
#[test]
fn public_board_legality() {
assert_eq!(legal_moves(&b(2,2,&[2,0,0,0])).unwrap(),vec![Direction::Down,Direction::Right]);assert_eq!(move_board(&b(0,2,&[]),Direction::Left),Err(Error::Shape));
}

// R26,R28-R31
#[test]
fn public_spawn_seeded() {
let x=b(2,2,&vec![2,0,0,0]);let old=x.clone();let r=spawn_tile(&x,5).unwrap();assert_eq!(r,SpawnResult {board:b(2,2,&vec![2,2,0,0]),rng:2587011477941047999,spawn:Some(Spawn {index:1,value:2})});assert_eq!(x,old);
}

// R33,R35-R36
#[test]
fn public_turn_merge_spawn() {
let x=b(2,2,&[2,2,0,0]);let t=play_turn(&x,5,Direction::Left).unwrap();assert!(t.changed);assert_eq!(t.score,4);assert_eq!(t.merges,vec![mg(0,1,0,4)]);let s=spawn_tile(&b(2,2,&[4,0,0,0]),5).unwrap();assert_eq!(t.board,s.board);assert_eq!(t.rng,s.rng);assert_eq!(t.spawn,s.spawn);
}
