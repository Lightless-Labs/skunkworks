//! Challenge definitions: real tasks that drive evolutionary pressure.
//!
//! Each challenge is a requirement string + a mechanical fitness evaluator.
//! The system attempts to build real software, and the failure to do so
//! drives the evolution of its enzyme capabilities.

use a2d_core::benchmark::{BenchmarkCase, BenchmarkSuite, FitnessReport};

/// A challenge: a task description + benchmark suite for evaluation.
pub struct Challenge {
    pub name: &'static str,
    pub requirements: &'static str,
    benchmark: BenchmarkSuite,
    /// Acceptance test code appended to the produced artifact before
    /// compilation. The coder never sees this — it's the system's
    /// holdout validation. "Does it do what it's supposed to do?"
    acceptance_test: Option<String>,
}

/// Baseline: what a single model produces on the same task, evaluated
/// with the same benchmark. If the catalytic cycle can't beat this,
/// it's overhead.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Baseline {
    pub provider: String,
    pub fitness: FitnessReport,
    pub compiled: bool,
    pub tests_passed: Option<usize>,
}

impl Challenge {
    /// Benchmark configured exactly as the challenge runtime sees it.
    ///
    /// This is the only supported way for callers outside this module to
    /// obtain a benchmark for a challenge: it attaches the hidden acceptance
    /// tests so replay/baseline paths cannot accidentally score only visible
    /// string checks.
    pub fn scoring_benchmark(&self) -> BenchmarkSuite {
        let mut benchmark = self.benchmark.clone();
        benchmark.acceptance_test = self.acceptance_test.clone();
        benchmark
    }

    /// Score a generated artifact with this challenge's visible checks and
    /// hidden acceptance tests.
    pub fn score_artifact(&self, code_output: &str) -> FitnessReport {
        self.scoring_benchmark().evaluate(code_output)
    }

    /// Run the challenge requirements through a single model invocation
    /// and evaluate the result with the same benchmark. This is the bar
    /// the catalytic cycle must clear.
    #[allow(dead_code)]
    pub fn establish_baseline(&self, code_output: &str, provider_name: &str) -> Baseline {
        let fitness = self.score_artifact(code_output);
        let compiled = fitness
            .results
            .iter()
            .any(|result| result.name == "compiles" && result.passed);

        Baseline {
            provider: provider_name.to_string(),
            fitness,
            compiled,
            tests_passed: None,
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
2. Define `#[derive(Clone, Copy, Debug, PartialEq, Eq)] struct Piece` or equivalent piece type usable in `Option<Piece>` equality checks
3. Define `pub struct Board` with `pub fn new() -> Self` for the standard starting position and `pub fn piece_at(&self, rank: usize, file: usize) -> Option<Piece>`
4. Define `pub struct Move { pub from_rank: usize, pub from_file: usize, pub to_rank: usize, pub to_file: usize, ... }`
5. Define `pub fn generate_moves(board: &Board, white: bool) -> Vec<Move>` returning legal moves only, never moves that leave the moving side's king in check
6. Define `pub fn apply_move(board: &Board, mv: &Move) -> Board` and update all game state needed for castling rights and en-passant captures
7. Define `pub fn is_check(board: &Board, white: bool) -> bool`
8. Include legal pawn movement, knight movement, check detection, castling, en passant, and checkmate/no-escape handling
9. Include a simple evaluation function (material count)
10. At least 5 unit tests covering: initial board setup, pawn moves, knight moves, check detection, and move application

Output ONLY the Rust source code inside ```rust fences. The code must compile and all tests must pass.",
        benchmark: BenchmarkSuite {
            name: "chess-engine".to_string(),
            cases: vec![
                BenchmarkCase { name: "has_board_struct".to_string(), input: String::new(), expected_output: "struct Board".to_string() },
                BenchmarkCase { name: "has_move_struct".to_string(), input: String::new(), expected_output: "struct Move".to_string() },
                BenchmarkCase { name: "has_generate_moves".to_string(), input: String::new(), expected_output: "fn generate_moves".to_string() },
                BenchmarkCase { name: "has_apply_move".to_string(), input: String::new(), expected_output: "fn apply_move".to_string() },
                BenchmarkCase { name: "has_is_check".to_string(), input: String::new(), expected_output: "fn is_check".to_string() },
                BenchmarkCase { name: "has_piece_at".to_string(), input: String::new(), expected_output: "fn piece_at".to_string() },
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

    fn count_pieces(board: &Board) -> usize {
        let mut count = 0;
        for rank in 0..8 {
            for file in 0..8 {
                if board.piece_at(rank, file).is_some() {
                    count += 1;
                }
            }
        }
        count
    }

    fn play(board: Board, white: bool, from_rank: usize, from_file: usize, to_rank: usize, to_file: usize) -> Option<Board> {
        generate_moves(&board, white)
            .into_iter()
            .find(|mv| {
                mv.from_rank == from_rank
                    && mv.from_file == from_file
                    && mv.to_rank == to_rank
                    && mv.to_file == to_file
            })
            .map(|mv| apply_move(&board, &mv))
    }

    #[test]
    fn generated_moves_do_not_leave_own_king_in_check() {
        let board = Board::new();
        for white in [true, false] {
            for mv in generate_moves(&board, white) {
                let next = apply_move(&board, &mv);
                assert!(
                    !is_check(&next, white),
                    "generated move from ({}, {}) to ({}, {}) must not leave moving side in check",
                    mv.from_rank,
                    mv.from_file,
                    mv.to_rank,
                    mv.to_file
                );
            }
        }
    }

    #[test]
    fn kingside_castling_is_generated_after_path_is_clear_and_moves_rook() {
        for white_back_rank in [0usize, 7usize] {
            let white_dir: isize = if white_back_rank == 0 { 1 } else { -1 };
            let black_back_rank = 7 - white_back_rank;
            let black_dir = -white_dir;
            let wr = |steps: isize| (white_back_rank as isize + steps * white_dir) as usize;
            let br = |steps: isize| (black_back_rank as isize + steps * black_dir) as usize;

            let mut board = Board::new();
            board = match play(board, true, wr(1), 4, wr(3), 4) { Some(board) => board, None => continue };
            board = match play(board, false, br(1), 4, br(3), 4) { Some(board) => board, None => continue };
            board = match play(board, true, wr(0), 6, wr(2), 5) { Some(board) => board, None => continue };
            board = match play(board, false, br(0), 1, br(2), 2) { Some(board) => board, None => continue };
            board = match play(board, true, wr(0), 5, wr(3), 2) { Some(board) => board, None => continue };
            board = match play(board, false, br(0), 6, br(2), 5) { Some(board) => board, None => continue };

            let before_count = count_pieces(&board);
            let castle = generate_moves(&board, true)
                .into_iter()
                .find(|mv| mv.from_rank == wr(0) && mv.from_file == 4 && mv.to_rank == wr(0) && mv.to_file == 6);
            assert!(castle.is_some(), "white kingside castling must be generated after e4/e5/Nf3/Nc6/Bc4/Nf6");

            let after = apply_move(&board, &castle.unwrap());
            assert_eq!(count_pieces(&after), before_count, "castling must not capture a piece");
            assert!(after.piece_at(wr(0), 4).is_none(), "king origin must be empty after castling");
            assert!(after.piece_at(wr(0), 6).is_some(), "king destination must be occupied after castling");
            assert!(after.piece_at(wr(0), 7).is_none(), "rook origin must be empty after castling");
            assert!(after.piece_at(wr(0), 5).is_some(), "rook must move next to king after castling");
            return;
        }

        panic!("could not establish a standard castling setup from Board::new()");
    }

    #[test]
    fn en_passant_is_generated_immediately_and_removes_captured_pawn() {
        for white_pawn_rank in [1usize, 6usize] {
            let white_dir: isize = if white_pawn_rank == 1 { 1 } else { -1 };
            let black_pawn_rank = 7 - white_pawn_rank;
            let black_dir = -white_dir;
            let wr = |steps: isize| (white_pawn_rank as isize + steps * white_dir) as usize;
            let br = |steps: isize| (black_pawn_rank as isize + steps * black_dir) as usize;

            let mut board = Board::new();
            board = match play(board, true, wr(0), 4, wr(2), 4) { Some(board) => board, None => continue };
            board = match play(board, false, br(0), 0, br(1), 0) { Some(board) => board, None => continue };
            board = match play(board, true, wr(2), 4, wr(3), 4) { Some(board) => board, None => continue };
            board = match play(board, false, br(0), 3, br(2), 3) { Some(board) => board, None => continue };

            let en_passant = generate_moves(&board, true)
                .into_iter()
                .find(|mv| mv.from_rank == wr(3) && mv.from_file == 4 && mv.to_rank == wr(4) && mv.to_file == 3);
            assert!(en_passant.is_some(), "en-passant capture must be generated immediately after a double pawn push");

            let after = apply_move(&board, &en_passant.unwrap());
            assert!(after.piece_at(wr(3), 4).is_none(), "capturing pawn origin must be empty");
            assert!(after.piece_at(wr(4), 3).is_some(), "capturing pawn must land on the en-passant target square");
            assert!(after.piece_at(wr(3), 3).is_none(), "captured pawn must be removed from its bypassed square");
            return;
        }

        panic!("could not establish an en-passant setup from Board::new()");
    }

    #[test]
    fn fools_mate_leaves_checked_side_with_no_legal_moves() {
        for white_pawn_rank in [1usize, 6usize] {
            let white_dir: isize = if white_pawn_rank == 1 { 1 } else { -1 };
            let black_back_rank = if white_pawn_rank == 1 { 7usize } else { 0usize };
            let black_pawn_rank = 7 - white_pawn_rank;
            let black_dir = -white_dir;
            let wr = |steps: isize| (white_pawn_rank as isize + steps * white_dir) as usize;
            let br = |steps: isize| (black_pawn_rank as isize + steps * black_dir) as usize;
            let bbr = |steps: isize| (black_back_rank as isize + steps * black_dir) as usize;

            let mut board = Board::new();
            board = match play(board, true, wr(0), 5, wr(1), 5) { Some(board) => board, None => continue };
            board = match play(board, false, br(0), 4, br(2), 4) { Some(board) => board, None => continue };
            board = match play(board, true, wr(0), 6, wr(2), 6) { Some(board) => board, None => continue };
            board = match play(board, false, bbr(0), 3, bbr(4), 7) { Some(board) => board, None => continue };

            assert!(is_check(&board, true), "Fool's mate position must put white in check");
            assert!(generate_moves(&board, true).is_empty(), "checkmated side must have zero legal moves");
            return;
        }

        panic!("could not establish Fool's mate from Board::new()");
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
1. Define `#[derive(Clone, Debug, PartialEq, Eq)] struct Cube` (or an equivalent public cube representation) for a solved cube with 6 faces and 9 cells each
2. Define `#[derive(Clone, Copy, Debug, PartialEq, Eq)] enum Move { U, UPrime, D, DPrime, L, LPrime, R, RPrime, F, FPrime, B, BPrime }`
3. Define `impl Cube { fn new() -> Self }` returning a solved cube
4. Define `fn rotate(cube: &mut Cube, mv: Move)` for each face quarter-turn and inverse
5. Define `fn is_solved(cube: &Cube) -> bool`
6. Define `fn scramble(cube: &mut Cube, len: usize, seed: u64) -> Vec<Move>`; it must be deterministic for a given seed, apply exactly the returned moves, and return exactly `len` moves
7. Define `fn solve(cube: &Cube) -> Option<Vec<Move>>`; returned moves must solve cubes reachable from `Cube::new()` by `rotate`
8. At least 4 unit tests: solved state detection, single rotation and inverse restores state, scramble changes state, basic solve works

Output ONLY the Rust source code inside ```rust fences. The code must compile and all tests must pass.",
        benchmark: BenchmarkSuite {
            name: "rubiks-cube".to_string(),
            cases: vec![
                BenchmarkCase { name: "has_cube_struct".to_string(), input: String::new(), expected_output: "struct Cube".to_string() },
                BenchmarkCase { name: "has_rotate".to_string(), input: String::new(), expected_output: "fn rotate".to_string() },
                BenchmarkCase { name: "has_is_solved".to_string(), input: String::new(), expected_output: "fn is_solved".to_string() },
                BenchmarkCase { name: "has_solve".to_string(), input: String::new(), expected_output: "fn solve".to_string() },
                BenchmarkCase { name: "has_scramble".to_string(), input: String::new(), expected_output: "fn scramble".to_string() },
                BenchmarkCase { name: "has_move_enum".to_string(), input: String::new(), expected_output: "enum Move".to_string() },
            ],
            acceptance_test: None,
            test_timeout_secs: 60,
        },
        acceptance_test: Some(r#"
// === A²D RUBIKS ACCEPTANCE TEST ===
#[cfg(test)]
mod a2d_rubiks_acceptance {
    use super::*;

    fn inverse(mv: Move) -> Move {
        match mv {
            Move::U => Move::UPrime,
            Move::UPrime => Move::U,
            Move::D => Move::DPrime,
            Move::DPrime => Move::D,
            Move::L => Move::LPrime,
            Move::LPrime => Move::L,
            Move::R => Move::RPrime,
            Move::RPrime => Move::R,
            Move::F => Move::FPrime,
            Move::FPrime => Move::F,
            Move::B => Move::BPrime,
            Move::BPrime => Move::B,
        }
    }

    fn all_moves() -> [Move; 12] {
        [
            Move::U, Move::UPrime,
            Move::D, Move::DPrime,
            Move::L, Move::LPrime,
            Move::R, Move::RPrime,
            Move::F, Move::FPrime,
            Move::B, Move::BPrime,
        ]
    }

    fn face_turns() -> [Move; 6] {
        [Move::U, Move::D, Move::L, Move::R, Move::F, Move::B]
    }

    fn apply_moves(cube: &mut Cube, moves: &[Move]) {
        for &mv in moves {
            rotate(cube, mv);
        }
    }

    #[test]
    fn solved_cube_starts_solved() {
        let cube = Cube::new();
        assert!(is_solved(&cube), "Cube::new() must produce a solved cube");
    }

    #[test]
    fn every_rotation_changes_solved_cube_and_inverse_restores() {
        for mv in all_moves() {
            let mut cube = Cube::new();
            rotate(&mut cube, mv);
            assert!(!is_solved(&cube), "{:?} must not be a no-op on a solved cube", mv);
            rotate(&mut cube, inverse(mv));
            assert!(is_solved(&cube), "{:?} followed by its inverse must restore solved state", mv);
        }
    }

    #[test]
    fn quarter_turns_have_order_four_not_two() {
        for mv in face_turns() {
            let mut cube = Cube::new();
            rotate(&mut cube, mv);
            assert!(!is_solved(&cube), "{:?} once should not solve", mv);
            rotate(&mut cube, mv);
            assert!(!is_solved(&cube), "{:?} twice should not solve", mv);
            rotate(&mut cube, mv);
            assert!(!is_solved(&cube), "{:?} three times should not solve", mv);
            rotate(&mut cube, mv);
            assert!(is_solved(&cube), "{:?} four times should restore solved", mv);
        }
    }

    #[test]
    fn known_sequence_inverse_roundtrip_restores_solved() {
        let sequence = [
            Move::U, Move::R, Move::F, Move::LPrime,
            Move::D, Move::B, Move::RPrime, Move::UPrime,
            Move::FPrime, Move::L,
        ];
        let mut cube = Cube::new();
        apply_moves(&mut cube, &sequence);
        assert!(!is_solved(&cube), "known scramble must change cube");
        for &mv in sequence.iter().rev() {
            rotate(&mut cube, inverse(mv));
        }
        assert!(is_solved(&cube), "reverse inverse sequence must restore solved state");
    }

    #[test]
    fn solver_solves_known_scrambles_by_returning_moves() {
        let cases: &[&[Move]] = &[
            &[Move::U],
            &[Move::U, Move::R, Move::F],
            &[Move::U, Move::R, Move::F, Move::LPrime, Move::D, Move::B, Move::RPrime, Move::UPrime],
        ];
        for (index, sequence) in cases.iter().enumerate() {
            let mut cube = Cube::new();
            apply_moves(&mut cube, sequence);
            assert!(!is_solved(&cube), "case {} should be scrambled", index);
            let solution = solve(&cube).unwrap_or_else(|| panic!("solver returned None for case {}", index));
            assert!(solution.len() <= 500, "solution for case {} is unreasonably long: {} moves", index, solution.len());
            apply_moves(&mut cube, &solution);
            assert!(is_solved(&cube), "solver failed to solve case {}", index);
        }
    }

    #[test]
    fn seeded_scramble_is_replayable_and_solvable() {
        let mut cube = Cube::new();
        let scramble_moves = scramble(&mut cube, 12, 0xA2D5_EED);
        assert_eq!(scramble_moves.len(), 12, "scramble must return len moves");
        assert!(!is_solved(&cube), "non-empty scramble must change cube");

        let mut replay = Cube::new();
        apply_moves(&mut replay, &scramble_moves);
        assert_eq!(cube, replay, "scramble must apply exactly the moves it returns");

        let solution = solve(&cube).expect("solver must solve seeded scramble");
        apply_moves(&mut cube, &solution);
        assert!(is_solved(&cube), "solver must solve scrambled cube");
    }
}
"#.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chess_acceptance_covers_special_moves_and_checkmate() {
        let challenge = chess_engine();
        let acceptance = challenge
            .acceptance_test
            .as_ref()
            .expect("chess challenge should have hidden acceptance tests");

        assert!(acceptance.contains("kingside_castling"));
        assert!(acceptance.contains("en_passant"));
        assert!(acceptance.contains("fools_mate"));
        assert!(acceptance.contains("generated_moves_do_not_leave_own_king_in_check"));
        assert!(challenge.requirements.contains("castling"));
        assert!(challenge.requirements.contains("en passant"));
    }

    #[test]
    fn rubiks_acceptance_covers_scramble_solve_roundtrip() {
        let challenge = rubiks_cube();
        let acceptance = challenge
            .acceptance_test
            .as_ref()
            .expect("rubiks challenge should have hidden acceptance tests");

        assert!(acceptance.contains("seeded_scramble_is_replayable_and_solvable"));
        assert!(acceptance.contains("solver_solves_known_scrambles_by_returning_moves"));
        assert!(acceptance.contains("known_sequence_inverse_roundtrip_restores_solved"));
        assert!(challenge.requirements.contains("fn scramble"));
        assert!(challenge.requirements.contains("Option<Vec<Move>>"));
    }

    #[test]
    fn scoring_benchmark_carries_acceptance_for_all_challenges() {
        for challenge in [chess_engine(), sudoku_solver(), rubiks_cube()] {
            assert!(
                challenge.scoring_benchmark().acceptance_test.is_some(),
                "{} scoring benchmark must carry hidden acceptance tests",
                challenge.name
            );
        }
    }

    #[test]
    fn score_artifact_uses_sudoku_acceptance_tests() {
        let report = sudoku_solver().score_artifact(fake_sudoku_artifact_with_bad_solver());

        assert!(case_passed(&report, "compiles"));
        assert!(case_passed(&report, "has_tests"));
        assert!(case_passed(&report, "has_parse_fn"));
        assert!(case_passed(&report, "has_solve_fn"));
        assert!(case_passed(&report, "has_validate_fn"));
        assert!(!case_passed(&report, "all_tests_pass"));
        assert!(report.fitness < 1.0);
    }

    #[test]
    fn establish_baseline_uses_acceptance_tests() {
        let baseline =
            sudoku_solver().establish_baseline(fake_sudoku_artifact_with_bad_solver(), "fake");

        assert!(baseline.compiled);
        assert!(!case_passed(&baseline.fitness, "all_tests_pass"));
        assert!(baseline.fitness.fitness < 1.0);
    }

    fn case_passed(report: &FitnessReport, name: &str) -> bool {
        report
            .results
            .iter()
            .any(|result| result.name == name && result.passed)
    }

    fn fake_sudoku_artifact_with_bad_solver() -> &'static str {
        r#"
fn parse(s: &str) -> [[u8; 9]; 9] {
    let mut grid = [[0u8; 9]; 9];
    for (idx, ch) in s.chars().take(81).enumerate() {
        grid[idx / 9][idx % 9] = ch.to_digit(10).unwrap_or(0) as u8;
    }
    grid
}

fn solve(_grid: [[u8; 9]; 9]) -> Option<[[u8; 9]; 9]> {
    Some([[1u8; 9]; 9])
}

fn validate(_grid: &[[u8; 9]; 9]) -> bool {
    true
}

fn main() {}

#[cfg(test)]
mod tests {
    #[test]
    fn local_smoke_passes() {
        assert!(true);
    }
}
"#
    }
}
