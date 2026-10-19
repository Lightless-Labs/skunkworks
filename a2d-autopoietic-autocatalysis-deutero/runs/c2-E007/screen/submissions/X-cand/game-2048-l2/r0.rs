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

fn move_validated(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    let width = board.width;
    let height = board.height;
    let mut cells = vec![0; board.cells.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;

    let line_count = match direction {
        Direction::Left | Direction::Right => height,
        Direction::Up | Direction::Down => width,
    };
    let line_length = match direction {
        Direction::Left | Direction::Right => width,
        Direction::Up | Direction::Down => height,
    };

    for line in 0..line_count {
        let mut inputs: Vec<(usize, u32)> = Vec::new();

        for offset in 0..line_length {
            let (row, col) = match direction {
                Direction::Left => (line, offset),
                Direction::Right => (line, width - 1 - offset),
                Direction::Up => (offset, line),
                Direction::Down => (height - 1 - offset, line),
            };
            let index = row * width + col;
            let value = board.cells[index];
            if value != 0 {
                inputs.push((index, value));
            }
        }

        let mut outputs: Vec<(u32, Option<[usize; 2]>)> = Vec::new();
        let mut i = 0;
        while i < inputs.len() {
            if i + 1 < inputs.len() && inputs[i].1 == inputs[i + 1].1 {
                let value = inputs[i].1;
                if value == 2_147_483_648 {
                    return Err(Error::MergeOverflow {
                        first: inputs[i].0,
                        second: inputs[i + 1].0,
                    });
                }
                let merged_value = value * 2;
                outputs.push((merged_value, Some([inputs[i].0, inputs[i + 1].0])));
                score += u64::from(merged_value);
                i += 2;
            } else {
                outputs.push((inputs[i].1, None));
                i += 1;
            }
        }

        for (slot, (value, from)) in outputs.into_iter().enumerate() {
            let (row, col) = match direction {
                Direction::Left => (line, slot),
                Direction::Right => (line, width - 1 - slot),
                Direction::Up => (slot, line),
                Direction::Down => (height - 1 - slot, line),
            };
            let to = row * width + col;
            cells[to] = value;
            if let Some(from) = from {
                merges.push(Merge { from, to, value });
            }
        }
    }

    let changed = cells != board.cells;
    Ok(MoveResult {
        board: Board {
            width,
            height,
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
        if let Ok(result) = move_validated(board, direction) {
            if result.changed {
                legal.push(direction);
            }
        }
    }

    Ok(legal)
}