#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    King,
    Queen,
    Rook,
    Bishop,
    Knight,
    Pawn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

impl Piece {
    pub const fn new(color: Color, kind: PieceKind) -> Self {
        Self { color, kind }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    squares: [[Option<Piece>; 8]; 8],
    white_to_move: bool,
    castling: [bool; 4], // White K/Q, Black K/Q
    en_passant: Option<(usize, usize)>,
}

impl Board {
    pub fn new() -> Self {
        let mut board = Self {
            squares: [[None; 8]; 8],
            white_to_move: true,
            castling: [true; 4],
            en_passant: None,
        };

        let back_rank = [
            PieceKind::Rook,
            PieceKind::Knight,
            PieceKind::Bishop,
            PieceKind::Queen,
            PieceKind::King,
            PieceKind::Bishop,
            PieceKind::Knight,
            PieceKind::Rook,
        ];

        for file in 0..8 {
            board.squares[0][file] = Some(Piece::new(Color::White, back_rank[file]));
            board.squares[1][file] = Some(Piece::new(Color::White, PieceKind::Pawn));
            board.squares[6][file] = Some(Piece::new(Color::Black, PieceKind::Pawn));
            board.squares[7][file] = Some(Piece::new(Color::Black, back_rank[file]));
        }

        board
    }

    pub fn piece_at(&self, rank: usize, file: usize) -> Option<Piece> {
        if rank < 8 && file < 8 {
            self.squares[rank][file]
        } else {
            None
        }
    }

    fn king_position(&self, white: bool) -> Option<(usize, usize)> {
        let color = color_of(white);
        for rank in 0..8 {
            for file in 0..8 {
                if self.squares[rank][file]
                    == Some(Piece::new(color, PieceKind::King))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub from_rank: usize,
    pub from_file: usize,
    pub to_rank: usize,
    pub to_file: usize,
    pub promotion: Option<PieceKind>,
}

impl Move {
    pub const fn new(
        from_rank: usize,
        from_file: usize,
        to_rank: usize,
        to_file: usize,
    ) -> Self {
        Self {
            from_rank,
            from_file,
            to_rank,
            to_file,
            promotion: None,
        }
    }

    pub const fn with_promotion(
        from_rank: usize,
        from_file: usize,
        to_rank: usize,
        to_file: usize,
        promotion: PieceKind,
    ) -> Self {
        Self {
            from_rank,
            from_file,
            to_rank,
            to_file,
            promotion: Some(promotion),
        }
    }
}

fn color_of(white: bool) -> Color {
    if white { Color::White } else { Color::Black }
}

fn inside(rank: i32, file: i32) -> bool {
    (0..8).contains(&rank) && (0..8).contains(&file)
}

fn offset_square(rank: usize, file: usize, dr: i32, df: i32) -> Option<(usize, usize)> {
    let r = rank as i32 + dr;
    let f = file as i32 + df;
    inside(r, f).then_some((r as usize, f as usize))
}

fn attacked_by(board: &Board, rank: usize, file: usize, by_white: bool) -> bool {
    let attacker = color_of(by_white);

    // Pawn attacks.
    let pawn_source_dr = if by_white { -1 } else { 1 };
    for df in [-1, 1] {
        if let Some((r, f)) = offset_square(rank, file, pawn_source_dr, df) {
            if board.squares[r][f] == Some(Piece::new(attacker, PieceKind::Pawn)) {
                return true;
            }
        }
    }

    // Knight attacks.
    for (dr, df) in [
        (-2, -1), (-2, 1), (-1, -2), (-1, 2),
        (1, -2), (1, 2), (2, -1), (2, 1),
    ] {
        if let Some((r, f)) = offset_square(rank, file, dr, df) {
            if board.squares[r][f] == Some(Piece::new(attacker, PieceKind::Knight)) {
                return true;
            }
        }
    }

    // King attacks.
    for dr in -1..=1 {
        for df in -1..=1 {
            if dr == 0 && df == 0 {
                continue;
            }
            if let Some((r, f)) = offset_square(rank, file, dr, df) {
                if board.squares[r][f] == Some(Piece::new(attacker, PieceKind::King)) {
                    return true;
                }
            }
        }
    }

    // Sliding attacks.
    for (dr, df) in [
        (-1, 0), (1, 0), (0, -1), (0, 1),
        (-1, -1), (-1, 1), (1, -1), (1, 1),
    ] {
        let mut current = offset_square(rank, file, dr, df);
        let mut distance = 1;
        while let Some((r, f)) = current {
            if let Some(piece) = board.squares[r][f] {
                if piece.color == attacker {
                    let diagonal = dr != 0 && df != 0;
                    let attacks = match piece.kind {
                        PieceKind::Queen => true,
                        PieceKind::Rook => !diagonal,
                        PieceKind::Bishop => diagonal,
                        _ => false,
                    };
                    if attacks {
                        return true;
                    }
                }
                break;
            }
            distance += 1;
            current = offset_square(
                rank,
                file,
                dr * distance,
                df * distance,
            );
        }
    }

    false
}

pub fn is_check(board: &Board, white: bool) -> bool {
    board
        .king_position(white)
        .map(|(rank, file)| attacked_by(board, rank, file, !white))
        .unwrap_or(false)
}

fn push_move(moves: &mut Vec<Move>, from: (usize, usize), to: (usize, usize)) {
    moves.push(Move::new(from.0, from.1, to.0, to.1));
}

fn generate_pseudo_moves(board: &Board, white: bool) -> Vec<Move> {
    let mut moves = Vec::new();
    let own_color = color_of(white);

    for rank in 0..8 {
        for file in 0..8 {
            let Some(piece) = board.squares[rank][file] else {
                continue;
            };
            if piece.color != own_color {
                continue;
            }

            match piece.kind {
                PieceKind::Pawn => {
                    let direction = if white { 1 } else { -1 };
                    let start_rank = if white { 1 } else { 6 };
                    let promotion_rank = if white { 7 } else { 0 };

                    if let Some((r, f)) = offset_square(rank, file, direction, 0) {
                        if board.squares[r][f].is_none() {
                            if r == promotion_rank {
                                for kind in [
                                    PieceKind::Queen,
                                    PieceKind::Rook,
                                    PieceKind::Bishop,
                                    PieceKind::Knight,
                                ] {
                                    moves.push(Move::with_promotion(rank, file, r, f, kind));
                                }
                            } else {
                                push_move(&mut moves, (rank, file), (r, f));
                            }

                            if rank == start_rank {
                                if let Some((r2, f2)) =
                                    offset_square(rank, file, direction * 2, 0)
                                {
                                    if board.squares[r2][f2].is_none() {
                                        push_move(&mut moves, (rank, file), (r2, f2));
                                    }
                                }
                            }
                        }
                    }

                    for df in [-1, 1] {
                        if let Some((r, f)) = offset_square(rank, file, direction, df) {
                            let target = board.squares[r][f];
                            if target.is_some_and(|p| p.color != own_color) {
                                if r == promotion_rank {
                                    for kind in [
                                        PieceKind::Queen,
                                        PieceKind::Rook,
                                        PieceKind::Bishop,
                                        PieceKind::Knight,
                                    ] {
                                        moves.push(Move::with_promotion(rank, file, r, f, kind));
                                    }
                                } else {
                                    push_move(&mut moves, (rank, file), (r, f));
                                }
                            } else if board.en_passant == Some((r, f)) {
                                push_move(&mut moves, (rank, file), (r, f));
                            }
                        }
                    }
                }
                PieceKind::Knight => {
                    for (dr, df) in [
                        (-2, -1), (-2, 1), (-1, -2), (-1, 2),
                        (1, -2), (1, 2), (2, -1), (2, 1),
                    ] {
                        if let Some((r, f)) = offset_square(rank, file, dr, df) {
                            if board.squares[r][f].is_none_or(|p| p.color != own_color) {
                                push_move(&mut moves, (rank, file), (r, f));
                            }
                        }
                    }
                }
                PieceKind::King => {
                    for dr in -1..=1 {
                        for df in -1..=1 {
                            if dr == 0 && df == 0 {
                                continue;
                            }
                            if let Some((r, f)) = offset_square(rank, file, dr, df) {
                                if board.squares[r][f].is_none_or(|p| p.color != own_color) {
                                    push_move(&mut moves, (rank, file), (r, f));
                                }
                            }
                        }
                    }

                    let home_rank = if white { 0 } else { 7 };
                    let king_right = if white { 0 } else { 2 };
                    let queen_right = king_right + 1;
                    if rank == home_rank && file == 4
                        && board.squares[home_rank][4]
                            == Some(Piece::new(own_color, PieceKind::King))
                        && !is_check(board, white)
                    {
                        // King-side castling.
                        if board.castling[king_right]
                            && board.squares[home_rank][7]
                                == Some(Piece::new(own_color, PieceKind::Rook))
                            && board.squares[home_rank][5].is_none()
                            && board.squares[home_rank][6].is_none()
                            && !attacked_by(board, home_rank, 5, !white)
                            && !attacked_by(board, home_rank, 6, !white)
                        {
                            push_move(&mut moves, (home_rank, 4), (home_rank, 6));
                        }

                        // Queen-side castling.
                        if board.castling[queen_right]
                            && board.squares[home_rank][0]
                                == Some(Piece::new(own_color, PieceKind::Rook))
                            && board.squares[home_rank][1].is_none()
                            && board.squares[home_rank][2].is_none()
                            && board.squares[home_rank][3].is_none()
                            && !attacked_by(board, home_rank, 3, !white)
                            && !attacked_by(board, home_rank, 2, !white)
                        {
                            push_move(&mut moves, (home_rank, 4), (home_rank, 2));
                        }
                    }
                }
                PieceKind::Rook | PieceKind::Bishop | PieceKind::Queen => {
                    let directions: &[(i32, i32)] = match piece.kind {
                        PieceKind::Rook => &[(-1, 0), (1, 0), (0, -1), (0, 1)],
                        PieceKind::Bishop => &[(-1, -1), (-1, 1), (1, -1), (1, 1)],
                        PieceKind::Queen => &[
                            (-1, 0), (1, 0), (0, -1), (0, 1),
                            (-1, -1), (-1, 1), (1, -1), (1, 1),
                        ],
                        _ => unreachable!(),
                    };

                    for &(dr, df) in directions {
                        let mut step = 1;
                        while let Some((r, f)) =
                            offset_square(rank, file, dr * step, df * step)
                        {
                            match board.squares[r][f] {
                                None => push_move(&mut moves, (rank, file), (r, f)),
                                Some(target) if target.color != own_color => {
                                    push_move(&mut moves, (rank, file), (r, f));
                                    break;
                                }
                                Some(_) => break,
                            }
                            step += 1;
                        }
                    }
                }
            }
        }
    }

    moves
}

pub fn generate_moves(board: &Board, white: bool) -> Vec<Move> {
    generate_pseudo_moves(board, white)
        .into_iter()
        .filter(|mv| {
            let next = apply_move(board, mv);
            !is_check(&next, white)
        })
        .collect()
}

pub fn apply_move(board: &Board, mv: &Move) -> Board {
    let mut next = board.clone();

    if mv.from_rank >= 8 || mv.from_file >= 8 || mv.to_rank >= 8 || mv.to_file >= 8 {
        return next;
    }

    let Some(mut piece) = next.squares[mv.from_rank][mv.from_file] else {
        return next;
    };

    let from_rank = mv.from_rank;
    let from_file = mv.from_file;
    let to_rank = mv.to_rank;
    let to_file = mv.to_file;
    let color = piece.color;
    let white = color == Color::White;

    // Clear en-passant target unless this move creates another one.
    next.en_passant = None;

    // En-passant capture.
    if piece.kind == PieceKind::Pawn
        && from_file != to_file
        && next.squares[to_rank][to_file].is_none()
    {
        let captured_rank = if white {
            to_rank.saturating_sub(1)
        } else {
            to_rank.saturating_add(1)
        };
        if captured_rank < 8
            && next.squares[captured_rank][to_file]
                == Some(Piece::new(
                    if white { Color::Black } else { Color::White },
                    PieceKind::Pawn,
                ))
        {
            next.squares[captured_rank][to_file] = None;
        }
    }

    // Update castling rights when a king or rook moves.
    if piece.kind == PieceKind::King {
        let base = if white { 0 } else { 2 };
        next.castling[base] = false;
        next.castling[base + 1] = false;
    }
    if piece.kind == PieceKind::Rook {
        if from_rank == 0 && from_file == 0 {
            next.castling[1] = false;
        } else if from_rank == 0 && from_file == 7 {
            next.castling[0] = false;
        } else if from_rank == 7 && from_file == 0 {
            next.castling[3] = false;
        } else if from_rank == 7 && from_file == 7 {
            next.castling[2] = false;
        }
    }

    // A rook captured on its original square loses that side's castling right.
    if next.squares[to_rank][to_file].is_some_and(|p| p.kind == PieceKind::Rook) {
        if to_rank == 0 && to_file == 0 {
            next.castling[1] = false;
        } else if to_rank == 0 && to_file == 7 {
            next.castling[0] = false;
        } else if to_rank == 7 && to_file == 0 {
            next.castling[3] = false;
        } else if to_rank == 7 && to_file == 7 {
            next.castling[2] = false;
        }
    }

    // Set en-passant target after a two-square pawn push.
    if piece.kind == PieceKind::Pawn && from_rank.abs_diff(to_rank) == 2 {
        next.en_passant = Some(((from_rank + to_rank) / 2, from_file));
    }

    // Castling moves the rook as well.
    if piece.kind == PieceKind::King && from_file.abs_diff(to_file) == 2 {
        if to_file == 6 {
            next.squares[from_rank][5] = next.squares[from_rank][7];
            next.squares[from_rank][7] = None;
        } else if to_file == 2 {
            next.squares[from_rank][3] = next.squares[from_rank][0];
            next.squares[from_rank][0] = None;
        }
    }

    next.squares[from_rank][from_file] = None;

    if piece.kind == PieceKind::Pawn && (to_rank == 0 || to_rank == 7) {
        piece.kind = mv.promotion.unwrap_or(PieceKind::Queen);
    }
    next.squares[to_rank][to_file] = Some(piece);
    next.white_to_move = !next.white_to_move;
    next
}

pub fn is_checkmate(board: &Board, white: bool) -> bool {
    is_check(board, white) && generate_moves(board, white).is_empty()
}

pub fn is_stalemate(board: &Board, white: bool) -> bool {
    !is_check(board, white) && generate_moves(board, white).is_empty()
}

pub fn evaluate(board: &Board) -> i32 {
    let mut score = 0;
    for rank in 0..8 {
        for file in 0..8 {
            if let Some(piece) = board.squares[rank][file] {
                let value = match piece.kind {
                    PieceKind::Pawn => 100,
                    PieceKind::Knight => 320,
                    PieceKind::Bishop => 330,
                    PieceKind::Rook => 500,
                    PieceKind::Queen => 900,
                    PieceKind::King => 0,
                };
                score += if piece.color == Color::White { value } else { -value };
            }
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_board_setup() {
        let board = Board::new();
        assert_eq!(
            board.piece_at(0, 4),
            Some(Piece::new(Color::White, PieceKind::King))
        );
        assert_eq!(
            board.piece_at(7, 3),
            Some(Piece::new(Color::Black, PieceKind::Queen))
        );
        assert_eq!(
            board.piece_at(1, 0),
            Some(Piece::new(Color::White, PieceKind::Pawn))
        );
        for rank in 2..6 {
            for file in 0..8 {
                assert!(board.piece_at(rank, file).is_none());
            }
        }
    }

    #[test]
    fn pawn_moves_from_start() {
        let board = Board::new();
        let moves = generate_moves(&board, true);
        assert!(moves.contains(&Move::new(1, 4, 2, 4)));
        assert!(moves.contains(&Move::new(1, 4, 3, 4)));
    }

    #[test]
    fn knight_moves_from_start() {
        let board = Board::new();
        let moves = generate_moves(&board, true);
        assert!(moves.contains(&Move::new(0, 1, 2, 0)));
        assert!(moves.contains(&Move::new(0, 1, 2, 2)));
    }

    #[test]
    fn check_detection() {
        let mut board = Board::new();
        board.squares[1][4] = None;
        board.squares[6][4] = None;
        board.squares[1][4] = Some(Piece::new(Color::Black, PieceKind::Rook));
        assert!(is_check(&board, true));
    }

    #[test]
    fn move_application_updates_board() {
        let board = Board::new();
        let next = apply_move(&board, &Move::new(1, 4, 3, 4));
        assert!(next.piece_at(1, 4).is_none());
        assert_eq!(
            next.piece_at(3, 4),
            Some(Piece::new(Color::White, PieceKind::Pawn))
        );
    }

    #[test]
    fn both_sides_have_twenty_moves_from_start() {
        let board = Board::new();
        assert_eq!(generate_moves(&board, true).len(), 20);
        assert_eq!(generate_moves(&board, false).len(), 20);
    }
}