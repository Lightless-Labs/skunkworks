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
        if value != 0
            && (value < 2 || value > 2_147_483_648 || !value.is_power_of_two())
        {
            return Err(Error::InvalidTile { index, value });
        }
    }

    let indices: Box<dyn Iterator<Item = usize>> = match side {
        Side::Left => Box::new(0..row.len()),
        Side::Right => Box::new((0..row.len()).rev()),
    };

    let tiles: Vec<(usize, u32)> = indices
        .filter_map(|index| (row[index] != 0).then_some((index, row[index])))
        .collect();

    let mut output = Vec::with_capacity(row.len());
    let mut merges = Vec::new();
    let mut score = 0u64;
    let mut cursor = 0;

    while cursor < tiles.len() {
        let (index, value) = tiles[cursor];

        if cursor + 1 < tiles.len() && tiles[cursor + 1].1 == value {
            let (second_index, _) = tiles[cursor + 1];
            let Some(merged_value) = value.checked_mul(2) else {
                return Err(Error::MergeOverflow {
                    first: index,
                    second: second_index,
                });
            };

            let to = match side {
                Side::Left => output.len(),
                Side::Right => row.len() - 1 - output.len(),
            };
            merges.push(Merge {
                from: [index, second_index],
                to,
                value: merged_value,
            });
            output.push(merged_value);
            score += u64::from(merged_value);
            cursor += 2;
        } else {
            output.push(value);
            cursor += 1;
        }
    }

    let mut cells = vec![0; row.len()];
    match side {
        Side::Left => cells[..output.len()].copy_from_slice(&output),
        Side::Right => {
            let start = row.len() - output.len();
            for (offset, value) in output.into_iter().enumerate() {
                cells[row.len() - 1 - offset] = value;
            }
            debug_assert!(start <= row.len());
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

fn main() {}