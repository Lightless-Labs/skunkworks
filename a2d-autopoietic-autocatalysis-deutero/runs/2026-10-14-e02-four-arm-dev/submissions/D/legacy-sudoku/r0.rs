pub fn parse(s: &str) -> [[u8; 9]; 9] {
    let mut grid = [[0u8; 9]; 9];
    for (index, ch) in s.chars().take(81).enumerate() {
        let value = ch.to_digit(10).unwrap_or(0) as u8;
        grid[index / 9][index % 9] = value;
    }
    grid
}

pub fn validate(grid: &[[u8; 9]; 9]) -> bool {
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

pub fn solve(mut grid: [[u8; 9]; 9]) -> Option<[[u8; 9]; 9]> {
    if !is_consistent(&grid) {
        return None;
    }
    if solve_in_place(&mut grid) {
        Some(grid)
    } else {
        None
    }
}

fn is_consistent(grid: &[[u8; 9]; 9]) -> bool {
    for row in 0..9 {
        for col in 0..9 {
            let value = grid[row][col];
            if value > 9 {
                return false;
            }
            if value != 0 {
                for other_col in 0..9 {
                    if other_col != col && grid[row][other_col] == value {
                        return false;
                    }
                }
                for other_row in 0..9 {
                    if other_row != row && grid[other_row][col] == value {
                        return false;
                    }
                }
                let box_row = (row / 3) * 3;
                let box_col = (col / 3) * 3;
                for other_row in box_row..box_row + 3 {
                    for other_col in box_col..box_col + 3 {
                        if (other_row != row || other_col != col)
                            && grid[other_row][other_col] == value
                        {
                            return false;
                        }
                    }
                }
            }
        }
    }
    true
}

fn solve_in_place(grid: &mut [[u8; 9]; 9]) -> bool {
    let mut best: Option<(usize, usize, Vec<u8>)> = None;

    for row in 0..9 {
        for col in 0..9 {
            if grid[row][col] == 0 {
                let candidates: Vec<u8> = (1..=9)
                    .filter(|&value| is_safe(grid, row, col, value))
                    .collect();

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
    }

    let Some((row, col, candidates)) = best else {
        return true;
    };

    for value in candidates {
        grid[row][col] = value;
        if solve_in_place(grid) {
            return true;
        }
    }
    grid[row][col] = 0;
    false
}

fn is_safe(grid: &[[u8; 9]; 9], row: usize, col: usize, value: u8) -> bool {
    for index in 0..9 {
        if grid[row][index] == value || grid[index][col] == value {
            return false;
        }
    }

    let box_row = (row / 3) * 3;
    let box_col = (col / 3) * 3;
    for r in box_row..box_row + 3 {
        for c in box_col..box_col + 3 {
            if grid[r][c] == value {
                return false;
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
    fn parses_digits() {
        let grid = parse("530070000600195000098000060800060003400803001700020006060000280000419005000080079");
        assert_eq!(grid[0][0], 5);
        assert_eq!(grid[8][8], 9);
    }

    #[test]
    fn validates_complete_grid() {
        let grid = parse("534678912672195348198342567859761423426853791713924856961537284287419635345286179");
        assert!(validate(&grid));
    }

    #[test]
    fn solves_puzzle() {
        let grid = parse("530070000600195000098000060800060003400803001700020006060000280000419005000080079");
        let solution = solve(grid).unwrap();
        assert!(validate(&solution));
    }
}