#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side { Left, Right }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error { RowTooLong { len: usize }, InvalidTile { index: usize, value: u32 }, MergeOverflow { first: usize, second: usize } }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merge { pub from: [usize; 2], pub to: usize, pub value: u32 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowResult { pub cells: Vec<u32>, pub score: u64, pub changed: bool, pub merges: Vec<Merge> }

pub fn slide_row(row: &[u32], side: Side) -> Result<RowResult, Error> {
    if row.len() > 32 { return Err(Error::RowTooLong { len: row.len() }); }
    for (index, &value) in row.iter().enumerate().rev() {
        if value != 0 && (value < 2 || !value.is_power_of_two()) {
            return Err(Error::InvalidTile { index, value });
        }
    }
    let positions: Vec<usize> = match side {
        Side::Left => (0..row.len()).collect(),
        Side::Right => (0..row.len()).rev().collect(),
    };
    let tiles: Vec<(usize,u32)> = positions.iter().copied().filter(|&p| row[p]!=0).map(|p| (p,row[p])).collect();
    let mut cells = vec![0;row.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;
    let (mut i, mut out) = (0,0);
    while i < tiles.len() {
        let (first, value) = tiles[i];
        if i+1 < tiles.len() && value == tiles[i+1].1 {
            let second = tiles[i+1].0;
            let value = value.checked_mul(2).ok_or(Error::MergeOverflow { first, second })?;
            cells[positions[out]] = value;
            score += u64::from(value);
            merges.push(Merge { from: [first,second], to: positions[out], value });
            i += 2;
        } else {
            cells[positions[out]] = value;
            i += 1;
        }
        out += 1;
    }
    Ok(RowResult { changed: cells != row, cells, score, merges })
}
