// R1-R3: Rust 2024 compilation and separate-crate API verification apply throughout.
fn b(w:usize,h:usize,c:&[u32])->Board { Board {width:w,height:h,cells:c.to_vec()} }
fn mg(a:usize,b:usize,to:usize,value:u32)->Merge { Merge {from:[a,b],to,value} }
fn st(board:Board,score:u64,rng:u64)->State { State {board,score,rng} }
// R4,R6
#[test]
fn board_shape_dimensions() {
for (w,h) in [(0,1),(1,0),(9,1),(1,9),(usize::MAX,usize::MAX),(1,usize::MAX)] {let x=b(w,h,&[1]);assert_eq!(move_board(&x,Direction::Right),Err(Error::Shape));assert_eq!(legal_moves(&x),Err(Error::Shape));}
}

// R4,R6
#[test]
fn board_shape_length() {
for c in [vec![],vec![0;5],vec![1;7]] {assert_eq!(move_board(&b(3,2,&c),Direction::Down),Err(Error::Shape));}
}

// R5
#[test]
fn board_tile_values() {
for value in [1,3,12,2147483649,u32::MAX] {assert_eq!(move_board(&b(3,1,&[0,value,0]),Direction::Left),Err(Error::InvalidTile {index:1,value}));}
}

// R5-R6,R24
#[test]
fn board_validation_precedence() {
let x=b(2,3,&[2147483648,2147483648,0,7,1,0]);for d in [Direction::Up,Direction::Left,Direction::Down,Direction::Right] {assert_eq!(move_board(&x,d),Err(Error::InvalidTile {index:3,value:7}));}assert_eq!(legal_moves(&x),Err(Error::InvalidTile {index:3,value:7}));
}

// R8-R10,R14-R18
#[test]
fn board_rectangle_up() {
let r=move_board(&b(3,2,&[0,8,2,4,8,0]),Direction::Up).unwrap();assert_eq!(r,MoveResult {board:b(3,2,&[4,16,2,0,0,0]),score:16,changed:true,merges:vec![mg(1,4,1,16)]});
}

// R8-R10,R14-R18
#[test]
fn board_rectangle_down() {
let r=move_board(&b(2,3,&[2,4,0,4,2,8]),Direction::Down).unwrap();assert_eq!(r,MoveResult {board:b(2,3,&[0,0,0,8,4,8]),score:12,changed:true,merges:vec![mg(4,0,4,4),mg(3,1,3,8)]});
}

// R8-R9,R15-R17
#[test]
fn board_right_trace_order() {
let r=move_board(&b(4,2,&[2,2,4,4,8,8,16,16]),Direction::Right).unwrap();assert_eq!(r.board.cells,vec![0,0,4,8,0,0,16,32]);assert_eq!(r.score,60);assert_eq!(r.merges,vec![mg(3,2,3,8),mg(1,0,2,4),mg(7,6,7,32),mg(5,4,6,16)]);
}

// R8-R9,R15-R17
#[test]
fn board_down_column_order() {
let r=move_board(&b(2,4,&[2,8,2,8,4,16,4,16]),Direction::Down).unwrap();assert_eq!(r.board.cells,vec![0,0,0,0,4,16,8,32]);assert_eq!(r.merges,vec![mg(6,4,6,8),mg(2,0,4,4),mg(7,5,7,32),mg(3,1,5,16)]);assert_eq!(r.score,60);
}

// R10-R11
#[test]
fn board_no_chain() {
let r=move_board(&b(4,1,&[8,8,16,16]),Direction::Left).unwrap();assert_eq!(r.board.cells,vec![16,32,0,0]);assert_eq!(r.score,48);assert_eq!(r.merges.len(),2);
}

// R9,R12-R13
#[test]
fn board_right_overflow_priority() {
let x=b(4,2,&[2147483648;8]);assert_eq!(move_board(&x,Direction::Right),Err(Error::MergeOverflow {first:3,second:2}));assert_eq!(move_board(&x,Direction::Left),Err(Error::MergeOverflow {first:0,second:1}));
}

// R9,R12-R13
#[test]
fn board_down_overflow_priority() {
let x=b(2,4,&[2147483648;8]);assert_eq!(move_board(&x,Direction::Down),Err(Error::MergeOverflow {first:6,second:4}));assert_eq!(move_board(&x,Direction::Up),Err(Error::MergeOverflow {first:0,second:2}));
}

// R4-R5,R11,R18
#[test]
fn board_large_score() {
let x=b(8,8,&[1073741824;64]);let r=move_board(&x,Direction::Up).unwrap();assert_eq!(r.score,68719476736);assert_eq!(r.merges.len(),32);assert_eq!(&r.board.cells[..32],&[2147483648;32]);assert_eq!(&r.board.cells[32..],&[0;32]);
}

// R26,R28-R32
#[test]
fn random_seed_boundaries() {
{
let x=b(3,2,&vec![0,0,0,0,0,0]);let old=x.clone();let r=spawn_tile(&x,0).unwrap();assert_eq!(r,SpawnResult {board:b(3,2,&vec![0,2,0,0,0,0]),rng:1876011003808476466,spawn:Some(Spawn {index:1,value:2})});assert_eq!(x,old);
}
{
let x=b(2,3,&vec![0,8,0,16,0,32]);let old=x.clone();let r=spawn_tile(&x,18446744073709551615).unwrap();assert_eq!(r,SpawnResult {board:b(2,3,&vec![0,8,0,16,2,32]),rng:12801857353207693129,spawn:Some(Spawn {index:4,value:2})});assert_eq!(x,old);
}
}

// R29-R31
#[test]
fn random_rank_and_four() {
{
let x=b(4,1,&vec![0,0,8,0]);let old=x.clone();let r=spawn_tile(&x,34).unwrap();assert_eq!(r,SpawnResult {board:b(4,1,&vec![0,0,8,4]),rng:17778860672135693860,spawn:Some(Spawn {index:3,value:4})});assert_eq!(x,old);
}
{
let x=b(3,3,&vec![2,0,4,8,0,16,32,0,64]);let old=x.clone();let r=spawn_tile(&x,17).unwrap();assert_eq!(r,SpawnResult {board:b(3,3,&vec![2,0,4,8,0,16,32,2,64]),rng:604063801117309355,spawn:Some(Spawn {index:7,value:2})});assert_eq!(x,old);
}
}

// R27-R32
#[test]
fn random_single_and_full() {
{
let x=b(3,1,&vec![16,0,8]);let old=x.clone();let r=spawn_tile(&x,123456789).unwrap();assert_eq!(r,SpawnResult {board:b(3,1,&vec![16,2,8]),rng:16626069846921725519,spawn:Some(Spawn {index:1,value:2})});assert_eq!(x,old);
}
{
let x=b(2,2,&[2,4,8,16]);for rng in [0,1,u64::MAX] {assert_eq!(spawn_tile(&x,rng).unwrap(),SpawnResult {board:x.clone(),rng,spawn:None});}
}
}

// R4-R6,R26
#[test]
fn rng_invalid_full_and_shape() {
assert_eq!(spawn_tile(&b(2,1,&[1,3]),0),Err(Error::InvalidTile {index:0,value:1}));assert_eq!(spawn_tile(&b(0,1,&[1]),u64::MAX),Err(Error::Shape));assert_eq!(play_turn(&b(0,1,&[]),0,Direction::Left),Err(Error::Shape));
}

// R23,R34,R37
#[test]
fn random_noop_turns() {
{
for rng in [0,2,u64::MAX] {let x=b(3,2,&[2,4,0,8,16,0]);let t=play_turn(&x,rng,Direction::Left).unwrap();assert_eq!(t,Turn {board:x,rng,score:0,changed:false,merges:vec![],spawn:None});}
}
{
let x=b(2,3,&[0;6]);assert!(legal_moves(&x).unwrap().is_empty());let t=play_turn(&x,2,Direction::Down).unwrap();assert_eq!(t.board,x);assert_eq!(t.rng,2);assert_eq!(t.spawn,None);assert!(!t.changed);assert_eq!(t.score,0);
}
}

// R6,R12-R13,R33
#[test]
fn rng_overflow_before_spawn() {
let x=b(3,1,&[2147483648,0,2147483648]);assert_eq!(play_turn(&x,2,Direction::Right),Err(Error::MergeOverflow {first:2,second:0}));assert_eq!(x.cells,vec![2147483648,0,2147483648]);assert_eq!(play_turn(&b(3,1,&[2147483648,2147483648,1]),0,Direction::Left),Err(Error::InvalidTile{index:2,value:1}));
}

// R33,R35-R36
#[test]
fn random_turn_goldens() {
{
let x=b(3,2,&[0,0,8,0,0,4]);let t=play_turn(&x,11,Direction::Left).unwrap();assert_eq!(t,Turn {board:b(3,2,&vec![8,0,0,4,2,0]),rng:10818909676383954485,score:0,changed:true,merges:vec![],spawn:Some(Spawn {index:4,value:2})});
}
{
let t=play_turn(&b(2,3,&[8,16,0,0,8,16]),91,Direction::Down).unwrap();assert_eq!(t,Turn {board:b(2,3,&vec![0,0,2,0,16,32]),rng:3748173188795547397,score:48,changed:true,merges:vec![mg(4,0,4,16),mg(5,1,5,32)],spawn:Some(Spawn {index:2,value:2})});
}
}

// R28-R32,R37
#[test]
fn random_sequence_and_onecell() {
{
let x=b(1,1,&vec![0]);let old=x.clone();let r=spawn_tile(&x,42).unwrap();assert_eq!(r,SpawnResult {board:b(1,1,&vec![4]),rng:4159066171780167020,spawn:Some(Spawn {index:0,value:4})});assert_eq!(x,old);
}
{
let mut x=b(2,2,&[0;4]);let mut seed=0;let expected=[(3,2,1876011003808476466u64),(1,4,7401132627792533940),(2,4,10346034117385188870),(0,2,6566661184467396264)];for (index,value,next) in expected {let r=spawn_tile(&x,seed).unwrap();assert_eq!(r.spawn,Some(Spawn {index,value}));assert_eq!(r.rng,next);x=r.board;seed=r.rng;}assert_eq!(x.cells,vec![2,4,4,2]);assert_eq!(spawn_tile(&x,seed).unwrap().rng,seed);
}
}

// R25,R37-R39
#[test]
fn game_constructor_and_no_auto_spawn() {
for score in [0,u64::MAX] {for rng in [0,u64::MAX] {let initial=st(b(1,1,&[0]),score,rng);let g=new_game(initial.clone()).unwrap();assert_eq!(snapshot(&g),initial);assert!(!can_undo(&g));}}assert_eq!(new_game(st(b(0,9,&[1]),0,0)),Err(Error::Shape));assert_eq!(new_game(st(b(2,1,&[3,1]),0,0)),Err(Error::InvalidTile{index:0,value:3}));
}

// R3,R7,R40,R48
#[test]
fn game_snapshot_clone_independence() {
let mut g=new_game(st(b(3,1,&[4,4,0]),12,7)).unwrap();step(&mut g,Direction::Left).unwrap();let original=snapshot(&g);let mut copy=snapshot(&g);copy.board.cells.fill(0);copy.score=999;copy.rng=999;assert_eq!(snapshot(&g),original);let mut other=g.clone();assert!(undo(&mut other));assert_eq!(snapshot(&g),original);assert!(can_undo(&g));assert!(!can_undo(&other));let x=b(3,1,&[0,8,8]);let mut m=move_board(&x,Direction::Right).unwrap();m.board.cells.fill(0);assert_eq!(x.cells,vec![0,8,8]);
}

// R18,R28-R32,R35-R36,R38,R41-R42
#[test]
fn game_step_exact_state() {
let initial=st(b(2,3,&[8,16,0,0,8,16]),100,91);let mut g=new_game(initial.clone()).unwrap();let t=step(&mut g,Direction::Down).unwrap();assert_eq!(t,Turn {board:b(2,3,&vec![0,0,2,0,16,32]),rng:3748173188795547397,score:48,changed:true,merges:vec![mg(4,0,4,16),mg(5,1,5,32)],spawn:Some(Spawn {index:2,value:2})});assert_eq!(snapshot(&g),st(t.board.clone(),148,3748173188795547397));assert!(can_undo(&g));assert!(undo(&mut g));assert_eq!(snapshot(&g),initial);
}

// R20,R34,R43,R46-R48
#[test]
fn game_noop_keeps_history() {
let initial=st(b(2,1,&[2,2]),7,0);let mut g=new_game(initial.clone()).unwrap();step(&mut g,Direction::Left).unwrap();let before=snapshot(&g);let t=step(&mut g,Direction::Right).unwrap();assert_eq!(t,Turn {board:before.board.clone(),rng:before.rng,score:0,changed:false,merges:vec![],spawn:None});assert_eq!(snapshot(&g),before);assert!(can_undo(&g));assert!(undo(&mut g));assert_eq!(snapshot(&g),initial);assert!(!undo(&mut g));
}

// R12-R13,R41,R44,R46
#[test]
fn game_merge_error_keeps_history() {
let initial=st(b(3,2,&[2147483648,0,2147483648,0,0,0]),u64::MAX,1);let mut g=new_game(initial.clone()).unwrap();assert!(step(&mut g,Direction::Down).unwrap().changed);let before=g.clone();assert_eq!(step(&mut g,Direction::Left),Err(Error::MergeOverflow{first:3,second:5}));assert_eq!(g,before);assert!(undo(&mut g));assert_eq!(snapshot(&g),initial);
}

// R38-R39,R41,R44
#[test]
fn game_score_error_without_history() {
let initial=st(b(3,1,&[2,2,0]),u64::MAX-3,123);let mut g=new_game(initial.clone()).unwrap();let before=g.clone();assert_eq!(step(&mut g,Direction::Left),Err(Error::ScoreOverflow));assert_eq!(g,before);assert_eq!(snapshot(&g),initial);assert!(!can_undo(&g));
}

// R41,R44,R46
#[test]
fn game_score_error_with_history() {
let initial=st(b(4,1,&[2,2,2,2]),u64::MAX-8,0);let mut g=new_game(initial.clone()).unwrap();step(&mut g,Direction::Left).unwrap();assert_eq!(snapshot(&g).score,u64::MAX);let before=g.clone();assert_eq!(step(&mut g,Direction::Left),Err(Error::ScoreOverflow));assert_eq!(g,before);assert!(undo(&mut g));assert_eq!(snapshot(&g),initial);
}

// R21-R24,R41-R42,R49
#[test]
fn game_max_score_zero_delta() {
let mut g=new_game(st(b(2,1,&[0,2]),u64::MAX,0)).unwrap();let t=step(&mut g,Direction::Left).unwrap();assert_eq!(t.score,0);assert_eq!(snapshot(&g).score,u64::MAX);assert_eq!(snapshot(&g).board.cells,vec![2,2]);assert!(!game_over(&g));assert_eq!(legal_moves(&snapshot(&g).board).unwrap(),vec![Direction::Left,Direction::Right]);assert_eq!(step(&mut g,Direction::Left),Err(Error::ScoreOverflow));assert!(can_undo(&g));
}

// R28-R32,R42,R45-R48
#[test]
fn game_depth_one_and_deterministic_redo() {
let initial=st(b(4,1,&[2,2,0,0]),99,0);let mut g=new_game(initial.clone()).unwrap();step(&mut g,Direction::Left).unwrap();let middle=snapshot(&g);let t=step(&mut g,Direction::Right).unwrap();assert!(t.changed);let final_state=snapshot(&g);assert!(undo(&mut g));assert_eq!(snapshot(&g),middle);assert!(!can_undo(&g));assert!(!undo(&mut g));assert_ne!(snapshot(&g),initial);assert_eq!(step(&mut g,Direction::Right).unwrap(),t);assert_eq!(snapshot(&g),final_state);
}

// R21-R25,R47-R49
#[test]
fn game_game_over_and_pure_queries() {
let cases=[(b(2,2,&[0;4]),true),(b(1,1,&[2147483648]),true),(b(2,2,&[2,4,4,2]),true),(b(2,1,&[2147483648,2147483648]),true),(b(3,1,&[2147483648,2147483648,0]),true),(b(2,2,&[2,4,2,8]),false),(b(3,3,&[0,0,0,0,8,0,0,0,0]),false)];for (board,dead) in cases {let mut g=new_game(st(board,u64::MAX,u64::MAX)).unwrap();let before=g.clone();assert_eq!(game_over(&g),dead);assert_eq!(g,before);assert!(!undo(&mut g));assert_eq!(g,before);}let mut g=new_game(st(b(3,1,&[2,2,0]),0,2)).unwrap();step(&mut g,Direction::Left).unwrap();let old=g.clone();let _=snapshot(&g);let _=can_undo(&g);let _=game_over(&g);assert_eq!(g,old);assert_eq!(legal_moves(&b(3,3,&[0,0,0,0,8,0,0,0,0])).unwrap(),vec![Direction::Up,Direction::Left,Direction::Down,Direction::Right]);assert_eq!(legal_moves(&b(2,2,&[2147483648,2147483648,0,0])).unwrap(),vec![Direction::Down]);
}

// R51,R53
#[test]
fn game_empty_replay() {
let initial=st(b(3,1,&[4096,4096,0]),999,u64::MAX);assert_eq!(replay(initial.clone(),&[]).unwrap(),Replay {state:initial,can_undo:false,events:vec![]});
}

// R4-R6,R51,R55
#[test]
fn game_replay_initial_error() {
for (board,error) in [(b(9,0,&[1]),Error::Shape),(b(3,1,&[2147483648,2147483648,1]),Error::InvalidTile {index:2,value:1})] {let initial=st(board,77,88);for commands in [vec![],vec![Command::Undo,Command::Move(Direction::Left)]] {assert_eq!(replay(initial.clone(),&commands),Err(ReplayError {index:None,error:error.clone(),state:initial.clone(),can_undo:false,events:vec![]}));}}
}

// R34,R43,R47,R50-R53
#[test]
fn game_replay_noop_events() {
let initial=st(b(1,1,&[0]),u64::MAX,u64::MAX);let r=replay(initial.clone(),&[Command::Undo,Command::Move(Direction::Down),Command::Check,Command::Undo]).unwrap();assert_eq!(r,Replay {state:initial.clone(),can_undo:false,events:vec![Event::Undone(false),Event::Moved(Turn {board:initial.board,rng:u64::MAX,score:0,changed:false,merges:vec![],spawn:None}),Event::Checked(true),Event::Undone(false)]});
}

// R41,R44,R50,R52,R54
#[test]
fn game_replay_error_prefix() {
let initial=st(b(3,1,&[2147483648,2147483648,0]),u64::MAX,99);let commands=[Command::Check,Command::Undo,Command::Move(Direction::Left),Command::Undo];assert_eq!(replay(initial.clone(),&commands),Err(ReplayError {index:Some(2),error:Error::MergeOverflow {first:0,second:1},state:initial,can_undo:false,events:vec![Event::Checked(true),Event::Undone(false)]}));
let initial=st(b(4,1,&[2,2,2,2]),u64::MAX-8,0);let turn=Turn {board:b(4,1,&[4,4,0,2]),rng:1876011003808476466,score:8,changed:true,merges:vec![mg(0,1,0,4),mg(2,3,1,4)],spawn:Some(Spawn{index:3,value:2})};assert_eq!(replay(initial,&[Command::Undo,Command::Move(Direction::Left),Command::Check,Command::Move(Direction::Left),Command::Undo]),Err(ReplayError {index:Some(3),error:Error::ScoreOverflow,state:st(turn.board.clone(),u64::MAX,turn.rng),can_undo:true,events:vec![Event::Undone(false),Event::Moved(turn),Event::Checked(false)]}));
}

// R28-R32,R36,R38,R42-R43,R45-R46,R50-R53
#[test]
fn game_replay_exact_golden() {
let initial=st(b(3,2,&[2,0,2,4,4,0]),31,6);let original=initial.clone();let commands=[Command::Move(Direction::Left),Command::Check,Command::Move(Direction::Up),Command::Undo,Command::Move(Direction::Right),Command::Check,Command::Undo,Command::Undo,Command::Move(Direction::Down)];let r=replay(initial.clone(),&commands).unwrap();assert_eq!(r,Replay {state:st(b(3,2,&vec![2,0,2,4,4,2]),31,10107909202251382952),can_undo:true,events:vec![Event::Moved(Turn {board:b(3,2,&vec![4,0,2,8,0,0]),rng:10107909202251382952,score:12,changed:true,merges:vec![mg(0,2,0,4),mg(3,4,3,8)],spawn:Some(Spawn {index:2,value:2})}),Event::Checked(false),Event::Moved(Turn {board:b(3,2,&vec![4,0,2,8,0,0]),rng:10107909202251382952,score:0,changed:false,merges:vec![],spawn:None}),Event::Undone(true),Event::Moved(Turn {board:b(3,2,&vec![0,2,4,0,0,8]),rng:10107909202251382952,score:12,changed:true,merges:vec![mg(2,0,2,4),mg(4,3,5,8)],spawn:Some(Spawn {index:1,value:2})}),Event::Checked(false),Event::Undone(true),Event::Undone(false),Event::Moved(Turn {board:b(3,2,&vec![2,0,2,4,4,2]),rng:10107909202251382952,score:0,changed:true,merges:vec![],spawn:Some(Spawn {index:2,value:2})})]});assert_eq!(initial,original);
}
