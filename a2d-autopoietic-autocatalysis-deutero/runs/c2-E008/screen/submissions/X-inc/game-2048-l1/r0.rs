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
    let len = row.len();

    if len > 32 {
        return Err(Error::RowTooLong { len });
    }

    for (index, &value) in row.iter().enumerate() {
        if value != 0 && (value < 2 || value > 2_147_483_648 || !value.is_power_of_two()) {
            return Err(Error::InvalidTile { index, value });
        }
    }

    let indices: Vec<usize> = match side {
        Side::Left => (0..len).collect(),
        Side::Right => (0..len).rev().collect(),
    };
    let nonempty: Vec<usize> = indices.into_iter().filter(|&i| row[i] != 0).collect();

    let mut cells = vec![0; len];
    let mut merges = Vec::new();
    let mut score = 0u64;
    let mut output_position = 0usize;
    let mut input_position = 0usize;

    while input_position < nonempty.len() {
        let first = nonempty[input_position];

        if input_position + 1 < nonempty.len() {
            let second = nonempty[input_position + 1];
            if row[first] == row[second] {
                let value = row[first]
                    .checked_mul(2)
                    .ok_or(Error::MergeOverflow { first, second })?;
                let to = match side {
                    Side::Left => output_position,
                    Side::Right => len - 1 - output_position,
                };
                cells[to] = value;
                merges.push(Merge {
                    from: [first, second],
                    to,
                    value,
                });
                score += u64::from(value);
                output_position += 1;
                input_position += 2;
                continue;
            }
        }

        let to = match side {
            Side::Left => output_position,
            Side::Right => len - 1 - output_position,
        };
        cells[to] = row[first];
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