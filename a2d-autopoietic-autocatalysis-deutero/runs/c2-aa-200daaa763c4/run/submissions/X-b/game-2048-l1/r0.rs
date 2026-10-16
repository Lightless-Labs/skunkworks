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

    let scan: Vec<(usize, u32)> = match side {
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

    let mut cells = vec![0; row.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;
    let mut output_count = 0usize;
    let mut i = 0usize;

    while i < scan.len() {
        let (index, value) = scan[i];
        let (output_value, consumed) =
            if i + 1 < scan.len() && scan[i + 1].1 == value {
                let (second_index, _) = scan[i + 1];
                let Some(merged_value) = value.checked_mul(2) else {
                    return Err(Error::MergeOverflow {
                        first: index,
                        second: second_index,
                    });
                };

                let to = match side {
                    Side::Left => output_count,
                    Side::Right => row.len() - 1 - output_count,
                };
                merges.push(Merge {
                    from: [index, second_index],
                    to,
                    value: merged_value,
                });
                score += u64::from(merged_value);
                (merged_value, 2)
            } else {
                (value, 1)
            };

        let to = match side {
            Side::Left => output_count,
            Side::Right => row.len() - 1 - output_count,
        };
        cells[to] = output_value;
        output_count += 1;
        i += consumed;
    }

    let changed = cells.as_slice() != row;

    Ok(RowResult {
        cells,
        score,
        changed,
        merges,
    })
}