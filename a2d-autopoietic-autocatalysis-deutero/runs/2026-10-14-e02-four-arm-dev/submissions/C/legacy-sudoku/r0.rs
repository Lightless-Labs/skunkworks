/// Parses the first 81 digits of a Sudoku string into a 9×9 grid.
/// Non-digit characters and missing cells are treated as empty.
pub fn parse(s: &str) -> [[u8; 9]; 9] {
    let mut grid = [[0; 9]; 9];
    for (i, ch) in s.chars().take(81).enumerate() {
        grid[i / 9][i % 9] = ch.to_digit(10).filter(|&d| d <= 9).unwrap_or(0) as u8;
    }
    grid
}

/// Checks that a completed Sudoku grid contains valid rows, columns, and boxes.
pub fn validate(grid: &[[u8; 9]; 9]) -> bool {
    for i in 0..9 {
        let mut row = 0u16;
        let mut col = 0u16;
        for j in 0..9 {
            let r = grid[i][j];
            let c = grid[j][i];
            if !(1..=9).contains(&r) || !(1..=9).contains(&c) {
                return false;
            }
            let rb = 1u16 << r;
            let cb = 1u16 << c;
            if row & rb != 0 || col & cb != 0 {
                return false;
            }
            row |= rb;
            col |= cb;
        }
    }

    for box_row in (0..9).step_by(3) {
        for box_col in (0..9).step_by(3) {
            let mut seen = 0u16;
            for r in box_row..box_row + 3 {
                for c in box_col..box_col + 3 {
                    let bit = 1u16 << grid[r][c];
                    if seen & bit != 0 {
                        return false;
                    }
                    seen |= bit;
                }
            }
        }
    }
    true
}

fn candidates(grid: &[[u8; 9]; 9], row: usize, col: usize) -> u16 {
    let mut used = 0u16;
    for i in 0..9 {
        if grid[row][i] != 0 {
            used |= 1 << grid[row][i];
        }
        if grid[i][col] != 0 {
            used |= 1 << grid[i][col];
        }
    }
    let box_row = row / 3 * 3;
    let box_col = col / 3 * 3;
    for r in box_row..box_row + 3 {
        for c in box_col..box_col + 3 {
            if grid[r][c] != 0 {
                used |= 1 << grid[r][c];
            }
        }
    }
    0b11_1111_1110 & !used
}

fn search(grid: &mut [[u8; 9]; 9]) -> bool {
    let mut best: Option<(usize, usize, u16)> = None;
    let mut best_count = 10;

    for r in 0..9 {
        for c in 0..9 {
            if grid[r][c] == 0 {
                let options = candidates(grid, r, c);
                let count = options.count_ones();
                if count == 0 {
                    return false;
                }
                if count < best_count {
                    best = Some((r, c, options));
                    best_count = count;
                }
            }
        }
    }

    let Some((row, col, mut options)) = best else {
        return true;
    };

    while options != 0 {
        let bit = options & options.wrapping_neg();
        options &= !bit;
        grid[row][col] = bit.trailing_zeros() as u8;
        if search(grid) {
            return true;
        }
    }
    grid[row][col] = 0;
    false
}

/// Solves a Sudoku puzzle by backtracking, returning `None` for invalid or unsolvable grids.
pub fn solve(mut grid: [[u8; 9]; 9]) -> Option<[[u8; 9]; 9]> {
    for r in 0..9 {
        for c in 0..9 {
            let value = grid[r][c];
            if value > 9 {
                return None;
            }
            if value != 0 {
                grid[r][c] = 0;
                if candidates(&grid, r, c) & (1 << value) == 0 {
                    return None;
                }
                grid[r][c] = value;
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
    fn parses_digits_in_row_major_order() {
        let grid = parse("530070000600195000098000060800060003400803001700020006060000280000419005000080079");
        assert_eq!(grid[0][0], 5);
        assert_eq!(grid[1][3], 1);
        assert_eq!(grid[8][8], 9);
    }

    #[test]
    fn invalid_and_incomplete_grids_are_not_valid_solutions() {
        let mut grid = parse("534678912672195348198342567859761423426853791713924856961537284287419635345286179");
        assert!(validate(&grid));
        grid[0][0] = grid[0][3];
        assert!(!validate(&grid));
        assert!(solve(grid).is_none());
    }

    #[test]
    fn solves_a_puzzle_end_to_end() {
        let puzzle = parse("570030000400915000012000040200040007600207009300080004040000820000691005000020031");
        let solved = solve(puzzle).expect("puzzle is solvable");
        assert!(validate(&solved));
        let digits: String = solved.iter().flatten()
            .map(|&d| char::from_digit(d as u32, 10).unwrap())
            .collect();
        assert_eq!(digits, "576432198438915762912768543251349687684257319397186254149573826823691475765824931");
    }
}