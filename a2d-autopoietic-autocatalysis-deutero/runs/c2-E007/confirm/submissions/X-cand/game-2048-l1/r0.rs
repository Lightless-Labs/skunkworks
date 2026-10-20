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
        if value != 0 && (value < 2 || value > 2_147_483_648 || !value.is_power_of_two()) {
            return Err(Error::InvalidTile { index, value });
        }
    }

    let indices: Vec<usize> = match side {
        Side::Left => (0..row.len()).collect(),
        Side::Right => (0..row.len()).rev().collect(),
    };
    let tiles: Vec<usize> = indices
        .into_iter()
        .filter(|&index| row[index] != 0)
        .collect();

    let mut output = Vec::with_capacity(row.len());
    let mut merges = Vec::new();
    let mut score = 0u64;
    let mut i = 0;

    while i < tiles.len() {
        let first = tiles[i];
        if i + 1 < tiles.len() && row[first] == row[tiles[i + 1]] {
            let second = tiles[i + 1];
            let value = row[first]
                .checked_mul(2)
                .ok_or(Error::MergeOverflow { first, second })?;
            let to = match side {
                Side::Left => output.len(),
                Side::Right => row.len() - 1 - output.len(),
            };
            output.push(value);
            merges.push(Merge {
                from: [first, second],
                to,
                value,
            });
            score += u64::from(value);
            i += 2;
        } else {
            output.push(row[first]);
            i += 1;
        }
    }

    let mut cells = vec![0; row.len()];
    match side {
        Side::Left => cells[..output.len()].copy_from_slice(&output),
        Side::Right => {
            for (offset, &value) in output.iter().enumerate() {
                cells[row.len() - 1 - offset] = value;
            }
        }
    }

    let changed = cells != row;
    Ok(RowResult {
        cells,
        score,
        changed,
        merges,
    })
}