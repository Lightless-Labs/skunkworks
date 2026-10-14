#[test]
fn middle_ranks_start_empty() {
    let board = Board::new();
    for rank in 2..6 {
        for file in 0..8 {
            assert!(board.piece_at(rank, file).is_none(), "rank {rank} file {file}");
        }
    }
}

#[test]
fn both_sides_have_twenty_moves_from_start() {
    let board = Board::new();
    assert_eq!(generate_moves(&board, true).len(), 20);
    assert_eq!(generate_moves(&board, false).len(), 20);
}
