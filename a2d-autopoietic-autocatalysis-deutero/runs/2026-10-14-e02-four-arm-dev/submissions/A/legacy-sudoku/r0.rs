fn parse(s: &str) -> [[u8; 9]; 9] {
    let mut grid = [[0u8; 9]; 9];
    for (i, ch) in s.chars().take(81).enumerate() {
        let value = ch.to_digit(10).unwrap_or(0) as u8;
        grid[i / 9][i % 9] = value;
    }
    grid
}

fn is_consistent(grid: &[[u8; 9]; 9]) -> bool {
    for row in 0..9 {
        for col in 0..9 {
            let value = grid[row][col];
            if value == 0 {
                continue;
            }
            if value > 9 {
                return false;
            }
            for other_col in 0..col {
                if grid[row][other_col] == value {
                    return false;
                }
            }
            for other_row in 0..row {
                if grid[other_row][col] == value {
                    return false;
                }
            }
            let box_row = (row / 3) * 3;
            let box_col = (col / 3) * 3;
            for r in box_row..=row {
                for c in box_col..9.min(box_col + 3) {
                    if (r < row || c < col) && grid[r][c] == value {
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn solve(grid: [[u8; 9]; 9]) -> Option<[[u8; 9]; 9]> {
    if !is_consistent(&grid) {
        return None;
    }

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
                    let box_ok = (box_row..box_row + 3).all(|r| {
                        (box_col..box_col + 3).all(|c| grid[r][c] != value)
                    });
                    if row_ok && col_ok && box_ok {
                        candidates.push(value);
                    }
                }

                if candidates.is_empty() {
                    return false;
                }
                if best
                    .as_ref()
                    .map_or(true, |(_, _, current)| candidates.len() < current.len())
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

    let mut solution = grid;
    if search(&mut solution) {
        Some(solution)
    } else {
        None
    }
}

fn validate(grid: &[[u8; 9]; 9]) -> bool {
    for row in 0..9 {
        let mut seen = [false; 10];
        for col in 0..9 {
            let value = grid[row][col] as usize;
            if !(1..=9).contains(&value) || seen[value] {
                return false;
            }
            seen[value] = true;
        }
    }

    for col in 0..9 {
        let mut seen = [false; 10];
        for row in 0..9 {
            let value = grid[row][col] as usize;
            if seen[value] {
                return false;
            }
            seen[value] = true;
        }
    }

    for box_row in (0..9).step_by(3) {
        for box_col in (0..9).step_by(3) {
            let mut seen = [false; 10];
            for row in box_row..box_row + 3 {
                for col in box_col..box_col + 3 {
                    let value = grid[row][col] as usize;
                    if seen[value] {
                        return false;
                    }
                    seen[value] = true;
                }
            }
        }
    }

    true
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_row_major() {
        let grid = parse("530070000600195000098000060800060003400803001700020006060000280000419005000080079");
        assert_eq!(grid[0][0], 5);
        assert_eq!(grid[0][1], 3);
        assert_eq!(grid[8][8], 9);
    }

    #[test]
    fn validates_solution() {
        let grid = parse("534678912672195348198342567859761423426853791713924856961537284287419635345286179");
        assert!(validate(&grid));
    }

    #[test]
    fn solves_puzzle() {
        let puzzle = parse("530070000600195000098000060800060003400803001700020006060000280000419005000080079");
        let solution = solve(puzzle).unwrap();
        assert!(validate(&solution));
    }
}