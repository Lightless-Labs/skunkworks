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

fn validate(board: &Board) -> Result<(), Error> {
    if !(1..=8).contains(&board.width)
        || !(1..=8).contains(&board.height)
        || board.cells.len() != board.width * board.height
    {
        return Err(Error::Shape);
    }

    for (index, &value) in board.cells.iter().enumerate() {
        if value != 0 && !(value.is_power_of_two() && (2..=2_147_483_648).contains(&value)) {
            return Err(Error::InvalidTile { index, value });
        }
    }

    Ok(())
}

pub fn move_board(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    validate(board)?;

    let mut output = vec![0u32; board.cells.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;

    let horizontal = matches!(direction, Direction::Left | Direction::Right);
    let line_count = if horizontal { board.height } else { board.width };
    let line_length = if horizontal { board.width } else { board.height };
    let reverse = matches!(direction, Direction::Right | Direction::Down);

    for line in 0..line_count {
        let mut tiles: Vec<(usize, u32)> = Vec::new();

        for offset in 0..line_length {
            let position = if reverse {
                line_length - 1 - offset
            } else {
                offset
            };
            let index = if horizontal {
                line * board.width + position
            } else {
                position * board.width + line
            };
            let value = board.cells[index];
            if value != 0 {
                tiles.push((index, value));
            }
        }

        let mut cursor = 0;
        let mut output_offset = 0;

        while cursor < tiles.len() {
            let (first_index, first_value) = tiles[cursor];

            if cursor + 1 < tiles.len() && tiles[cursor + 1].1 == first_value {
                if first_value == 2_147_483_648 {
                    return Err(Error::MergeOverflow {
                        first: first_index,
                        second: tiles[cursor + 1].0,
                    });
                }

                let value = first_value * 2;
                let position = if reverse {
                    line_length - 1 - output_offset
                } else {
                    output_offset
                };
                let to = if horizontal {
                    line * board.width + position
                } else {
                    position * board.width + line
                };

                output[to] = value;
                merges.push(Merge {
                    from: [first_index, tiles[cursor + 1].0],
                    to,
                    value,
                });
                score += value as u64;
                cursor += 2;
            } else {
                let position = if reverse {
                    line_length - 1 - output_offset
                } else {
                    output_offset
                };
                let to = if horizontal {
                    line * board.width + position
                } else {
                    position * board.width + line
                };

                output[to] = first_value;
                cursor += 1;
            }

            output_offset += 1;
        }
    }

    let changed = output != board.cells;
    Ok(MoveResult {
        board: Board {
            width: board.width,
            height: board.height,
            cells: output,
        },
        score: if changed { score } else { 0 },
        changed,
        merges: if changed { merges } else { Vec::new() },
    })
}

pub fn legal_moves(board: &Board) -> Result<Vec<Direction>, Error> {
    validate(board)?;

    let mut result = Vec::new();
    for direction in [
        Direction::Up,
        Direction::Left,
        Direction::Down,
        Direction::Right,
    ] {
        if matches!(
            move_board(board, direction),
            Ok(MoveResult { changed: true, .. })
        ) {
            result.push(direction);
        }
    }
    Ok(result)
}

fn next_rng(state: u64) -> u64 {
    state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
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

    let mut spawned_board = board.clone();
    spawned_board.cells[index] = value;

    Ok(SpawnResult {
        board: spawned_board,
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

fn main() {}