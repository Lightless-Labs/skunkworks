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
