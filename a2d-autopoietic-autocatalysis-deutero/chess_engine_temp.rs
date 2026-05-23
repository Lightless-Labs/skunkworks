#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}

impl Color {
    pub fn opposite(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChessMove {
    pub from: (usize, usize),
    pub to: (usize, usize),
    pub promotion: Option<PieceKind>,
}

impl ChessMove {
    pub fn new(from: (usize, usize), to: (usize, usize)) -> Self {
        Self {
            from,
            to,
            promotion: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    pub squares: [[Option<Piece>; 8]; 8],
    pub side_to_move: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChessError {
    NoPieceAtSource,
    WrongSideToMove,
    IllegalMove,
}

impl Board {
    pub fn empty() -> Self {
        Self {
            squares: [[None; 8]; 8],
            side_to_move: Color::White,
        }
    }

    pub fn initial() -> Self {
        let mut board = Self::empty();
        board.side_to_move = Color::White;

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
            board.squares[0][file] = Some(Piece {
                color: Color::White,
                kind: back_rank[file],
            });
            board.squares[1][file] = Some(Piece {
                color: Color::White,
                kind: PieceKind::Pawn,
            });
            board.squares[6][file] = Some(Piece {
                color: Color::Black,
                kind: PieceKind::Pawn,
            });
            board.squares[7][file] = Some(Piece {
                color: Color::Black,
                kind: back_rank[file],
            });
        }

        board
    }

    pub fn get(&self, row: usize, col: usize) -> Option<Piece> {
        self.squares[row][col]
    }

    pub fn set(&mut self, row: usize, col: usize, piece: Option<Piece>) {
        self.squares[row][col] = piece;
    }
}

fn in_bounds(row: isize, col: isize) -> bool {
    (0..8).contains(&row) && (0..8).contains(&col)
}

fn find_king(board: &Board, color: Color) -> Option<(usize, usize)> {
    for row in 0..8 {
        for col in 0..8 {
            if let Some(piece) = board.squares[row][col] {
                if piece.color == color && piece.kind == PieceKind::King {
                    return Some((row, col));
                }
            }
        }
    }
    None
}

fn push_if_valid_target(
    board: &Board,
    piece: Piece,
    from: (usize, usize),
    row: isize,
    col: isize,
    moves: &mut Vec<ChessMove>,
) {
    if !in_bounds(row, col) {
        return;
    }

    let to = (row as usize, col as usize);
    match board.get(to.0, to.1) {
        None => moves.push(ChessMove::new(from, to)),
        Some(target) if target.color != piece.color => moves.push(ChessMove::new(from, to)),
        _ => {}
    }
}

fn generate_pseudo_legal_moves_for_piece(
    board: &Board,
    from: (usize, usize),
    piece: Piece,
) -> Vec<ChessMove> {
    let mut moves = Vec::new();
    let row = from.0 as isize;
    let col = from.1 as isize;

    match piece.kind {
        PieceKind::Pawn => {
            let direction: isize = if piece.color == Color::White { 1 } else { -1 };
            let start_row: usize = if piece.color == Color::White { 1 } else { 6 };
            let promotion_row: usize = if piece.color == Color::White { 7 } else { 0 };

            let one_step = row + direction;
            if in_bounds(one_step, col) && board.get(one_step as usize, col as usize).is_none() {
                let mut mv = ChessMove::new(from, (one_step as usize, col as usize));
                if one_step as usize == promotion_row {
                    mv.promotion = Some(PieceKind::Queen);
                }
                moves.push(mv);

                let two_step = row + direction * 2;
                if from.0 == start_row
                    && in_bounds(two_step, col)
                    && board.get(two_step as usize, col as usize).is_none()
                {
                    moves.push(ChessMove::new(from, (two_step as usize, col as usize)));
                }
            }

            for dc in [-1, 1] {
                let target_row = row + direction;
                let target_col = col + dc;
                if !in_bounds(target_row, target_col) {
                    continue;
                }
                let target = (target_row as usize, target_col as usize);
                if let Some(other) = board.get(target.0, target.1) {
                    if other.color != piece.color {
                        let mut mv = ChessMove::new(from, target);
                        if target.0 == promotion_row {
                            mv.promotion = Some(PieceKind::Queen);
                        }
                        moves.push(mv);
                    }
                }
            }
        }
        PieceKind::Knight => {
            let deltas = [
                (-2, -1),
                (-2, 1),
                (-1, -2),
                (-1, 2),
                (1, -2),
                (1, 2),
                (2, -1),
                (2, 1),
            ];
            for (dr, dc) in deltas {
                push_if_valid_target(board, piece, from, row + dr, col + dc, &mut moves);
            }
        }
        PieceKind::Bishop | PieceKind::Rook | PieceKind::Queen => {
            let bishop_dirs: &[(isize, isize)] = &[(-1, -1), (-1, 1), (1, -1), (1, 1)];
            let rook_dirs: &[(isize, isize)] = &[(-1, 0), (1, 0), (0, -1), (0, 1)];
            let queen_dirs: &[(isize, isize)] = &[
                (-1, -1),
                (-1, 1),
                (1, -1),
                (1, 1),
                (-1, 0),
                (1, 0),
                (0, -1),
                (0, 1),
            ];

            let directions = match piece.kind {
                PieceKind::Bishop => bishop_dirs,
                PieceKind::Rook => rook_dirs,
                PieceKind::Queen => queen_dirs,
                _ => unreachable!(),
            };

            for &(dr, dc) in directions {
                let mut r = row + dr;
                let mut c = col + dc;
                while in_bounds(r, c) {
                    let to = (r as usize, c as usize);
                    match board.get(to.0, to.1) {
                        None => moves.push(ChessMove::new(from, to)),
                        Some(target) if target.color != piece.color => {
                            moves.push(ChessMove::new(from, to));
                            break;
                        }
                        Some(_) => break,
                    }
                    r += dr;
                    c += dc;
                }
            }
        }
        PieceKind::King => {
            for dr in -1..=1 {
                for dc in -1..=1 {
                    if dr == 0 && dc == 0 {
                        continue;
                    }
                    push_if_valid_target(board, piece, from, row + dr, col + dc, &mut moves);
                }
            }
        }
    }

    moves
}

fn apply_move_unchecked(board: &Board, mv: ChessMove) -> Board {
    let mut next = board.clone();
    let mut piece = next.squares[mv.from.0][mv.from.1].expect("piece must exist");
    next.squares[mv.from.0][mv.from.1] = None;

    if piece.kind == PieceKind::Pawn {
        let promotion_row = if piece.color == Color::White { 7 } else { 0 };
        if mv.to.0 == promotion_row {
            piece.kind = mv.promotion.unwrap_or(PieceKind::Queen);
        }
    }

    next.squares[mv.to.0][mv.to.1] = Some(piece);
    next.side_to_move = board.side_to_move.opposite();
    next
}

fn square_attacked_by(board: &Board, target: (usize, usize), attacker_color: Color) -> bool {
    for row in 0..8 {
        for col in 0..8 {
            if let Some(piece) = board.squares[row][col] {
                if piece.color != attacker_color {
                    continue;
                }

                let from = (row, col);
                if piece.kind == PieceKind::Pawn {
                    let direction: isize = if piece.color == Color::White { 1 } else { -1 };
                    for dc in [-1, 1] {
                        let attack_row = row as isize + direction;
                        let attack_col = col as isize + dc;
                        if in_bounds(attack_row, attack_col)
                            && (attack_row as usize, attack_col as usize) == target
                        {
                            return true;
                        }
                    }
                    continue;
                }

                for mv in generate_pseudo_legal_moves_for_piece(board, from, piece) {
                    if mv.to == target {
                        return true;
                    }
                }
            }
        }
    }
    false
}

pub fn is_in_check(board: &Board, color: Color) -> bool {
    let king_square = match find_king(board, color) {
        Some(square) => square,
        None => return false,
    };
    square_attacked_by(board, king_square, color.opposite())
}

pub fn generate_legal_moves(board: &Board) -> Vec<ChessMove> {
    let mut legal_moves = Vec::new();

    for row in 0..8 {
        for col in 0..8 {
            let Some(piece) = board.squares[row][col] else {
                continue;
            };
            if piece.color != board.side_to_move {
                continue;
            }

            for mv in generate_pseudo_legal_moves_for_piece(board, (row, col), piece) {
                let next = apply_move_unchecked(board, mv);
                if !is_in_check(&next, piece.color) {
                    legal_moves.push(mv);
                }
            }
        }
    }

    legal_moves
}

pub fn apply_move(board: &Board, mv: ChessMove) -> Result<Board, ChessError> {
    let piece = board.squares[mv.from.0][mv.from.1].ok_or(ChessError::NoPieceAtSource)?;
    if piece.color != board.side_to_move {
        return Err(ChessError::WrongSideToMove);
    }

    let legal_moves = generate_legal_moves(board);
    if !legal_moves.contains(&mv) {
        return Err(ChessError::IllegalMove);
    }

    Ok(apply_move_unchecked(board, mv))
}

pub fn evaluate(board: &Board) -> i32 {
    fn piece_value(kind: PieceKind) -> i32 {
        match kind {
            PieceKind::Pawn => 1,
            PieceKind::Knight | PieceKind::Bishop => 3,
            PieceKind::Rook => 5,
            PieceKind::Queen => 9,
            PieceKind::King => 0,
        }
    }

    let mut score = 0;
    for row in 0..8 {
        for col in 0..8 {
            if let Some(piece) = board.squares[row][col] {
                let value = piece_value(piece.kind);
                if piece.color == Color::White {
                    score += value;
                } else {
                    score -= value;
                }
            }
        }
    }
    score
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_board_setup() {
        let board = Board::initial();
        assert_eq!(board.side_to_move, Color::White);
        assert_eq!(
            board.get(0, 4),
            Some(Piece {
                color: Color::White,
                kind: PieceKind::King,
            })
        );
        assert_eq!(
            board.get(7, 4),
            Some(Piece {
                color: Color::Black,
                kind: PieceKind::King,
            })
        );
        assert_eq!(
            board.get(1, 0),
            Some(Piece {
                color: Color::White,
                kind: PieceKind::Pawn,
            })
        );
        assert_eq!(
            board.get(6, 7),
            Some(Piece {
                color: Color::Black,
                kind: PieceKind::Pawn,
            })
        );
    }

    #[test]
    fn pawn_moves_from_initial_position() {
        let board = Board::initial();
        let moves = generate_legal_moves(&board);
        assert!(moves.contains(&ChessMove::new((1, 4), (2, 4))));
        assert!(moves.contains(&ChessMove::new((1, 4), (3, 4))));
    }

    #[test]
    fn knight_moves_from_initial_position() {
        let board = Board::initial();
        let moves = generate_legal_moves(&board);
        assert!(moves.contains(&ChessMove::new((0, 1), (2, 0))));
        assert!(moves.contains(&ChessMove::new((0, 1), (2, 2))));
        assert!(moves.contains(&ChessMove::new((0, 6), (2, 5))));
        assert!(moves.contains(&ChessMove::new((0, 6), (2, 7))));
    }

    #[test]
    fn check_detection_works() {
        let mut board = Board::empty();
        board.set(
            0,
            4,
            Some(Piece {
                color: Color::White,
                kind: PieceKind::King,
            }),
        );
        board.set(
            7,
            4,
            Some(Piece {
                color: Color::Black,
                kind: PieceKind::King,
            }),
        );
        board.set(
            3,
            4,
            Some(Piece {
                color: Color::Black,
                kind: PieceKind::Rook,
            }),
        );
        assert!(is_in_check(&board, Color::White));
        assert!(!is_in_check(&board, Color::Black));
    }

    #[test]
    fn move_application_updates_board_state() {
        let board = Board::initial();
        let next = apply_move(&board, ChessMove::new((1, 4), (3, 4))).unwrap();
        assert_eq!(next.get(1, 4), None);
        assert_eq!(
            next.get(3, 4),
            Some(Piece {
                color: Color::White,
                kind: PieceKind::Pawn,
            })
        );
        assert_eq!(next.side_to_move, Color::Black);
    }

    #[test]
    fn illegal_moves_that_leave_king_in_check_are_filtered_out() {
        let mut board = Board::empty();
        board.side_to_move = Color::White;
        board.set(
            0,
            4,
            Some(Piece {
                color: Color::White,
                kind: PieceKind::King,
            }),
        );
        board.set(
            1,
            4,
            Some(Piece {
                color: Color::White,
                kind: PieceKind::Rook,
            }),
        );
        board.set(
            7,
            4,
            Some(Piece {
                color: Color::Black,
                kind: PieceKind::Rook,
            }),
        );
        board.set(
            7,
            7,
            Some(Piece {
                color: Color::Black,
                kind: PieceKind::King,
            }),
        );

        let moves = generate_legal_moves(&board);
        assert!(!moves.contains(&ChessMove::new((1, 4), (1, 5))));
        assert!(moves.contains(&ChessMove::new((1, 4), (2, 4))));
    }

    #[test]
    fn evaluation_is_material_based() {
        let mut board = Board::empty();
        board.set(
            0,
            0,
            Some(Piece {
                color: Color::White,
                kind: PieceKind::Queen,
            }),
        );
        board.set(
            7,
            7,
            Some(Piece {
                color: Color::Black,
                kind: PieceKind::Rook,
            }),
        );
        assert_eq!(evaluate(&board), 4);
    }
}
