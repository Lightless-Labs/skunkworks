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
    let tiles: Vec<(usize, u32)> = indices
        .into_iter()
        .filter_map(|i| (row[i] != 0).then_some((i, row[i])))
        .collect();

    let mut cells = vec![0; row.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;
    let mut cursor = 0usize;
    let mut output_count = 0usize;

    while cursor < tiles.len() {
        let (first_index, first_value) = tiles[cursor];

        if cursor + 1 < tiles.len() && tiles[cursor + 1].1 == first_value {
            let second_index = tiles[cursor + 1].0;
            let Some(value) = first_value.checked_mul(2) else {
                return Err(Error::MergeOverflow {
                    first: first_index,
                    second: second_index,
                });
            };

            let to = match side {
                Side::Left => output_count,
                Side::Right => row.len() - 1 - output_count,
            };
            cells[to] = value;
            merges.push(Merge {
                from: [first_index, second_index],
                to,
                value,
            });
            score += u64::from(value);
            output_count += 1;
            cursor += 2;
        } else {
            let to = match side {
                Side::Left => output_count,
                Side::Right => row.len() - 1 - output_count,
            };
            cells[to] = first_value;
            output_count += 1;
            cursor += 1;
        }
    }

    let changed = cells.as_slice() != row;

    Ok(RowResult {
        cells,
        score,
        changed,
        merges,
    })
}
