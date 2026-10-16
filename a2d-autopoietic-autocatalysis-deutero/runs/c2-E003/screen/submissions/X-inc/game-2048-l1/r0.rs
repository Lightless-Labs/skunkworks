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

    let mut scan: Vec<(usize, u32)> = match side {
        Side::Left => row
            .iter()
            .copied()
            .enumerate()
            .filter(|&(_, value)| value != 0)
            .collect(),
        Side::Right => row
            .iter()
            .copied()
            .enumerate()
            .rev()
            .filter(|&(_, value)| value != 0)
            .collect(),
    };

    let mut outputs = Vec::with_capacity(scan.len());
    let mut merges = Vec::new();
    let mut score = 0u64;
    let mut i = 0;

    while i < scan.len() {
        let (first_index, value) = scan[i];
        if i + 1 < scan.len() && scan[i + 1].1 == value {
            let second_index = scan[i + 1].0;
            let Some(merged_value) = value.checked_mul(2) else {
                return Err(Error::MergeOverflow {
                    first: first_index,
                    second: second_index,
                });
            };
            let to = match side {
                Side::Left => outputs.len(),
                Side::Right => row.len() - 1 - outputs.len(),
            };
            merges.push(Merge {
                from: [first_index, second_index],
                to,
                value: merged_value,
            });
            outputs.push(merged_value);
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
            for (offset, value) in outputs.into_iter().enumerate() {
                cells[row.len() - 1 - offset] = value;
            }
            let _ = start;
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