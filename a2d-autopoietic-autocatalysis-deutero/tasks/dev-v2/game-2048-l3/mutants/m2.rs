#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction { Up, Left, Down, Right }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error { Shape, InvalidTile { index: usize, value: u32 }, MergeOverflow { first: usize, second: usize } }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board { pub width: usize, pub height: usize, pub cells: Vec<u32> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merge { pub from: [usize; 2], pub to: usize, pub value: u32 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveResult { pub board: Board, pub score: u64, pub changed: bool, pub merges: Vec<Merge> }

fn validate(board: &Board) -> Result<(), Error> {
    if board.width == 0 || board.height == 0 || board.width > 8 || board.height > 8
        || board.cells.len() != board.width * board.height { return Err(Error::Shape); }
    for (index, &value) in board.cells.iter().enumerate() {
        if value != 0 && (value < 2 || !value.is_power_of_two()) {
            return Err(Error::InvalidTile { index, value });
        }
    }
    Ok(())
}
pub fn move_board(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    validate(board)?;
    let horizontal = matches!(direction, Direction::Left | Direction::Right);
    let lines = if horizontal { board.height } else { board.width };
    let length = if horizontal { board.width } else { board.height };
    let mut result = board.clone();
    let mut merges = Vec::new();
    let mut score = 0u64;
    for line in 0..lines {
        let mut positions: Vec<usize> = (0..length).map(|p| {
            if horizontal { line * board.width + p } else { p * board.width + line }
        }).collect();
        if matches!(direction, Direction::Right | Direction::Down) { positions.reverse(); }
        let tiles: Vec<(usize,u32)> = positions.iter().copied().filter(|&p| board.cells[p]!=0).map(|p| (p,board.cells[p])).collect();
        for &p in &positions { result.cells[p] = 0; }
        let (mut i, mut out) = (0,0);
        while i < tiles.len() {
            let (first, value) = tiles[i];
            if i+1 < tiles.len() && value == tiles[i+1].1 {
                let second = tiles[i+1].0;
                let value = value.checked_mul(2).ok_or(Error::MergeOverflow { first, second })?;
                result.cells[positions[out]] = value;
                score += u64::from(value);
                merges.push(Merge { from: [first,second], to: positions[out], value });
                i += 2;
            } else {
                result.cells[positions[out]] = value;
                i += 1;
            }
            out += 1;
        }
    }
    Ok(MoveResult { changed: result != *board, board: result, score, merges })
}
pub fn legal_moves(board: &Board) -> Result<Vec<Direction>, Error> {
    validate(board)?;
    let mut result = Vec::new();
    for d in [Direction::Up,Direction::Left,Direction::Down,Direction::Right] {
        match move_board(board, d) {
            Ok(m) if m.changed => result.push(d),
            Ok(_) | Err(Error::MergeOverflow { .. }) => {},
            Err(e) => return Err(e),
        }
    }
    Ok(result)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawn { pub index: usize, pub value: u32 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnResult { pub board: Board, pub rng: u64, pub spawn: Option<Spawn> }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn { pub board: Board, pub rng: u64, pub score: u64, pub changed: bool, pub merges: Vec<Merge>, pub spawn: Option<Spawn> }

fn draw(rng: u64) -> u64 {
    rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}
pub fn spawn_tile(board: &Board, rng: u64) -> Result<SpawnResult, Error> {
    validate(board)?;
    let empty: Vec<usize> = board.cells.iter().enumerate().filter_map(|(i,&v)| if v==0 {Some(i)} else {None}).collect();
    if empty.is_empty() { return Ok(SpawnResult { board: board.clone(), rng: draw(draw(rng)), spawn: None }); }
    let first = draw(rng);
    let rng = draw(first);
    let index = empty[(first % empty.len() as u64) as usize];
    let value = if rng % 10 == 0 { 4 } else { 2 };
    let mut board = board.clone();
    board.cells[index] = value;
    Ok(SpawnResult { board, rng, spawn: Some(Spawn { index,value }) })
}
pub fn play_turn(board: &Board, rng: u64, direction: Direction) -> Result<Turn, Error> {
    let moved = move_board(board,direction)?;
    if !moved.changed {
        return Ok(Turn { board: moved.board, rng, score: moved.score, changed: false, merges: moved.merges, spawn: None });
    }
    let spawned = spawn_tile(&moved.board,rng)?;
    Ok(Turn { board: spawned.board, rng: spawned.rng, score: moved.score, changed: true, merges: moved.merges, spawn: spawned.spawn })
}
