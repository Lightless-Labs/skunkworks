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

    let indices: Box<dyn Iterator<Item = usize>> = match side {
        Side::Left => Box::new(0..row.len()),
        Side::Right => Box::new((0..row.len()).rev()),
    };
    let tiles: Vec<(usize, u32)> = indices
        .filter_map(|index| (row[index] != 0).then_some((index, row[index])))
        .collect();

    let mut cells = vec![0; row.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;
    let mut output_position = 0usize;
    let mut i = 0usize;

    while i < tiles.len() {
        let (index, value) = tiles[i];
        if i + 1 < tiles.len() && tiles[i + 1].1 == value {
            let second_index = tiles[i + 1].0;
            let Some(merged_value) = value.checked_add(value) else {
                return Err(Error::MergeOverflow {
                    first: index,
                    second: second_index,
                });
            };

            let to = match side {
                Side::Left => output_position,
                Side::Right => row.len() - 1 - output_position,
            };
            cells[to] = merged_value;
            merges.push(Merge {
                from: [index, second_index],
                to,
                value: merged_value,
            });
            score += u64::from(merged_value);
            output_position += 1;
            i += 2;
        } else {
            let to = match side {
                Side::Left => output_position,
                Side::Right => row.len() - 1 - output_position,
            };
            cells[to] = value;
            output_position += 1;
            i += 1;
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