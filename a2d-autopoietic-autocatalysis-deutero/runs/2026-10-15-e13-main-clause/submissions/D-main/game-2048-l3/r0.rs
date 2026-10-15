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
        if value != 0 && (value < 2 || value > 2_147_483_648 || !value.is_power_of_two()) {
            return Err(Error::InvalidTile { index, value });
        }
    }

    Ok(())
}

fn line_indices(board: &Board, direction: Direction, line: usize) -> Vec<usize> {
    match direction {
        Direction::Left => (0..board.width).map(|col| line * board.width + col).collect(),
        Direction::Right => (0..board.width)
            .rev()
            .map(|col| line * board.width + col)
            .collect(),
        Direction::Up => (0..board.height)
            .map(|row| row * board.width + line)
            .collect(),
        Direction::Down => (0..board.height)
            .rev()
            .map(|row| row * board.width + line)
            .collect(),
    }
}

pub fn move_board(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    validate(board)?;

    let mut cells = vec![0; board.cells.len()];
    let mut score = 0u64;
    let mut merges = Vec::new();

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

        let mut outputs: Vec<(u32, Option<[usize; 2]>)> = Vec::new();
        let mut pos = 0;

        while pos < tiles.len() {
            if pos + 1 < tiles.len() && tiles[pos].1 == tiles[pos + 1].1 {
                let first = tiles[pos].0;
                let second = tiles[pos + 1].0;
                let value = tiles[pos].1;
                if value == 2_147_483_648 {
                    return Err(Error::MergeOverflow { first, second });
                }
                let merged_value = value * 2;
                outputs.push((merged_value, Some([first, second])));
                score += merged_value as u64;
                pos += 2;
            } else {
                outputs.push((tiles[pos].1, None));
                pos += 1;
            }
        }

        for (offset, (value, source)) in outputs.into_iter().enumerate() {
            let to = indices[offset];
            cells[to] = value;
            if let Some(from) = source {
                merges.push(Merge { from, to, value });
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

pub fn legal_moves(board: &Board) -> Result<Vec<Direction>, Error> {
    validate(board)?;

    let mut legal = Vec::new();
    for direction in [
        Direction::Up,
        Direction::Left,
        Direction::Down,
        Direction::Right,
    ] {
        if matches!(move_board(board, direction), Ok(result) if result.changed) {
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

    let mut cells = board.cells.clone();
    cells[index] = value;

    Ok(SpawnResult {
        board: Board {
            width: board.width,
            height: board.height,
            cells,
        },
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