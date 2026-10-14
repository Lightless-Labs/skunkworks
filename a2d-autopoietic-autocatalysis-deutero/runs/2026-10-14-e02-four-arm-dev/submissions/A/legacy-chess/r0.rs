#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    squares: [[Option<Piece>; 8]; 8],
    white_can_castle_kingside: bool,
    white_can_castle_queenside: bool,
    black_can_castle_kingside: bool,
    black_can_castle_queenside: bool,
    en_passant_target: Option<(usize, usize)>,
}

impl Board {
    pub fn new() -> Self {
        let mut squares = [[None; 8]; 8];

        for file in 0..8 {
            squares[1][file] = Some(Piece::new(Color::White, PieceKind::Pawn));
            squares[6][file] = Some(Piece::new(Color::Black, PieceKind::Pawn));
        }

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

        for (file, kind) in back_rank.into_iter().enumerate() {
            squares[0][file] = Some(Piece::new(Color::White, kind));
            squares[7][file] = Some(Piece::new(Color::Black, kind));
        }

        Self {
            squares,
            white_can_castle_kingside: true,
            white_can_castle_queenside: true,
            black_can_castle_kingside: true,
            black_can_castle_queenside: true,
            en_passant_target: None,
        }
    }

    pub fn piece_at(&self, rank: usize, file: usize) -> Option<Piece> {
        if rank < 8 && file < 8 {
            self.squares[rank][file]
        } else {
            None
        }
    }

    fn king_position(&self, color: Color) -> Option<(usize, usize)> {
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

fn color_from_white(white: bool) -> Color {
    if white { Color::White } else { Color::Black }
}

fn inside(rank: isize, file: isize) -> bool {
    (0..8).contains(&rank) && (0..8).contains(&file)
}

fn square_attacked(board: &Board, rank: usize, file: usize, by: Color) -> bool {
    let pawn_source_rank = match by {
        Color::White => rank as isize - 1,
        Color::Black => rank as isize + 1,
    };

    for df in [-1isize, 1] {
        let source_file = file as isize + df;
        if inside(pawn_source_rank, source_file)
            && board.squares[pawn_source_rank as usize][source_file as usize]
                == Some(Piece::new(by, PieceKind::Pawn))
        {
            return true;
        }
    }

    for (dr, df) in [
        (-2isize, -1isize),
        (-2, 1),
        (-1, -2),
        (-1, 2),
        (1, -2),
        (1, 2),
        (2, -1),
        (2, 1),
    ] {
        let r = rank as isize + dr;
        let f = file as isize + df;
        if inside(r, f)
            && board.squares[r as usize][f as usize]
                == Some(Piece::new(by, PieceKind::Knight))
        {
            return true;
        }
    }

    for dr in -1isize..=1 {
        for df in -1isize..=1 {
            if dr == 0 && df == 0 {
                continue;
            }
            let r = rank as isize + dr;
            let f = file as isize + df;
            if inside(r, f)
                && board.squares[r as usize][f as usize]
                    == Some(Piece::new(by, PieceKind::King))
            {
                return true;
            }
        }
    }

    let directions = [
        (-1isize, 0isize),
        (1, 0),
        (0, -1),
        (0, 1),
        (-1, -1),
        (-1, 1),
        (1, -1),
        (1, 1),
    ];

    for (dr, df) in directions {
        let mut r = rank as isize + dr;
        let mut f = file as isize + df;
        let mut distance = 1;
        while inside(r, f) {
            if let Some(piece) = board.squares[r as usize][f as usize] {
                if piece.color == by {
                    let is_diagonal = dr != 0 && df != 0;
                    let attacks = match piece.kind {
                        PieceKind::Queen => true,
                        PieceKind::Rook => !is_diagonal,
                        PieceKind::Bishop => is_diagonal,
                        PieceKind::King => distance == 1,
                        _ => false,
                    };
                    if attacks {
                        return true;
                    }
                }
                break;
            }
            r += dr;
            f += df;
            distance += 1;
        }
    }

    false
}

pub fn is_check(board: &Board, white: bool) -> bool {
    let color = color_from_white(white);
    let Some((rank, file)) = board.king_position(color) else {
        return false;
    };
    square_attacked(board, rank, file, color_from_white(!white))
}

fn push_move(moves: &mut Vec<Move>, from: (usize, usize), to: (usize, usize)) {
    moves.push(Move::new(from.0, from.1, to.0, to.1));
}

fn pseudo_legal_moves(board: &Board, white: bool) -> Vec<Move> {
    let color = color_from_white(white);
    let opponent = color_from_white(!white);
    let mut moves = Vec::new();

    for rank in 0..8 {
        for file in 0..8 {
            let Some(piece) = board.squares[rank][file] else {
                continue;
            };
            if piece.color != color {
                continue;
            }

            match piece.kind {
                PieceKind::Pawn => {
                    let step: isize = if white { 1 } else { -1 };
                    let start_rank = if white { 1 } else { 6 };
                    let next_rank = rank as isize + step;

                    if inside(next_rank, file as isize)
                        && board.squares[next_rank as usize][file].is_none()
                    {
                        push_move(
                            &mut moves,
                            (rank, file),
                            (next_rank as usize, file),
                        );

                        let double_rank = rank as isize + 2 * step;
                        if rank == start_rank
                            && inside(double_rank, file as isize)
                            && board.squares[double_rank as usize][file].is_none()
                        {
                            push_move(
                                &mut moves,
                                (rank, file),
                                (double_rank as usize, file),
                            );
                        }
                    }

                    for df in [-1isize, 1] {
                        let target_file = file as isize + df;
                        if !inside(next_rank, target_file) {
                            continue;
                        }
                        let target = board.squares[next_rank as usize][target_file as usize];
                        if target.is_some_and(|p| p.color == opponent)
                            || board.en_passant_target
                                == Some((next_rank as usize, target_file as usize))
                        {
                            push_move(
                                &mut moves,
                                (rank, file),
                                (next_rank as usize, target_file as usize),
                            );
                        }
                    }
                }
                PieceKind::Knight => {
                    for (dr, df) in [
                        (-2isize, -1isize),
                        (-2, 1),
                        (-1, -2),
                        (-1, 2),
                        (1, -2),
                        (1, 2),
                        (2, -1),
                        (2, 1),
                    ] {
                        let r = rank as isize + dr;
                        let f = file as isize + df;
                        if inside(r, f)
                            && board.squares[r as usize][f as usize]
                                .is_none_or(|p| p.color != color)
                        {
                            push_move(
                                &mut moves,
                                (rank, file),
                                (r as usize, f as usize),
                            );
                        }
                    }
                }
                PieceKind::Bishop | PieceKind::Rook | PieceKind::Queen => {
                    let directions: &[(isize, isize)] = match piece.kind {
                        PieceKind::Bishop => &[
                            (-1, -1),
                            (-1, 1),
                            (1, -1),
                            (1, 1),
                        ],
                        PieceKind::Rook => &[(-1, 0), (1, 0), (0, -1), (0, 1)],
                        PieceKind::Queen => &[
                            (-1, 0),
                            (1, 0),
                            (0, -1),
                            (0, 1),
                            (-1, -1),
                            (-1, 1),
                            (1, -1),
                            (1, 1),
                        ],
                        _ => unreachable!(),
                    };

                    for &(dr, df) in directions {
                        let mut r = rank as isize + dr;
                        let mut f = file as isize + df;
                        while inside(r, f) {
                            match board.squares[r as usize][f as usize] {
                                Some(target) if target.color == color => break,
                                Some(_) => {
                                    push_move(
                                        &mut moves,
                                        (rank, file),
                                        (r as usize, f as usize),
                                    );
                                    break;
                                }
                                None => push_move(
                                    &mut moves,
                                    (rank, file),
                                    (r as usize, f as usize),
                                ),
                            }
                            r += dr;
                            f += df;
                        }
                    }
                }
                PieceKind::King => {
                    for dr in -1isize..=1 {
                        for df in -1isize..=1 {
                            if dr == 0 && df == 0 {
                                continue;
                            }
                            let r = rank as isize + dr;
                            let f = file as isize + df;
                            if inside(r, f)
                                && board.squares[r as usize][f as usize]
                                    .is_none_or(|p| p.color != color)
                            {
                                push_move(
                                    &mut moves,
                                    (rank, file),
                                    (r as usize, f as usize),
                                );
                            }
                        }
                    }

                    let home_rank = if white { 0 } else { 7 };
                    if rank == home_rank
                        && file == 4
                        && !is_check(board, white)
                    {
                        let can_king_side = if white {
                            board.white_can_castle_kingside
                        } else {
                            board.black_can_castle_kingside
                        };
                        if can_king_side
                            && board.squares[home_rank][5].is_none()
                            && board.squares[home_rank][6].is_none()
                            && board.squares[home_rank][7]
                                == Some(Piece::new(color, PieceKind::Rook))
                            && !square_attacked(board, home_rank, 5, opponent)
                            && !square_attacked(board, home_rank, 6, opponent)
                        {
                            push_move(
                                &mut moves,
                                (home_rank, 4),
                                (home_rank, 6),
                            );
                        }

                        let can_queen_side = if white {
                            board.white_can_castle_queenside
                        } else {
                            board.black_can_castle_queenside
                        };
                        if can_queen_side
                            && board.squares[home_rank][1].is_none()
                            && board.squares[home_rank][2].is_none()
                            && board.squares[home_rank][3].is_none()
                            && board.squares[home_rank][0]
                                == Some(Piece::new(color, PieceKind::Rook))
                            && !square_attacked(board, home_rank, 3, opponent)
                            && !square_attacked(board, home_rank, 2, opponent)
                        {
                            push_move(
                                &mut moves,
                                (home_rank, 4),
                                (home_rank, 2),
                            );
                        }
                    }
                }
            }
        }
    }

    moves
}

pub fn apply_move(board: &Board, mv: &Move) -> Board {
    let mut next = board.clone();

    if mv.from_rank >= 8
        || mv.from_file >= 8
        || mv.to_rank >= 8
        || mv.to_file >= 8
    {
        return next;
    }

    let Some(mut moving_piece) = next.squares[mv.from_rank][mv.from_file] else {
        return next;
    };

    let from = (mv.from_rank, mv.from_file);
    let to = (mv.to_rank, mv.to_file);
    let old_en_passant = next.en_passant_target;
    next.en_passant_target = None;

    if moving_piece.kind == PieceKind::Pawn
        && old_en_passant == Some(to)
        && mv.from_file != mv.to_file
        && next.squares[mv.to_rank][mv.to_file].is_none()
    {
        let captured_rank = if moving_piece.color == Color::White {
            mv.to_rank.saturating_sub(1)
        } else {
            (mv.to_rank + 1).min(7)
        };
        next.squares[captured_rank][mv.to_file] = None;
    }

    if moving_piece.kind == PieceKind::King
        && mv.from_file == 4
        && (mv.to_file == 6 || mv.to_file == 2)
        && mv.from_rank == mv.to_rank
    {
        let rook_from = if mv.to_file == 6 { 7 } else { 0 };
        let rook_to = if mv.to_file == 6 { 5 } else { 3 };
        if let Some(rook) = next.squares[mv.from_rank][rook_from] {
            next.squares[mv.from_rank][rook_from] = None;
            next.squares[mv.from_rank][rook_to] = Some(rook);
        }
    }

    if moving_piece.kind == PieceKind::Pawn && (mv.to_rank == 0 || mv.to_rank == 7) {
        moving_piece.kind = mv.promotion.unwrap_or(PieceKind::Queen);
    }

    next.squares[from.0][from.1] = None;
    next.squares[to.0][to.1] = Some(moving_piece);

    if board.squares[from.0][from.1]
        == Some(Piece::new(Color::White, PieceKind::King))
    {
        next.white_can_castle_kingside = false;
        next.white_can_castle_queenside = false;
    }
    if board.squares[from.0][from.1]
        == Some(Piece::new(Color::Black, PieceKind::King))
    {
        next.black_can_castle_kingside = false;
        next.black_can_castle_queenside = false;
    }

    if from == (0, 0) || to == (0, 0) {
        next.white_can_castle_queenside = false;
    }
    if from == (0, 7) || to == (0, 7) {
        next.white_can_castle_kingside = false;
    }
    if from == (7, 0) || to == (7, 0) {
        next.black_can_castle_queenside = false;
    }
    if from == (7, 7) || to == (7, 7) {
        next.black_can_castle_kingside = false;
    }

    if moving_piece.kind == PieceKind::Pawn
        && (mv.from_rank as isize - mv.to_rank as isize).abs() == 2
    {
        next.en_passant_target = Some(((mv.from_rank + mv.to_rank) / 2, mv.from_file));
    }

    next
}

pub fn generate_moves(board: &Board, white: bool) -> Vec<Move> {
    pseudo_legal_moves(board, white)
        .into_iter()
        .filter(|mv| {
            let next = apply_move(board, mv);
            !is_check(&next, white)
        })
        .collect()
}

pub fn is_checkmate(board: &Board, white: bool) -> bool {
    is_check(board, white) && generate_moves(board, white).is_empty()
}

pub fn is_stalemate(board: &Board, white: bool) -> bool {
    !is_check(board, white) && generate_moves(board, white).is_empty()
}

pub fn evaluate_material(board: &Board) -> i32 {
    let mut score = 0;
    for row in board.squares {
        for piece in row.into_iter().flatten() {
            let value = match piece.kind {
                PieceKind::Pawn => 100,
                PieceKind::Knight => 320,
                PieceKind::Bishop => 330,
                PieceKind::Rook => 500,
                PieceKind::Queen => 900,
                PieceKind::King => 0,
            };
            if piece.color == Color::White {
                score += value;
            } else {
                score -= value;
            }
        }
    }
    score
}

pub fn material_count(board: &Board) -> i32 {
    evaluate_material(board)
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
        for rank in 2..6 {
            for file in 0..8 {
                assert_eq!(board.piece_at(rank, file), None);
            }
        }
    }

    #[test]
    fn initial_pawn_moves() {
        let board = Board::new();
        let moves = generate_moves(&board, true);
        assert!(moves.contains(&Move::new(1, 4, 2, 4)));
        assert!(moves.contains(&Move::new(1, 4, 3, 4)));
        assert_eq!(
            moves.iter().filter(|mv| mv.from_rank == 1).count(),
            16
        );
    }

    #[test]
    fn initial_knight_moves() {
        let board = Board::new();
        let moves = generate_moves(&board, true);
        assert!(moves.contains(&Move::new(0, 1, 2, 0)));
        assert!(moves.contains(&Move::new(0, 1, 2, 2)));
        assert_eq!(
            moves.iter().filter(|mv| mv.from_rank == 0 && (mv.from_file == 1 || mv.from_file == 6)).count(),
            4
        );
    }

    #[test]
    fn detects_check() {
        let mut board = Board::new();
        board.squares[1][4] = None;
        board.squares[6][3] = None;
        board.squares[4][4] = Some(Piece::new(Color::White, PieceKind::Queen));
        assert!(is_check(&board, false));
        assert!(!is_check(&board, true));
    }

    #[test]
    fn move_application_updates_board() {
        let board = Board::new();
        let next = apply_move(&board, &Move::new(1, 4, 3, 4));
        assert_eq!(next.piece_at(1, 4), None);
        assert_eq!(
            next.piece_at(3, 4),
            Some(Piece::new(Color::White, PieceKind::Pawn))
        );
    }

    #[test]
    fn has_twenty_moves_at_start() {
        let board = Board::new();
        assert_eq!(generate_moves(&board, true).len(), 20);
        assert_eq!(generate_moves(&board, false).len(), 20);
    }
}