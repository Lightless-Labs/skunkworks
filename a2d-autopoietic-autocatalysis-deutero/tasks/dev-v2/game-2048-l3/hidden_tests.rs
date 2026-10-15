// R1-R3: Rust 2024 compilation and separate-crate API verification apply throughout.
fn b(w:usize,h:usize,c:&[u32])->Board { Board {width:w,height:h,cells:c.to_vec()} }
fn mg(a:usize,b:usize,to:usize,value:u32)->Merge { Merge {from:[a,b],to,value} }
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

// R10,R14-R16
#[test]
fn board_zeros_provenance() {
let r=move_board(&b(1,5,&[0,32,0,32,8]),Direction::Up).unwrap();assert_eq!(r.board.cells,vec![64,8,0,0,0]);assert_eq!(r.merges,vec![mg(1,3,0,64)]);
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

// R19-R20
#[test]
fn board_still_and_slide() {
let x=b(3,2,&[0,2,4,0,8,16]);let a=move_board(&x,Direction::Right).unwrap();assert!(!a.changed);assert_eq!(a.score,0);assert!(a.merges.is_empty());let z=move_board(&x,Direction::Left).unwrap();assert!(z.changed);assert_eq!(z.score,0);assert!(z.merges.is_empty());assert_eq!(z.board.cells,vec![2,4,0,8,16,0]);
}

// R21-R22
#[test]
fn board_legal_stable_order() {
assert_eq!(legal_moves(&b(3,3,&[0,0,0,0,16,0,0,0,0])).unwrap(),vec![Direction::Up,Direction::Left,Direction::Down,Direction::Right]);
}

// R12,R21-R23
#[test]
fn board_legal_overflow_skip() {
assert_eq!(legal_moves(&b(2,2,&[2147483648,2147483648,0,0])).unwrap(),vec![Direction::Down]);assert!(legal_moves(&b(2,1,&[2147483648,2147483648])).unwrap().is_empty());
}

// R20,R22-R23,R25
#[test]
fn board_zeros_not_legal() {
assert!(legal_moves(&b(4,3,&[0;12])).unwrap().is_empty());for d in [Direction::Up,Direction::Left,Direction::Down,Direction::Right] {let r=move_board(&b(4,3,&[0;12]),d).unwrap();assert!(!r.changed);assert_eq!(r.score,0);assert!(r.merges.is_empty());}
}

// R4-R5,R25
#[test]
fn board_one_cell() {
for value in [0,2,2048,2147483648] {let x=b(1,1,&[value]);assert!(legal_moves(&x).unwrap().is_empty());for d in [Direction::Up,Direction::Left,Direction::Down,Direction::Right] {let r=move_board(&x,d).unwrap();assert_eq!(r.board,x);assert!(!r.changed);}}
}

// R28-R32,R37
#[test]
fn rng_zero_literal() {
let x=b(3,2,&vec![0,0,0,0,0,0]);let old=x.clone();let r=spawn_tile(&x,0).unwrap();assert_eq!(r,SpawnResult {board:b(3,2,&vec![0,2,0,0,0,0]),rng:1876011003808476466,spawn:Some(Spawn {index:1,value:2})});assert_eq!(x,old);
}

// R28-R32
#[test]
fn rng_max_wrap() {
let x=b(2,3,&vec![0,8,0,16,0,32]);let old=x.clone();let r=spawn_tile(&x,18446744073709551615).unwrap();assert_eq!(r,SpawnResult {board:b(2,3,&vec![0,8,0,16,2,32]),rng:12801857353207693129,spawn:Some(Spawn {index:4,value:2})});assert_eq!(x,old);
}

// R28-R31
#[test]
fn rng_four_selection() {
let x=b(4,1,&vec![0,0,8,0]);let old=x.clone();let r=spawn_tile(&x,34).unwrap();assert_eq!(r,SpawnResult {board:b(4,1,&vec![0,0,8,4]),rng:17778860672135693860,spawn:Some(Spawn {index:3,value:4})});assert_eq!(x,old);
}

// R29-R31
#[test]
fn rng_rank_is_empty_rank() {
let x=b(3,3,&vec![2,0,4,8,0,16,32,0,64]);let old=x.clone();let r=spawn_tile(&x,17).unwrap();assert_eq!(r,SpawnResult {board:b(3,3,&vec![2,0,4,8,0,16,32,2,64]),rng:604063801117309355,spawn:Some(Spawn {index:7,value:2})});assert_eq!(x,old);
}

// R28-R32
#[test]
fn rng_single_empty_two_draws() {
let x=b(3,1,&vec![16,0,8]);let old=x.clone();let r=spawn_tile(&x,123456789).unwrap();assert_eq!(r,SpawnResult {board:b(3,1,&vec![16,2,8]),rng:16626069846921725519,spawn:Some(Spawn {index:1,value:2})});assert_eq!(x,old);
}

// R26-R27
#[test]
fn rng_full_no_draws() {
let x=b(2,2,&[2,4,8,16]);for rng in [0,1,u64::MAX] {assert_eq!(spawn_tile(&x,rng).unwrap(),SpawnResult {board:x.clone(),rng,spawn:None});}
}

// R4-R6,R26
#[test]
fn rng_invalid_full_and_shape() {
assert_eq!(spawn_tile(&b(2,1,&[1,3]),0),Err(Error::InvalidTile {index:0,value:1}));assert_eq!(spawn_tile(&b(0,1,&[1]),u64::MAX),Err(Error::Shape));assert_eq!(play_turn(&b(0,1,&[]),0,Direction::Left),Err(Error::Shape));
}

// R34
#[test]
fn rng_noop_preserves_rng() {
for rng in [0,2,u64::MAX] {let x=b(3,2,&[2,4,0,8,16,0]);let t=play_turn(&x,rng,Direction::Left).unwrap();assert_eq!(t,Turn {board:x,rng,score:0,changed:false,merges:vec![],spawn:None});}
}

// R6,R12-R13,R33
#[test]
fn rng_overflow_before_spawn() {
let x=b(3,1,&[2147483648,0,2147483648]);assert_eq!(play_turn(&x,2,Direction::Right),Err(Error::MergeOverflow {first:2,second:0}));assert_eq!(x.cells,vec![2147483648,0,2147483648]);assert_eq!(play_turn(&b(3,1,&[2147483648,2147483648,1]),0,Direction::Left),Err(Error::InvalidTile{index:2,value:1}));
}

// R33,R35-R36
#[test]
fn rng_slide_then_spawn_metadata() {
let x=b(3,2,&[0,0,8,0,0,4]);let t=play_turn(&x,11,Direction::Left).unwrap();assert_eq!(t,Turn {board:b(3,2,&vec![8,0,0,4,2,0]),rng:10818909676383954485,score:0,changed:true,merges:vec![],spawn:Some(Spawn {index:4,value:2})});
}

// R8-R18,R35-R36
#[test]
fn rng_down_merge_golden() {
let t=play_turn(&b(2,3,&[8,16,0,0,8,16]),91,Direction::Down).unwrap();assert_eq!(t,Turn {board:b(2,3,&vec![0,0,2,0,16,32]),rng:3748173188795547397,score:48,changed:true,merges:vec![mg(4,0,4,16),mg(5,1,5,32)],spawn:Some(Spawn {index:2,value:2})});
}

// R28-R32,R37
#[test]
fn rng_one_cell_spawn() {
let x=b(1,1,&vec![0]);let old=x.clone();let r=spawn_tile(&x,42).unwrap();assert_eq!(r,SpawnResult {board:b(1,1,&vec![4]),rng:4159066171780167020,spawn:Some(Spawn {index:0,value:4})});assert_eq!(x,old);
}

// R23,R34,R37
#[test]
fn rng_empty_turn_no_spawn() {
let x=b(2,3,&[0;6]);assert!(legal_moves(&x).unwrap().is_empty());let t=play_turn(&x,2,Direction::Down).unwrap();assert_eq!(t.board,x);assert_eq!(t.rng,2);assert_eq!(t.spawn,None);assert!(!t.changed);assert_eq!(t.score,0);
}

// R28-R32
#[test]
fn rng_sequential_spawns() {
let mut x=b(2,2,&[0;4]);let mut seed=0;let expected=[(3,2,1876011003808476466u64),(1,4,7401132627792533940),(2,4,10346034117385188870),(0,2,6566661184467396264)];for (index,value,next) in expected {let r=spawn_tile(&x,seed).unwrap();assert_eq!(r.spawn,Some(Spawn {index,value}));assert_eq!(r.rng,next);x=r.board;seed=r.rng;}assert_eq!(x.cells,vec![2,4,4,2]);assert_eq!(spawn_tile(&x,seed).unwrap().rng,seed);
}
