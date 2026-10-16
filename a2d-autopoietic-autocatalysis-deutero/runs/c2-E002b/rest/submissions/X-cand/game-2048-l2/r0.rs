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
        || board.width.checked_mul(board.height) != Some(board.cells.len())
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

fn line_indices(board: &Board, direction: Direction, line: usize) -> Vec<usize> {
    match direction {
        Direction::Left => {
            (0..board.width).map(|x| line * board.width + x).collect()
        }
        Direction::Right => (0..board.width)
            .rev()
            .map(|x| line * board.width + x)
            .collect(),
        Direction::Up => (0..board.height)
            .map(|y| y * board.width + line)
            .collect(),
        Direction::Down => (0..board.height)
            .rev()
            .map(|y| y * board.width + line)
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

        let mut outputs: Vec<(u32, Option<[usize; 2]>)> = Vec::new();
        let mut i = 0;

        while i < tiles.len() {
            if i + 1 < tiles.len() && tiles[i].1 == tiles[i + 1].1 {
                let first = tiles[i].0;
                let second = tiles[i + 1].0;
                if tiles[i].1 == 2_147_483_648 {
                    return Err(Error::MergeOverflow { first, second });
                }

                let value = tiles[i].1 * 2;
                outputs.push((value, Some([first, second])));
                score += u64::from(value);
                i += 2;
            } else {
                outputs.push((tiles[i].1, None));
                i += 1;
            }
        }

        for (position, (value, from)) in outputs.into_iter().enumerate() {
            let to = indices[position];
            cells[to] = value;
            if let Some(from) = from {
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
        if matches!(
            move_board(board, direction),
            Ok(MoveResult { changed: true, .. })
        ) {
            legal.push(direction);
        }
    }

    Ok(legal)
}