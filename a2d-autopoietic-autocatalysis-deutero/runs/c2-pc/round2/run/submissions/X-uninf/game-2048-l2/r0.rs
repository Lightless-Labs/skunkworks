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

pub fn move_board(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    validate(board)?;

    let mut cells = vec![0; board.cells.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;

    let line_count = match direction {
        Direction::Left | Direction::Right => board.height,
        Direction::Up | Direction::Down => board.width,
    };
    let line_length = match direction {
        Direction::Left | Direction::Right => board.width,
        Direction::Up | Direction::Down => board.height,
    };

    for line in 0..line_count {
        let indices: Vec<usize> = (0..line_length)
            .map(|position| match direction {
                Direction::Left => line * board.width + position,
                Direction::Right => line * board.width + (board.width - 1 - position),
                Direction::Up => position * board.width + line,
                Direction::Down => (board.height - 1 - position) * board.width + line,
            })
            .collect();

        let occupied: Vec<(usize, u32)> = indices
            .iter()
            .copied()
            .filter_map(|index| {
                let value = board.cells[index];
                (value != 0).then_some((index, value))
            })
            .collect();

        let mut output_position = 0usize;
        let mut input_position = 0usize;

        while input_position < occupied.len() {
            let (first_index, first_value) = occupied[input_position];

            if input_position + 1 < occupied.len()
                && occupied[input_position + 1].1 == first_value
            {
                let second_index = occupied[input_position + 1].0;
                if first_value == 2_147_483_648 {
                    return Err(Error::MergeOverflow {
                        first: first_index,
                        second: second_index,
                    });
                }

                let value = first_value * 2;
                let to = indices[output_position];
                cells[to] = value;
                merges.push(Merge {
                    from: [first_index, second_index],
                    to,
                    value,
                });
                score += u64::from(value);
                input_position += 2;
            } else {
                cells[indices[output_position]] = first_value;
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