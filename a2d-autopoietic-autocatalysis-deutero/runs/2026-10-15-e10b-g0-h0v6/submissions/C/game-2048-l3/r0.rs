#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Left,
    Down,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Shape,
    InvalidTile { index: usize, value: u32 },
    MergeOverflow { first: usize, second: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merge {
    pub from: [usize; 2],
    pub to: usize,
    pub value: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveResult {
    pub board: Board,
    pub score: u64,
    pub changed: bool,
    pub merges: Vec<Merge>,
}

pub fn move_board(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    validate(board)?;

    let mut cells = vec![0; board.cells.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;

    match direction {
        Direction::Left | Direction::Right => {
            for row in 0..board.height {
                let indices: Vec<usize> = (0..board.width)
                    .map(|step| {
                        let col = if direction == Direction::Left {
                            step
                        } else {
                            board.width - 1 - step
                        };
                        row * board.width + col
                    })
                    .collect();
                process_line(board, &indices, &mut cells, &mut merges, &mut score)?;
            }
        }
        Direction::Up | Direction::Down => {
            for col in 0..board.width {
                let indices: Vec<usize> = (0..board.height)
                    .map(|step| {
                        let row = if direction == Direction::Up {
                            step
                        } else {
                            board.height - 1 - step
                        };
                        row * board.width + col
                    })
                    .collect();
                process_line(board, &indices, &mut cells, &mut merges, &mut score)?;
            }
        }
    }

    let changed = cells != board.cells;
    Ok(MoveResult {
        board: Board {
            width: board.width,
            height: board.height,
            cells,
        },
        score,
        changed,
        merges,
    })
}

fn process_line(
    board: &Board,
    indices: &[usize],
    output: &mut [u32],
    merges: &mut Vec<Merge>,
    score: &mut u64,
) -> Result<(), Error> {
    let tiles: Vec<(usize, u32)> = indices
        .iter()
        .filter_map(|&index| {
            let value = board.cells[index];
            (value != 0).then_some((index, value))
        })
        .collect();

    let mut produced = Vec::new();
    let mut i = 0;
    while i < tiles.len() {
        if i + 1 < tiles.len() && tiles[i].1 == tiles[i + 1].1 {
            let (first, value) = tiles[i];
            let (second, _) = tiles[i + 1];
            if value == 2_147_483_648 {
                return Err(Error::MergeOverflow { first, second });
            }
            let merged_value = value * 2;
            produced.push(merged_value);
            merges.push(Merge {
                from: [first, second],
                to: indices[produced.len() - 1],
                value: merged_value,
            });
            *score += u64::from(merged_value);
            i += 2;
        } else {
            produced.push(tiles[i].1);
            i += 1;
        }
    }

    for (slot, value) in indices.iter().zip(produced) {
        output[*slot] = value;
    }
    Ok(())
}

pub fn legal_moves(board: &Board) -> Result<Vec<Direction>, Error> {
    validate(board)?;
    let mut legal = Vec::new();
    for direction in [
        Direction::Up,
        Direction::Left,
        Direction::Down,
        Direction::Right,
    ] {
        if let Ok(result) = move_board(board, direction) {
            if result.changed {
                legal.push(direction);
            }
        }
    }
    Ok(legal)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawn {
    pub index: usize,
    pub value: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnResult {
    pub board: Board,
    pub rng: u64,
    pub spawn: Option<Spawn>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub board: Board,
    pub rng: u64,
    pub score: u64,
    pub changed: bool,
    pub merges: Vec<Merge>,
    pub spawn: Option<Spawn>,
}

pub fn spawn_tile(board: &Board, rng: u64) -> Result<SpawnResult, Error> {
    validate(board)?;

    let empty: Vec<usize> = board
        .cells
        .iter()
        .enumerate()
        .filter_map(|(index, &value)| (value == 0).then_some(index))
        .collect();

    if empty.is_empty() {
        return Ok(SpawnResult {
            board: board.clone(),
            rng,
            spawn: None,
        });
    }

    let a = next_rng(rng);
    let index = empty[(a % empty.len() as u64) as usize];
    let b = next_rng(a);
    let value = if b % 10 == 0 { 4 } else { 2 };

    let mut result_board = board.clone();
    result_board.cells[index] = value;

    Ok(SpawnResult {
        board: result_board,
        rng: b,
        spawn: Some(Spawn { index, value }),
    })
}

pub fn play_turn(board: &Board, rng: u64, direction: Direction) -> Result<Turn, Error> {
    let moved = move_board(board, direction)?;
    if !moved.changed {
        return Ok(Turn {
            board: moved.board,
            rng,
            score: 0,
            changed: false,
            merges: Vec::new(),
            spawn: None,
        });
    }

    let spawned = spawn_tile(&moved.board, rng)?;
    Ok(Turn {
        board: spawned.board,
        rng: spawned.rng,
        score: moved.score,
        changed: moved.changed,
        merges: moved.merges,
        spawn: spawned.spawn,
    })
}

fn validate(board: &Board) -> Result<(), Error> {
    if !(1..=8).contains(&board.width) || !(1..=8).contains(&board.height) {
        return Err(Error::Shape);
    }
    let expected = board.width * board.height;
    if board.cells.len() != expected {
        return Err(Error::Shape);
    }
    for (index, &value) in board.cells.iter().enumerate() {
        if value != 0 && (value < 2 || value > 2_147_483_648 || !value.is_power_of_two()) {
            return Err(Error::InvalidTile { index, value });
        }
    }
    Ok(())
}

fn next_rng(state: u64) -> u64 {
    state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}

fn main() {}