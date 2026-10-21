#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    RowTooLong { len: usize },
    InvalidTile { index: usize, value: u32 },
    MergeOverflow { first: usize, second: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merge {
    pub from: [usize; 2],
    pub to: usize,
    pub value: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowResult {
    pub cells: Vec<u32>,
    pub score: u64,
    pub changed: bool,
    pub merges: Vec<Merge>,
}

pub fn slide_row(row: &[u32], side: Side) -> Result<RowResult, Error> {
    if row.len() > 32 {
        return Err(Error::RowTooLong { len: row.len() });
    }

    for (index, &value) in row.iter().enumerate() {
        let valid = value == 0
            || (value >= 2 && value <= 2_147_483_648 && (value & (value - 1)) == 0);
        if !valid {
            return Err(Error::InvalidTile { index, value });
        }
    }

    let mut cells = vec![0u32; row.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;
    let mut scan = Vec::new();

    match side {
        Side::Left => {
            for index in 0..row.len() {
                if row[index] != 0 {
                    scan.push((index, row[index]));
                }
            }
        }
        Side::Right => {
            for index in (0..row.len()).rev() {
                if row[index] != 0 {
                    scan.push((index, row[index]));
                }
            }
        }
    }

    let mut input = 0usize;
    let mut output = 0usize;
    while input < scan.len() {
        let (index, value) = scan[input];
        if input + 1 < scan.len() && scan[input + 1].1 == value {
            let other_index = scan[input + 1].0;
            let doubled = value.checked_mul(2).ok_or(Error::MergeOverflow {
                first: index,
                second: other_index,
            })?;
            let destination = match side {
                Side::Left => output,
                Side::Right => row.len() - 1 - output,
            };
            cells[destination] = doubled;
            merges.push(Merge {
                from: [index, other_index],
                to: destination,
                value: doubled,
            });
            score += u64::from(doubled);
            input += 2;
        } else {
            let destination = match side {
                Side::Left => output,
                Side::Right => row.len() - 1 - output,
            };
            cells[destination] = value;
            input += 1;
        }
        output += 1;
    }

    Ok(RowResult {
        changed: cells.as_slice() != row,
        cells,
        score,
        merges,
    })
}