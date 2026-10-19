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

fn validate(board: &Board) -> Result<(), Error> {
    if !(1..=8).contains(&board.width) || !(1..=8).contains(&board.height) {
        return Err(Error::Shape);
    }
    let Some(expected) = board.width.checked_mul(board.height) else {
        return Err(Error::Shape);
    };
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

pub fn move_board(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    validate(board)?;

    let mut cells = vec![0; board.cells.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;

    let line_count = match direction {
        Direction::Left | Direction::Right => board.height,
        Direction::Up | Direction::Down => board.width,
    };
    let line_len = match direction {
        Direction::Left | Direction::Right => board.width,
        Direction::Up | Direction::Down => board.height,
    };

    for line in 0..line_count {
        let mut inputs: Vec<(usize, u32)> = Vec::new();
        for step in 0..line_len {
            let (row, col) = match direction {
                Direction::Left => (line, step),
                Direction::Right => (line, line_len - 1 - step),
                Direction::Up => (step, line),
                Direction::Down => (line_len - 1 - step, line),
            };
            let index = row * board.width + col;
            let value = board.cells[index];
            if value != 0 {
                inputs.push((index, value));
            }
        }

        let mut outputs: Vec<(u32, Option<([usize; 2], u32)>)> = Vec::new();
        let mut i = 0;
        while i < inputs.len() {
            if i + 1 < inputs.len() && inputs[i].1 == inputs[i + 1].1 {
                if inputs[i].1 == 2_147_483_648 {
                    return Err(Error::MergeOverflow {
                        first: inputs[i].0,
                        second: inputs[i + 1].0,
                    });
                }
                let value = inputs[i].1 * 2;
                outputs.push((value, Some(([inputs[i].0, inputs[i + 1].0], value))));
                score += u64::from(value);
                i += 2;
            } else {
                outputs.push((inputs[i].1, None));
                i += 1;
            }
        }

        for (position, (value, merge_info)) in outputs.into_iter().enumerate() {
            let (row, col) = match direction {
                Direction::Left => (line, position),
                Direction::Right => (line, line_len - 1 - position),
                Direction::Up => (position, line),
                Direction::Down => (line_len - 1 - position, line),
            };
            let to = row * board.width + col;
            cells[to] = value;
            if let Some((from, merged_value)) = merge_info {
                merges.push(Merge {
                    from,
                    to,
                    value: merged_value,
                });
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
        match move_board(board, direction) {
            Ok(result) if result.changed => legal.push(direction),
            Ok(_) | Err(Error::MergeOverflow { .. }) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(legal)
}