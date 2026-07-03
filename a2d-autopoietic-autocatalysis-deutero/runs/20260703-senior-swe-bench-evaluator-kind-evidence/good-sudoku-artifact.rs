fn parse(s: &str) -> [[u8; 9]; 9] {
    let mut grid = [[0u8; 9]; 9];
    for (idx, ch) in s.chars().take(81).enumerate() {
        grid[idx / 9][idx % 9] = ch.to_digit(10).unwrap_or(0) as u8;
    }
    grid
}

fn validate(grid: &[[u8; 9]; 9]) -> bool {
    let mut rows = [[false; 10]; 9];
    let mut cols = [[false; 10]; 9];
    let mut boxes = [[false; 10]; 9];
    for r in 0..9 {
        for c in 0..9 {
            let value = grid[r][c] as usize;
            if value == 0 || value > 9 {
                return false;
            }
            let b = (r / 3) * 3 + (c / 3);
            if rows[r][value] || cols[c][value] || boxes[b][value] {
                return false;
            }
            rows[r][value] = true;
            cols[c][value] = true;
            boxes[b][value] = true;
        }
    }
    true
}

fn is_valid_partial(grid: &[[u8; 9]; 9]) -> bool {
    let mut rows = [[false; 10]; 9];
    let mut cols = [[false; 10]; 9];
    let mut boxes = [[false; 10]; 9];
    for r in 0..9 {
        for c in 0..9 {
            let value = grid[r][c] as usize;
            if value == 0 {
                continue;
            }
            if value > 9 {
                return false;
            }
            let b = (r / 3) * 3 + (c / 3);
            if rows[r][value] || cols[c][value] || boxes[b][value] {
                return false;
            }
            rows[r][value] = true;
            cols[c][value] = true;
            boxes[b][value] = true;
        }
    }
    true
}

fn solve(mut grid: [[u8; 9]; 9]) -> Option<[[u8; 9]; 9]> {
    if !is_valid_partial(&grid) {
        return None;
    }
    if solve_cell(&mut grid) {
        Some(grid)
    } else {
        None
    }
}

fn solve_cell(grid: &mut [[u8; 9]; 9]) -> bool {
    let mut best: Option<(usize, usize, Vec<u8>)> = None;
    for r in 0..9 {
        for c in 0..9 {
            if grid[r][c] == 0 {
                let candidates = candidates(grid, r, c);
                if candidates.is_empty() {
                    return false;
                }
                if best
                    .as_ref()
                    .map(|(_, _, current)| candidates.len() < current.len())
                    .unwrap_or(true)
                {
                    best = Some((r, c, candidates));
                }
            }
        }
    }

    let Some((r, c, candidates)) = best else {
        return validate(grid);
    };

    for value in candidates {
        grid[r][c] = value;
        if solve_cell(grid) {
            return true;
        }
        grid[r][c] = 0;
    }
    false
}

fn candidates(grid: &[[u8; 9]; 9], row: usize, col: usize) -> Vec<u8> {
    let mut allowed = [true; 10];
    for i in 0..9 {
        allowed[grid[row][i] as usize] = false;
        allowed[grid[i][col] as usize] = false;
    }
    let box_row = (row / 3) * 3;
    let box_col = (col / 3) * 3;
    for r in box_row..box_row + 3 {
        for c in box_col..box_col + 3 {
            allowed[grid[r][c] as usize] = false;
        }
    }
    (1..=9).filter(|&value| allowed[value as usize]).collect()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_easy_puzzle() {
        let puzzle = parse("530070000600195000098000060800060003400803001700020006060000280000419005000080079");
        let solved = solve(puzzle).expect("solvable");
        assert!(validate(&solved));
    }
}
