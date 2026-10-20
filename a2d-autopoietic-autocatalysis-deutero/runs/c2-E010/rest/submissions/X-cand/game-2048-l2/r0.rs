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
    let expected = match board.width.checked_mul(board.height) {
        Some(n) => n,
        None => return Err(Error::Shape),
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
    let mut cells = vec![0u32; board.cells.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;

    let horizontal = matches!(direction, Direction::Left | Direction::Right);
    let line_count = if horizontal { board.height } else { board.width };
    let line_len = if horizontal { board.width } else { board.height };
    let reverse = matches!(direction, Direction::Right | Direction::Down);

    for line in 0..line_count {
        let mut tiles: Vec<(usize, u32)> = Vec::new();
        for offset in 0..line_len {
            let position = if reverse { line_len - 1 - offset } else { offset };
            let index = if horizontal {
                line * board.width + position
            } else {
                position * board.width + line
            };
            let value = board.cells[index];
            if value != 0 {
                tiles.push((index, value));
            }
        }

        let mut outputs: Vec<(u32, Option<[usize; 2]>)> = Vec::new();
        let mut i = 0;
        while i < tiles.len() {
            if i + 1 < tiles.len() && tiles[i].1 == tiles[i + 1].1 {
                let value = tiles[i].1;
                if value == 2_147_483_648 {
                    return Err(Error::MergeOverflow {
                        first: tiles[i].0,
                        second: tiles[i + 1].0,
                    });
                }
                let doubled = value * 2;
                outputs.push((doubled, Some([tiles[i].0, tiles[i + 1].0])));
                score += doubled as u64;
                i += 2;
            } else {
                outputs.push((tiles[i].1, None));
                i += 1;
            }
        }

        for (slot, (value, source_pair)) in outputs.into_iter().enumerate() {
            let position = if reverse { line_len - 1 - slot } else { slot };
            let index = if horizontal {
                line * board.width + position
            } else {
                position * board.width + line
            };
            cells[index] = value;
            if let Some(from) = source_pair {
                merges.push(Merge { from, to: index, value });
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
