fn b(w:usize,h:usize,c:&[u32])->Board { Board {width:w,height:h,cells:c.to_vec()} }
fn mg(a:usize,b:usize,to:usize,value:u32)->Merge { Merge {from:[a,b],to,value} }
fn st(board:Board,score:u64,rng:u64)->State { State {board,score,rng} }
// R7-R24
#[test]
fn public_board_operations() {
{
let x=b(2,2,&[2,2,0,4]);let r=move_board(&x,Direction::Left).unwrap();assert_eq!(r,MoveResult {board:b(2,2,&[4,0,4,0]),score:4,changed:true,merges:vec![mg(0,1,0,4)]});
}
{
assert_eq!(legal_moves(&b(2,2,&[2,0,0,0])).unwrap(),vec![Direction::Down,Direction::Right]);assert_eq!(move_board(&b(0,2,&[]),Direction::Left),Err(Error::Shape));
}
}

// R26-R36
#[test]
fn public_random_operations() {
{
let x=b(2,2,&vec![2,0,0,0]);let old=x.clone();let r=spawn_tile(&x,5).unwrap();assert_eq!(r,SpawnResult {board:b(2,2,&vec![2,2,0,0]),rng:2587011477941047999,spawn:Some(Spawn {index:1,value:2})});assert_eq!(x,old);
}
{
let x=b(2,2,&[2,2,0,0]);let t=play_turn(&x,5,Direction::Left).unwrap();assert!(t.changed);assert_eq!(t.score,4);assert_eq!(t.merges,vec![mg(0,1,0,4)]);let s=spawn_tile(&b(2,2,&[4,0,0,0]),5).unwrap();assert_eq!(t.board,s.board);assert_eq!(t.rng,s.rng);assert_eq!(t.spawn,s.spawn);
}
}

// R38-R42,R46-R49
#[test]
fn public_game_lifecycle() {
let initial=st(b(2,2,&[2,2,0,0]),10,5);let mut g=new_game(initial.clone()).unwrap();assert_eq!(snapshot(&g),initial);assert!(!can_undo(&g));assert!(!game_over(&g));let t=step(&mut g,Direction::Left).unwrap();assert_eq!(t.score,4);assert_eq!(snapshot(&g).score,14);assert_eq!(snapshot(&g).board,t.board);assert!(can_undo(&g));assert!(undo(&mut g));assert_eq!(snapshot(&g),initial);assert!(!can_undo(&g));assert!(!undo(&mut g));
}

// R50-R53
#[test]
fn public_replay() {
let initial=st(b(2,2,&[2,2,0,0]),0,5);let r=replay(initial.clone(),&[Command::Move(Direction::Left),Command::Undo,Command::Check]).unwrap();assert_eq!(r.state,initial);assert!(!r.can_undo);assert_eq!(r.events.len(),3);assert!(matches!(&r.events[0],Event::Moved(t) if t.score==4));assert_eq!(r.events[1],Event::Undone(true));assert_eq!(r.events[2],Event::Checked(false));
}
