//! Challenge definitions: real tasks that drive evolutionary pressure.
//!
//! Each challenge is a requirement string + a mechanical fitness evaluator.
//! The system attempts to build real software, and the failure to do so
//! drives the evolution of its enzyme capabilities.

use crate::benchmark::{BenchmarkCase, BenchmarkSuite, FitnessReport};
use crate::sandbox;
use std::time::Duration;

/// A challenge: a task description + benchmark suite for evaluation.
pub struct Challenge {
    pub name: &'static str,
    pub requirements: &'static str,
    pub benchmark: BenchmarkSuite,
    /// Acceptance test code appended to the produced artifact before
    /// compilation. The coder never sees this — it's the system's
    /// holdout validation. "Does it do what it's supposed to do?"
    pub acceptance_test: Option<String>,
}

/// Baseline: what a single model produces on the same task, evaluated
/// with the same benchmark. If the catalytic cycle can't beat this,
/// it's overhead.
#[derive(Debug, Clone)]
pub struct Baseline {
    pub provider: String,
    pub fitness: FitnessReport,
    pub compiled: bool,
    pub tests_passed: Option<usize>,
}

impl Challenge {
    /// Run the challenge requirements through a single model invocation
    /// and evaluate the result with the same benchmark. This is the bar
    /// the catalytic cycle must clear.
    pub fn establish_baseline(&self, code_output: &str, provider_name: &str) -> Baseline {
        let fitness = self.benchmark.evaluate(code_output);
        let timeout = Duration::from_secs(self.benchmark.test_timeout_secs);
        let sandbox_result =
            if let Some(code) = super::benchmark::extract_rust_code_pub(code_output) {
                sandbox::evaluate_rust_code(&code, timeout)
            } else {
                sandbox::evaluate_rust_code(code_output, timeout)
            };

        Baseline {
            provider: provider_name.to_string(),
            fitness,
            compiled: sandbox_result.compiled,
            tests_passed: sandbox_result.tests_passed,
        }
    }
}

pub fn chess_engine() -> Challenge {
    Challenge {
        name: "chess-engine",
        requirements: "\
Implement a chess engine in Rust as a SINGLE source file that compiles with `rustc --edition 2024`.

Requirements:
1. Board representation (8x8 grid with pieces)
2. A function to generate legal moves for a given position
3. A function to apply a move to a board and return the new board state
4. A function to check if a king is in check
5. A simple evaluation function (material count)
6. At least 5 unit tests covering: initial board setup, pawn moves, knight moves, check detection, and move application

Output ONLY the Rust source code inside ```rust fences. The code must compile and all tests must pass.",
        benchmark: BenchmarkSuite {
            name: "chess-engine".to_string(),
            cases: vec![
                BenchmarkCase { name: "has_board_struct".to_string(), input: String::new(), expected_output: "struct Board".to_string() },
                BenchmarkCase { name: "has_move_struct".to_string(), input: String::new(), expected_output: "struct Move".to_string() },
                BenchmarkCase { name: "has_generate_moves".to_string(), input: String::new(), expected_output: "fn generate_moves".to_string() },
                BenchmarkCase { name: "has_apply_move".to_string(), input: String::new(), expected_output: "fn apply_move".to_string() },
                BenchmarkCase { name: "has_is_check".to_string(), input: String::new(), expected_output: "fn is_check".to_string() },
            ],
            acceptance_test: None,
            test_timeout_secs: 5,
        },
        acceptance_test: Some(r#"
// === A²D ACCEPTANCE TEST ===
#[cfg(test)]
mod a2d_acceptance {
    use super::*;

    #[test]
    fn initial_board_has_32_pieces() {
        let board = Board::new();
        let mut count = 0;
        for rank in 0..8 {
            for file in 0..8 {
                if board.piece_at(rank, file).is_some() {
                    count += 1;
                }
            }
        }
        assert_eq!(count, 32, "Initial board must have 32 pieces");
    }

    #[test]
    fn white_has_legal_moves_from_start() {
        let board = Board::new();
        let moves = generate_moves(&board, true); // white to move
        // Standard chess: white has exactly 20 legal moves from starting position
        // (16 pawn moves + 4 knight moves). Accept anything >= 16.
        assert!(moves.len() >= 16,
            "White must have at least 16 legal moves from start, got {}", moves.len());
    }

    #[test]
    fn apply_move_changes_board() {
        let board = Board::new();
        let moves = generate_moves(&board, true);
        assert!(!moves.is_empty(), "Must have legal moves from starting position");
        let new_board = apply_move(&board, &moves[0]);
        // Board should be different after a move
        assert!(board.piece_at(moves[0].from_rank, moves[0].from_file) != new_board.piece_at(moves[0].from_rank, moves[0].from_file),
            "Board must change after applying a move");
    }

    #[test]
    fn king_not_in_check_at_start() {
        let board = Board::new();
        assert!(!is_check(&board, true), "White king not in check at start");
        assert!(!is_check(&board, false), "Black king not in check at start");
    }
}
"#.to_string()),
    }
}

pub fn sudoku_solver() -> Challenge {
    // The acceptance test: does calling solve() on a real puzzle produce the right answer?
    // This is appended to the produced code and compiled+run by the sandbox.
    let acceptance_test = r#"
// === A²D ACCEPTANCE TEST (appended by the system, not visible to the coder) ===
#[cfg(test)]
mod a2d_acceptance {
    use super::*;

    // Easy puzzle (many givens)
    #[test]
    fn solves_easy_puzzle() {
        let puzzle = "530070000600195000098000060800060003400803001700020006060000280000419005000080079";
        let expected = "534678912672195348198342567859761423426853791713924856961537284287419635345286179";
        let grid = parse(&puzzle);
        let solved = solve(grid);
        assert!(solved.is_some(), "Must solve easy puzzle");
        let solution = solved.unwrap();
        let result: String = solution.iter().flat_map(|row| row.iter()).map(|&c| char::from_digit(c as u32, 10).unwrap()).collect();
        assert_eq!(result, expected);
    }

    // Medium puzzle (fewer givens)
    #[test]
    fn solves_medium_puzzle() {
        let puzzle = "000260701680070090190004500820100040004602900050003028009300074040050036703018000";
        let expected = "435269781682571493197834562826195347374682915951743628519326874248957136763418259";
        let grid = parse(&puzzle);
        let solved = solve(grid);
        assert!(solved.is_some(), "Must solve medium puzzle");
        let solution = solved.unwrap();
        let result: String = solution.iter().flat_map(|row| row.iter()).map(|&c| char::from_digit(c as u32, 10).unwrap()).collect();
        assert_eq!(result, expected);
    }

    // Hard puzzle (minimal givens)
    #[test]
    fn solves_hard_puzzle() {
        let puzzle = "800000000003600000070090200050007000000045700000100030001000068008500010090000400";
        let expected = "812753649943682175675491283154237896369845721287169534521974368438526917796318452";
        let grid = parse(&puzzle);
        let solved = solve(grid);
        assert!(solved.is_some(), "Must solve hard puzzle");
        let solution = solved.unwrap();
        let result: String = solution.iter().flat_map(|row| row.iter()).map(|&c| char::from_digit(c as u32, 10).unwrap()).collect();
        assert_eq!(result, expected);
    }

    // Validation
    #[test]
    fn solved_board_validates() {
        let puzzle = "530070000600195000098000060800060003400803001700020006060000280000419005000080079";
        let grid = parse(&puzzle);
        let solution = solve(grid).unwrap();
        assert!(validate(&solution), "Solved board must validate");
    }

    #[test]
    fn rejects_invalid_puzzle() {
        // Two 5s in the first row = invalid
        let bad = "550070000600195000098000060800060003400803001700020006060000280000419005000080079";
        let grid = parse(&bad);
        let result = solve(grid);
        assert!(result.is_none(), "Must reject invalid puzzles");
    }

    #[test]
    fn empty_board_solvable() {
        let empty = "000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        let grid = parse(&empty);
        let solved = solve(grid);
        assert!(solved.is_some(), "Empty board must be solvable");
        assert!(validate(&solved.unwrap()), "Solution of empty board must validate");
    }
}
"#;

    Challenge {
        name: "sudoku-solver",
        requirements: "\
Implement a Sudoku solver in Rust as a SINGLE source file that compiles with `rustc --edition 2024`.

The file MUST define these exact public function signatures:
- `fn parse(s: &str) -> [[u8; 9]; 9]` — parse 81-char string (0=empty) into grid
- `fn solve(grid: [[u8; 9]; 9]) -> Option<[[u8; 9]; 9]>` — solve via backtracking, None if invalid
- `fn validate(grid: &[[u8; 9]; 9]) -> bool` — check if a completed grid is valid

Also include `fn main() {}` and at least 3 unit tests.

Output ONLY the Rust source code inside ```rust fences. No explanation.",
        benchmark: BenchmarkSuite {
            name: "sudoku-solver".to_string(),
            cases: vec![
                BenchmarkCase {
                    name: "has_parse_fn".to_string(),
                    input: String::new(),
                    expected_output: "fn parse".to_string(),
                },
                BenchmarkCase {
                    name: "has_solve_fn".to_string(),
                    input: String::new(),
                    expected_output: "fn solve".to_string(),
                },
                BenchmarkCase {
                    name: "has_validate_fn".to_string(),
                    input: String::new(),
                    expected_output: "fn validate".to_string(),
                },
            ],
            acceptance_test: None,
            test_timeout_secs: 10,
        },
        acceptance_test: Some(acceptance_test.to_string()),
    }
}

pub fn rubiks_cube() -> Challenge {
    Challenge {
        name: "rubiks-cube",
        requirements: "\
Implement a Rubik's cube representation and solver in Rust as a SINGLE source file that compiles with `rustc --edition 2024`.

Requirements:
1. A cube representation (6 faces, 9 cells each)
2. Functions for each face rotation (U, D, L, R, F, B and their inverses)
3. A function to check if the cube is solved
4. A scramble function that applies random moves
5. A simple solver (even layer-by-layer beginner method is fine)
6. At least 4 unit tests: solved state detection, single rotation and inverse restores state, scramble changes state, basic solve works

Output ONLY the Rust source code inside ```rust fences. The code must compile and all tests must pass.",
        benchmark: BenchmarkSuite {
            name: "rubiks-cube".to_string(),
            cases: vec![
                BenchmarkCase { name: "has_cube_struct".to_string(), input: String::new(), expected_output: "struct Cube".to_string() },
                BenchmarkCase { name: "has_rotate".to_string(), input: String::new(), expected_output: "fn rotate".to_string() },
                BenchmarkCase { name: "has_is_solved".to_string(), input: String::new(), expected_output: "fn is_solved".to_string() },
                BenchmarkCase { name: "has_solve".to_string(), input: String::new(), expected_output: "fn solve".to_string() },
            ],
            acceptance_test: None,
            test_timeout_secs: 60,
        },
        acceptance_test: None,
    }
}
