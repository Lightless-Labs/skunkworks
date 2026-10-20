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

    let expected_len = board.width * board.height;
    if board.cells.len() != expected_len {
        return Err(Error::Shape);
    }

    for (index, &value) in board.cells.iter().enumerate() {
        if value != 0
            && (value < 2 || value > 2_147_483_648 || !value.is_power_of_two())
        {
            return Err(Error::InvalidTile { index, value });
        }
    }

    Ok(())
}

fn move_board_validated(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    let width = board.width;
    let height = board.height;
    let mut cells = vec![0; board.cells.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;

    let line_count = match direction {
        Direction::Left | Direction::Right => height,
        Direction::Up | Direction::Down => width,
    };
    let line_len = match direction {
        Direction::Left | Direction::Right => width,
        Direction::Up | Direction::Down => height,
    };

    for line in 0..line_count {
        let indices: Vec<usize> = (0..line_len)
            .map(|offset| match direction {
                Direction::Left => line * width + offset,
                Direction::Right => line * width + (width - 1 - offset),
                Direction::Up => offset * width + line,
                Direction::Down => (height - 1 - offset) * width + line,
            })
            .collect();

        let tiles: Vec<(u32, usize)> = indices
            .iter()
            .filter_map(|&index| {
                let value = board.cells[index];
                (value != 0).then_some((value, index))
            })
            .collect();

        let mut output = Vec::new();
        let mut i = 0;
        while i < tiles.len() {
            if i + 1 < tiles.len() && tiles[i].0 == tiles[i + 1].0 {
                let value = tiles[i].0;
                if value == 2_147_483_648 {
                    return Err(Error::MergeOverflow {
                        first: tiles[i].1,
                        second: tiles[i + 1].1,
                    });
                }
                let merged_value = value * 2;
                let destination = indices[output.len()];
                output.push(merged_value);
                score += u64::from(merged_value);
                merges.push(Merge {
                    from: [tiles[i].1, tiles[i + 1].1],
                    to: destination,
                    value: merged_value,
                });
                i += 2;
            } else {
                output.push(tiles[i].0);
                i += 1;
            }
        }

        for (offset, value) in output.into_iter().enumerate() {
            cells[indices[offset]] = value;
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
    move_board_validated(board, direction)
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
        if let Ok(result) = move_board_validated(board, direction) {
            if result.changed {
                legal.push(direction);
            }
        }
    }

    Ok(legal)
}