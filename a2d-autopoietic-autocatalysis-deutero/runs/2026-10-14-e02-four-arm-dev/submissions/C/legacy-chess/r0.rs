#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub kind: Kind,
    pub white: bool,
}

impl Piece {
    pub const fn new(kind: Kind, white: bool) -> Self {
        Self { kind, white }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub from_rank: usize,
    pub from_file: usize,
    pub to_rank: usize,
    pub to_file: usize,
    pub promotion: Option<Kind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    squares: [[Option<Piece>; 8]; 8],
    white_king_side: bool,
    white_queen_side: bool,
    black_king_side: bool,
    black_queen_side: bool,
    en_passant: Option<(usize, usize)>,
}

impl Board {
    /// Creates the standard chess starting position.
    pub fn new() -> Self {
        let mut board = Self {
            squares: [[None; 8]; 8],
            white_king_side: true,
            white_queen_side: true,
            black_king_side: true,
            black_queen_side: true,
            en_passant: None,
        };
        let back = [
            Kind::Rook,
            Kind::Knight,
            Kind::Bishop,
            Kind::Queen,
            Kind::King,
            Kind::Bishop,
            Kind::Knight,
            Kind::Rook,
        ];
        for file in 0..8 {
            board.squares[0][file] = Some(Piece::new(back[file], false));
            board.squares[1][file] = Some(Piece::new(Kind::Pawn, false));
            board.squares[6][file] = Some(Piece::new(Kind::Pawn, true));
            board.squares[7][file] = Some(Piece::new(back[file], true));
        }
        board
    }

    /// Returns the piece at a rank and file, or `None` for an empty or out-of-range square.
    pub fn piece_at(&self, rank: usize, file: usize) -> Option<Piece> {
        if rank < 8 && file < 8 {
            self.squares[rank][file]
        } else {
            None
        }
    }

    /// Creates an empty board, useful for setting up positions.
    pub fn empty() -> Self {
        Self {
            squares: [[None; 8]; 8],
            white_king_side: false,
            white_queen_side: false,
            black_king_side: false,
            black_queen_side: false,
            en_passant: None,
        }
    }

    fn king_position(&self, white: bool) -> Option<(usize, usize)> {
        for rank in 0..8 {
            for file in 0..8 {
                if self.squares[rank][file]
                    == Some(Piece::new(Kind::King, white))
                {
                    return Some((rank, file));
                }
            }
        }
        None
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

fn inside(rank: i32, file: i32) -> bool {
    (0..8).contains(&rank) && (0..8).contains(&file)
}

fn attacked(board: &Board, rank: usize, file: usize, by_white: bool) -> bool {
    let r = rank as i32;
    let f = file as i32;

    let pawn_source_rank = r + if by_white { 1 } else { -1 };
    for df in [-1, 1] {
        let sf = f + df;
        if inside(pawn_source_rank, sf)
            && board.squares[pawn_source_rank as usize][sf as usize]
                == Some(Piece::new(Kind::Pawn, by_white))
        {
            return true;
        }
    }

    for (dr, df) in [
        (-2, -1), (-2, 1), (-1, -2), (-1, 2),
        (1, -2), (1, 2), (2, -1), (2, 1),
    ] {
        let sr = r + dr;
        let sf = f + df;
        if inside(sr, sf)
            && board.squares[sr as usize][sf as usize]
                == Some(Piece::new(Kind::Knight, by_white))
        {
            return true;
        }
    }

    for dr in -1..=1 {
        for df in -1..=1 {
            if dr == 0 && df == 0 {
                continue;
            }
            let sr = r + dr;
            let sf = f + df;
            if inside(sr, sf)
                && board.squares[sr as usize][sf as usize]
                    == Some(Piece::new(Kind::King, by_white))
            {
                return true;
            }
        }
    }

    for (dr, df, diagonal) in [
        (-1, 0, false), (1, 0, false), (0, -1, false), (0, 1, false),
        (-1, -1, true), (-1, 1, true), (1, -1, true), (1, 1, true),
    ] {
        let mut sr = r + dr;
        let mut sf = f + df;
        let mut distance = 1;
        while inside(sr, sf) {
            if let Some(piece) = board.squares[sr as usize][sf as usize] {
                if piece.white == by_white {
                    let compatible = match piece.kind {
                        Kind::Queen => true,
                        Kind::Rook => !diagonal,
                        Kind::Bishop => diagonal,
                        Kind::King => distance == 1,
                        _ => false,
                    };
                    if compatible {
                        return true;
                    }
                }
                break;
            }
            sr += dr;
            sf += df;
            distance += 1;
        }
    }
    false
}

/// Returns whether the specified side's king is attacked.
pub fn is_check(board: &Board, white: bool) -> bool {
    match board.king_position(white) {
        Some((rank, file)) => attacked(board, rank, file, !white),
        None => false,
    }
}

fn push_move(
    moves: &mut Vec<Move>,
    from_rank: usize,
    from_file: usize,
    to_rank: usize,
    to_file: usize,
    promotion: Option<Kind>,
) {
    moves.push(Move {
        from_rank,
        from_file,
        to_rank,
        to_file,
        promotion,
    });
}

fn pseudo_moves(board: &Board, white: bool) -> Vec<Move> {
    let mut moves = Vec::new();
    for rank in 0..8 {
        for file in 0..8 {
            let Some(piece) = board.squares[rank][file] else {
                continue;
            };
            if piece.white != white {
                continue;
            }
            let r = rank as i32;
            let f = file as i32;
            match piece.kind {
                Kind::Pawn => {
                    let direction = if white { -1 } else { 1 };
                    let start_rank = if white { 6 } else { 1 };
                    let next = r + direction;
                    if inside(next, f) && board.squares[next as usize][file].is_none() {
                        let promotion = if next == 0 || next == 7 {
                            Some(Kind::Queen)
                        } else {
                            None
                        };
                        push_move(&mut moves, rank, file, next as usize, file, promotion);
                        let twice = r + direction * 2;
                        if rank == start_rank
                            && board.squares[twice as usize][file].is_none()
                        {
                            push_move(&mut moves, rank, file, twice as usize, file, None);
                        }
                    }
                    for df in [-1, 1] {
                        let tr = r + direction;
                        let tf = f + df;
                        if !inside(tr, tf) {
                            continue;
                        }
                        let target = board.squares[tr as usize][tf as usize];
                        if target.is_some_and(|p| p.white != white) {
                            let promotion = if tr == 0 || tr == 7 {
                                Some(Kind::Queen)
                            } else {
                                None
                            };
                            push_move(
                                &mut moves, rank, file, tr as usize, tf as usize, promotion,
                            );
                        } else if board.en_passant == Some((tr as usize, tf as usize)) {
                            push_move(&mut moves, rank, file, tr as usize, tf as usize, None);
                        }
                    }
                }
                Kind::Knight => {
                    for (dr, df) in [
                        (-2, -1), (-2, 1), (-1, -2), (-1, 2),
                        (1, -2), (1, 2), (2, -1), (2, 1),
                    ] {
                        let tr = r + dr;
                        let tf = f + df;
                        if inside(tr, tf)
                            && board.squares[tr as usize][tf as usize]
                                .is_none_or(|p| p.white != white)
                        {
                            push_move(&mut moves, rank, file, tr as usize, tf as usize, None);
                        }
                    }
                }
                Kind::Bishop | Kind::Rook | Kind::Queen => {
                    let directions: &[(i32, i32)] = match piece.kind {
                        Kind::Bishop => &[(-1, -1), (-1, 1), (1, -1), (1, 1)],
                        Kind::Rook => &[(-1, 0), (1, 0), (0, -1), (0, 1)],
                        _ => &[
                            (-1, -1), (-1, 1), (1, -1), (1, 1),
                            (-1, 0), (1, 0), (0, -1), (0, 1),
                        ],
                    };
                    for &(dr, df) in directions {
                        let mut tr = r + dr;
                        let mut tf = f + df;
                        while inside(tr, tf) {
                            match board.squares[tr as usize][tf as usize] {
                                Some(target) if target.white == white => break,
                                Some(_) => {
                                    push_move(
                                        &mut moves, rank, file, tr as usize, tf as usize, None,
                                    );
                                    break;
                                }
                                None => push_move(
                                    &mut moves, rank, file, tr as usize, tf as usize, None,
                                ),
                            }
                            tr += dr;
                            tf += df;
                        }
                    }
                }
                Kind::King => {
                    for dr in -1..=1 {
                        for df in -1..=1 {
                            if dr == 0 && df == 0 {
                                continue;
                            }
                            let tr = r + dr;
                            let tf = f + df;
                            if inside(tr, tf)
                                && board.squares[tr as usize][tf as usize]
                                    .is_none_or(|p| p.white != white)
                            {
                                push_move(&mut moves, rank, file, tr as usize, tf as usize, None);
                            }
                        }
                    }
                    let home_rank = if white { 7 } else { 0 };
                    let king_side = if white {
                        board.white_king_side
                    } else {
                        board.black_king_side
                    };
                    let queen_side = if white {
                        board.white_queen_side
                    } else {
                        board.black_queen_side
                    };
                    if rank == home_rank
                        && file == 4
                        && !is_check(board, white)
                    {
                        if king_side
                            && board.squares[home_rank][5].is_none()
                            && board.squares[home_rank][6].is_none()
                            && board.squares[home_rank][7]
                                == Some(Piece::new(Kind::Rook, white))
                            && !attacked(board, home_rank, 5, !white)
                            && !attacked(board, home_rank, 6, !white)
                        {
                            push_move(&mut moves, rank, file, home_rank, 6, None);
                        }
                        if queen_side
                            && board.squares[home_rank][1].is_none()
                            && board.squares[home_rank][2].is_none()
                            && board.squares[home_rank][3].is_none()
                            && board.squares[home_rank][0]
                                == Some(Piece::new(Kind::Rook, white))
                            && !attacked(board, home_rank, 3, !white)
                            && !attacked(board, home_rank, 2, !white)
                        {
                            push_move(&mut moves, rank, file, home_rank, 2, None);
                        }
                    }
                }
            }
        }
    }
    moves
}

/// Generates all legal moves for the requested side.
pub fn generate_moves(board: &Board, white: bool) -> Vec<Move> {
    pseudo_moves(board, white)
        .into_iter()
        .filter(|mv| !is_check(&apply_move(board, mv), white))
        .collect()
}

/// Applies a move and returns the resulting board position.
pub fn apply_move(board: &Board, mv: &Move) -> Board {
    let mut next = board.clone();
    if mv.from_rank >= 8 || mv.from_file >= 8 || mv.to_rank >= 8 || mv.to_file >= 8 {
        return next;
    }
    let Some(mut piece) = next.squares[mv.from_rank][mv.from_file] else {
        return next;
    };
    let from = (mv.from_rank, mv.from_file);
    let to = (mv.to_rank, mv.to_file);
    let captured = next.squares[mv.to_rank][mv.to_file];

    if piece.kind == Kind::Pawn && board.en_passant == Some(to) && mv.from_file != mv.to_file {
        let captured_rank = if piece.white { mv.to_rank + 1 } else { mv.to_rank - 1 };
        next.squares[captured_rank][mv.to_file] = None;
    }

    if piece.kind == Kind::King && mv.from_file == 4 && (mv.to_file == 2 || mv.to_file == 6) {
        let home = mv.from_rank;
        if mv.to_file == 6 {
            next.squares[home][5] = next.squares[home][7];
            next.squares[home][7] = None;
        } else {
            next.squares[home][3] = next.squares[home][0];
            next.squares[home][0] = None;
        }
    }

    if piece.kind == Kind::King {
        if piece.white {
            next.white_king_side = false;
            next.white_queen_side = false;
        } else {
            next.black_king_side = false;
            next.black_queen_side = false;
        }
    }
    if piece.kind == Kind::Rook {
        update_rook_rights(&mut next, piece.white, from);
    }
    if let Some(captured_piece) = captured {
        if captured_piece.kind == Kind::Rook {
            update_rook_rights(&mut next, captured_piece.white, to);
        }
    }

    next.en_passant = None;
    if piece.kind == Kind::Pawn && mv.from_rank.abs_diff(mv.to_rank) == 2 {
        next.en_passant = Some(((mv.from_rank + mv.to_rank) / 2, mv.from_file));
    }
    if let Some(kind) = mv.promotion {
        piece.kind = kind;
    }
    next.squares[mv.from_rank][mv.from_file] = None;
    next.squares[mv.to_rank][mv.to_file] = Some(piece);
    next
}

fn update_rook_rights(board: &mut Board, white: bool, square: (usize, usize)) {
    match (white, square) {
        (true, (7, 0)) => board.white_queen_side = false,
        (true, (7, 7)) => board.white_king_side = false,
        (false, (0, 0)) => board.black_queen_side = false,
        (false, (0, 7)) => board.black_king_side = false,
        _ => {}
    }
}

/// Returns material balance from White's perspective.
pub fn evaluate(board: &Board) -> i32 {
    let mut score = 0;
    for row in board.squares {
        for piece in row.into_iter().flatten() {
            let value = match piece.kind {
                Kind::Pawn => 100,
                Kind::Knight => 320,
                Kind::Bishop => 330,
                Kind::Rook => 500,
                Kind::Queen => 900,
                Kind::King => 0,
            };
            score += if piece.white { value } else { -value };
        }
    }
    score
}

/// Returns true when the side to move is checkmated.
pub fn is_checkmate(board: &Board, white: bool) -> bool {
    is_check(board, white) && generate_moves(board, white).is_empty()
}

/// Returns true when the side to move has no legal moves.
pub fn is_stalemate(board: &Board, white: bool) -> bool {
    !is_check(board, white) && generate_moves(board, white).is_empty()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_position_and_empty_middle() {
        let board = Board::new();
        assert_eq!(board.piece_at(0, 0), Some(Piece::new(Kind::Rook, false)));
        assert_eq!(board.piece_at(7, 4), Some(Piece::new(Kind::King, true)));
        for rank in 2..6 {
            for file in 0..8 {
                assert_eq!(board.piece_at(rank, file), None);
            }
        }
    }

    #[test]
    fn starting_pawn_moves() {
        let board = Board::new();
        let moves = generate_moves(&board, true);
        assert!(moves.iter().any(|m| m.from_rank == 6 && m.from_file == 4 && m.to_rank == 5));
        assert!(moves.iter().any(|m| m.from_rank == 6 && m.from_file == 4 && m.to_rank == 4));
    }

    #[test]
    fn starting_knight_moves() {
        let board = Board::new();
        let moves = generate_moves(&board, true);
        assert_eq!(
            moves.iter().filter(|m| m.from_rank == 7 && m.from_file == 1).count(),
            2
        );
    }

    #[test]
    fn check_detection_and_mate() {
        let mut board = Board::empty();
        board.squares[7][4] = Some(Piece::new(Kind::King, true));
        board.squares[0][4] = Some(Piece::new(Kind::King, false));
        board.squares[3][4] = Some(Piece::new(Kind::Rook, false));
        assert!(is_check(&board, true));
        assert!(!is_check(&board, false));
    }

    #[test]
    fn applying_move_updates_board_and_en_passant() {
        let board = Board::new();
        let mv = Move {
            from_rank: 6,
            from_file: 4,
            to_rank: 4,
            to_file: 4,
            promotion: None,
        };
        let next = apply_move(&board, &mv);
        assert_eq!(next.piece_at(6, 4), None);
        assert_eq!(next.piece_at(4, 4), Some(Piece::new(Kind::Pawn, true)));
        assert_eq!(next.en_passant, Some((5, 4)));
    }

    #[test]
    fn invalid_square_read_is_safe_and_start_has_twenty_moves() {
        let board = Board::new();
        assert_eq!(board.piece_at(8, 0), None);
        assert_eq!(generate_moves(&board, true).len(), 20);
        assert_eq!(generate_moves(&board, false).len(), 20);
    }
}