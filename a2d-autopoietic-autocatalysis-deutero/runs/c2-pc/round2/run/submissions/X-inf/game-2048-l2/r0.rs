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
        if value != 0
            && (value < 2 || value > 2_147_483_648 || !value.is_power_of_two())
        {
            return Err(Error::InvalidTile { index, value });
        }
    }
    Ok(())
}

fn move_validated(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
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
        let index_at = |position: usize| -> usize {
            match direction {
                Direction::Left => line * board.width + position,
                Direction::Right => line * board.width + (board.width - 1 - position),
                Direction::Up => position * board.width + line,
                Direction::Down => (board.height - 1 - position) * board.width + line,
            }
        };

        let occupied: Vec<(usize, u32)> = (0..line_len)
            .filter_map(|position| {
                let index = index_at(position);
                let value = board.cells[index];
                (value != 0).then_some((index, value))
            })
            .collect();

        let mut outputs: Vec<(u32, Option<[usize; 2]>)> = Vec::new();
        let mut cursor = 0;
        while cursor < occupied.len() {
            if cursor + 1 < occupied.len() && occupied[cursor].1 == occupied[cursor + 1].1 {
                let first = occupied[cursor];
                let second = occupied[cursor + 1];
                let Some(value) = first.1.checked_mul(2) else {
                    return Err(Error::MergeOverflow {
                        first: first.0,
                        second: second.0,
                    });
                };
                outputs.push((value, Some([first.0, second.0])));
                cursor += 2;
            } else {
                outputs.push((occupied[cursor].1, None));
                cursor += 1;
            }
        }

        for (position, (value, source)) in outputs.into_iter().enumerate() {
            let to = index_at(position);
            cells[to] = value;
            if let Some(from) = source {
                score += u64::from(value);
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
    move_validated(board, direction)
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
        match move_validated(board, direction) {
            Ok(moved) if moved.changed => result.push(direction),
            Ok(_) | Err(Error::MergeOverflow { .. }) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(result)
}