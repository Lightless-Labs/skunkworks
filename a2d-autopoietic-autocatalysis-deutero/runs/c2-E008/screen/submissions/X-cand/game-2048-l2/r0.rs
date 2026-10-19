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

fn line_indices(board: &Board, direction: Direction, line: usize) -> Vec<usize> {
    match direction {
        Direction::Left => (0..board.width)
            .map(|column| line * board.width + column)
            .collect(),
        Direction::Right => (0..board.width)
            .rev()
            .map(|column| line * board.width + column)
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

fn move_validated(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    let line_count = match direction {
        Direction::Left | Direction::Right => board.height,
        Direction::Up | Direction::Down => board.width,
    };

    let mut cells = vec![0; board.cells.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;

    for line in 0..line_count {
        let indices = line_indices(board, direction, line);
        let tiles: Vec<(usize, u32)> = indices
            .iter()
            .filter_map(|&index| {
                let value = board.cells[index];
                (value != 0).then_some((index, value))
            })
            .collect();

        let mut input_position = 0;
        let mut output_position = 0;

        while input_position < tiles.len() {
            let (first, value) = tiles[input_position];
            let destination = indices[output_position];

            if input_position + 1 < tiles.len() && tiles[input_position + 1].1 == value {
                let second = tiles[input_position + 1].0;
                if value == 2_147_483_648 {
                    return Err(Error::MergeOverflow { first, second });
                }

                let merged_value = value * 2;
                cells[destination] = merged_value;
                merges.push(Merge {
                    from: [first, second],
                    to: destination,
                    value: merged_value,
                });
                score += u64::from(merged_value);
                input_position += 2;
            } else {
                cells[destination] = value;
                input_position += 1;
            }

            output_position += 1;
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
        if let Ok(result) = move_validated(board, direction) {
            if result.changed {
                legal.push(direction);
            }
        }
    }

    Ok(legal)
}