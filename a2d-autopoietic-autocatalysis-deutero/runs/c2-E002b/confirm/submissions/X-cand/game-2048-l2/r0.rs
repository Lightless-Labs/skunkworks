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

    let Some(expected_len) = board.width.checked_mul(board.height) else {
        return Err(Error::Shape);
    };
    if board.cells.len() != expected_len {
        return Err(Error::Shape);
    }

    for (index, &value) in board.cells.iter().enumerate() {
        if value != 0 && (value < 2 || value > 2_147_483_648 || !value.is_power_of_two()) {
            return Err(Error::InvalidTile { index, value });
        }
    }

    Ok(())
}

fn move_valid_board(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
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
        let indices: Vec<usize> = (0..line_len)
            .map(|offset| match direction {
                Direction::Left => line * board.width + offset,
                Direction::Right => line * board.width + (board.width - 1 - offset),
                Direction::Up => offset * board.width + line,
                Direction::Down => (board.height - 1 - offset) * board.width + line,
            })
            .collect();

        let inputs: Vec<(usize, u32)> = indices
            .iter()
            .filter_map(|&index| {
                let value = board.cells[index];
                (value != 0).then_some((index, value))
            })
            .collect();

        let mut outputs: Vec<(u32, Option<[usize; 2]>)> = Vec::new();
        let mut i = 0;
        while i < inputs.len() {
            let (first_index, first_value) = inputs[i];
            if i + 1 < inputs.len() && inputs[i + 1].1 == first_value {
                let second_index = inputs[i + 1].0;
                if first_value == 2_147_483_648 {
                    return Err(Error::MergeOverflow {
                        first: first_index,
                        second: second_index,
                    });
                }
                let value = first_value * 2;
                outputs.push((value, Some([first_index, second_index])));
                score += value as u64;
                i += 2;
            } else {
                outputs.push((first_value, None));
                i += 1;
            }
        }

        for (offset, (value, consumed)) in outputs.into_iter().enumerate() {
            let to = indices[offset];
            cells[to] = value;
            if let Some(from) = consumed {
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

pub fn move_board(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    validate(board)?;
    move_valid_board(board, direction)
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
        if let Ok(result) = move_valid_board(board, direction) {
            if result.changed {
                legal.push(direction);
            }
        }
    }
    Ok(legal)
}