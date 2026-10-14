pub fn parse(s: &str) -> [[u8; 9]; 9] {
    let mut grid = [[0u8; 9]; 9];
    for (index, byte) in s.bytes().take(81).enumerate() {
        if byte.is_ascii_digit() {
            grid[index / 9][index % 9] = byte - b'0';
        }
    }
    grid
}

pub fn validate(grid: &[[u8; 9]; 9]) -> bool {
    for row in 0..9 {
        let mut seen = [false; 10];
        for col in 0..9 {
            let value = grid[row][col];
            if !(1..=9).contains(&value) || seen[value as usize] {
                return false;
            }
            seen[value as usize] = true;
        }
    }

    for col in 0..9 {
        let mut seen = [false; 10];
        for row in 0..9 {
            let value = grid[row][col];
            if seen[value as usize] {
                return false;
            }
            seen[value as usize] = true;
        }
    }

    for box_row in 0..3 {
        for box_col in 0..3 {
            let mut seen = [false; 10];
            for row in box_row * 3..box_row * 3 + 3 {
                for col in box_col * 3..box_col * 3 + 3 {
                    let value = grid[row][col];
                    if seen[value as usize] {
                        return false;
                    }
                    seen[value as usize] = true;
                }
            }
        }
    }

    true
}

pub fn solve(mut grid: [[u8; 9]; 9]) -> Option<[[u8; 9]; 9]> {
    fn search(grid: &mut [[u8; 9]; 9]) -> bool {
        let mut best: Option<(usize, usize, Vec<u8>)> = None;

        for row in 0..9 {
            for col in 0..9 {
                if grid[row][col] != 0 {
                    continue;
                }

                let mut candidates = Vec::new();
                for value in 1..=9 {
                    let row_ok = (0..9).all(|c| grid[row][c] != value);
                    let col_ok = (0..9).all(|r| grid[r][col] != value);
                    let box_row = (row / 3) * 3;
                    let box_col = (col / 3) * 3;
                    let box_ok = (box_row..box_row + 3)
                        .all(|r| (box_col..box_col + 3).all(|c| grid[r][c] != value));

                    if row_ok && col_ok && box_ok {
                        candidates.push(value);
                    }
                }

                if candidates.is_empty() {
                    return false;
                }

                if best
                    .as_ref()
                    .is_none_or(|(_, _, current)| candidates.len() < current.len())
                {
                    best = Some((row, col, candidates));
                }
            }
        }

        let Some((row, col, candidates)) = best else {
            return true;
        };

        for value in candidates {
            grid[row][col] = value;
            if search(grid) {
                return true;
            }
            grid[row][col] = 0;
        }

        false
    }

    if grid.iter().flatten().any(|&value| value > 9) {
        return None;
    }

    for row in 0..9 {
        for col in 0..9 {
            let value = grid[row][col];
            if value == 0 {
                continue;
            }

            grid[row][col] = 0;
            let valid_here = (0..9).all(|c| grid[row][c] != value)
                && (0..9).all(|r| grid[r][col] != value)
                && ((row / 3) * 3..(row / 3) * 3 + 3)
                    .all(|r| ((col / 3) * 3..(col / 3) * 3 + 3)
                        .all(|c| grid[r][c] != value));
            grid[row][col] = value;

            if !valid_here {
                return None;
            }
        }
    }

    if search(&mut grid) {
        Some(grid)
    } else {
        None
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reads_row_major() {
        let grid = parse("530070000600195000098000060800060003400803001700020006060000280000419005000080079");
        assert_eq!(grid[0][0], 5);
        assert_eq!(grid[0][1], 3);
        assert_eq!(grid[1][3], 1);
    }

    #[test]
    fn validates_completed_grid() {
        let grid = parse("534678912672195348198342567859761423426853791713924856961537284287419635345286179");
        assert!(validate(&grid));
    }

    #[test]
    fn solves_puzzle() {
        let grid = parse("530070000600195000098000060800060003400803001700020006060000280000419005000080079");
        let solved = solve(grid).unwrap();
        assert!(validate(&solved));
    }
}