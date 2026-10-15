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
        if value != 0 && !(2..=2_147_483_648).contains(&value).then_some(value).unwrap_or(0).is_power_of_two() {
            return Err(Error::InvalidTile { index, value });
        }
    }

    Ok(())
}

fn flat_index(board: &Board, row: usize, col: usize) -> usize {
    row * board.width + col
}

fn line_indices(board: &Board, direction: Direction, line: usize) -> Vec<usize> {
    match direction {
        Direction::Left => (0..board.width)
            .map(|col| flat_index(board, line, col))
            .collect(),
        Direction::Right => (0..board.width)
            .rev()
            .map(|col| flat_index(board, line, col))
            .collect(),
        Direction::Up => (0..board.height)
            .map(|row| flat_index(board, row, line))
            .collect(),
        Direction::Down => (0..board.height)
            .rev()
            .map(|row| flat_index(board, row, line))
            .collect(),
    }
}

fn move_validated(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    let mut cells = vec![0; board.cells.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;

    let line_count = match direction {
        Direction::Left | Direction::Right => board.height,
        Direction::Up | Direction::Down => board.width,
    };

    for line in 0..line_count {
        let indices = line_indices(board, direction, line);
        let tiles: Vec<(usize, u32)> = indices
            .iter()
            .filter_map(|&index| {
                let value = board.cells[index];
                (value != 0).then_some((index, value))
            })
            .collect();

        let mut outputs: Vec<(u32, Option<Merge>)> = Vec::new();
        let mut i = 0;

        while i < tiles.len() {
            let (first_index, value) = tiles[i];
            if i + 1 < tiles.len() && tiles[i + 1].1 == value {
                let second_index = tiles[i + 1].0;
                if value == 2_147_483_648 {
                    return Err(Error::MergeOverflow {
                        first: first_index,
                        second: second_index,
                    });
                }

                let merged_value = value * 2;
                let to = indices[outputs.len()];
                let merge = Merge {
                    from: [first_index, second_index],
                    to,
                    value: merged_value,
                };
                score += merged_value as u64;
                outputs.push((merged_value, Some(merge)));
                i += 2;
            } else {
                outputs.push((value, None));
                i += 1;
            }
        }

        for (position, (value, merge)) in outputs.into_iter().enumerate() {
            let to = indices[position];
            cells[to] = value;
            if let Some(mut merge) = merge {
                merge.to = to;
                merges.push(merge);
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

pub fn move_board(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    validate(board)?;
    move_validated(board, direction)
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
        if matches!(move_validated(board, direction), Ok(result) if result.changed) {
            legal.push(direction);
        }
    }

    Ok(legal)
}

fn next_rng(rng: u64) -> u64 {
    rng.wrapping_mul(6_364_136_223_846_793_005)
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