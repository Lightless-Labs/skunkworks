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

    let expected = board.width * board.height;
    if board.cells.len() != expected {
        return Err(Error::Shape);
    }

    for (index, &value) in board.cells.iter().enumerate() {
        if value != 0 && (value > 2_147_483_648 || !value.is_power_of_two()) {
            return Err(Error::InvalidTile { index, value });
        }
    }

    Ok(())
}

fn line_indices(board: &Board, direction: Direction, line: usize) -> Vec<usize> {
    match direction {
        Direction::Left => (0..board.width)
            .map(|col| line * board.width + col)
            .collect(),
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

        let mut output_position = 0;
        let mut tile_position = 0;

        while tile_position < tiles.len() {
            let (first_index, first_value) = tiles[tile_position];

            if tile_position + 1 < tiles.len()
                && tiles[tile_position + 1].1 == first_value
            {
                let (second_index, _) = tiles[tile_position + 1];
                if first_value == 2_147_483_648 {
                    return Err(Error::MergeOverflow {
                        first: first_index,
                        second: second_index,
                    });
                }

                let value = first_value * 2;
                let destination = indices[output_position];
                cells[destination] = value;
                merges.push(Merge {
                    from: [first_index, second_index],
                    to: destination,
                    value,
                });
                score += u64::from(value);
                tile_position += 2;
            } else {
                cells[indices[output_position]] = first_value;
                tile_position += 1;
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