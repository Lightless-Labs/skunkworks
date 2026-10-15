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
        if value != 0 && !(2..=2_147_483_648).contains(&value) || (value != 0 && !value.is_power_of_two()) {
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

    let mut cells = vec![0; row.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;
    let mut output_position = 0usize;
    let mut input_position = 0usize;

    while input_position < tiles.len() {
        let first = tiles[input_position];
        let value = row[first];

        if input_position + 1 < tiles.len() {
            let second = tiles[input_position + 1];
            if row[second] == value {
                let Some(merged_value) = value.checked_mul(2) else {
                    return Err(Error::MergeOverflow { first, second });
                };

                let to = match side {
                    Side::Left => output_position,
                    Side::Right => row.len() - 1 - output_position,
                };
                cells[to] = merged_value;
                merges.push(Merge {
                    from: [first, second],
                    to,
                    value: merged_value,
                });
                score += u64::from(merged_value);
                output_position += 1;
                input_position += 2;
                continue;
            }
        }

        let to = match side {
            Side::Left => output_position,
            Side::Right => row.len() - 1 - output_position,
        };
        cells[to] = value;
        output_position += 1;
        input_position += 1;
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