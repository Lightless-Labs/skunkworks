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
    let tiles: Vec<usize> = indices.into_iter().filter(|&i| row[i] != 0).collect();

    let mut outputs = Vec::with_capacity(row.len());
    let mut merges = Vec::new();
    let mut score = 0u64;
    let mut i = 0;

    while i < tiles.len() {
        let first = tiles[i];
        let value = row[first];

        if i + 1 < tiles.len() && row[tiles[i + 1]] == value {
            let second = tiles[i + 1];
            let Some(merged_value) = value.checked_mul(2) else {
                return Err(Error::MergeOverflow { first, second });
            };

            let to = outputs.len();
            outputs.push(merged_value);
            merges.push(Merge {
                from: [first, second],
                to,
                value: merged_value,
            });
            score += u64::from(merged_value);
            i += 2;
        } else {
            outputs.push(value);
            i += 1;
        }
    }

    let mut cells = vec![0; row.len()];
    match side {
        Side::Left => {
            cells[..outputs.len()].copy_from_slice(&outputs);
        }
        Side::Right => {
            let start = row.len() - outputs.len();
            cells[start..].copy_from_slice(&outputs);
            for merge in &mut merges {
                merge.to += start;
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