#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceColor {
    White,
    Black,
}

impl PieceColor {
    pub fn opposite(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceType {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub piece_type: PieceType,
    pub color: PieceColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Square {
    pub rank: i8,
    pub file: i8,
}

impl Square {
    pub fn new(rank: i8, file: i8) -> Option<Self> {
        if (0..8).contains(&rank) && (0..8).contains(&file) {
            Some(Self { rank, file })
        } else {
            None
        }
    }

    pub fn to_index(self) -> usize {
        (self.rank as usize) * 8 + self.file as usize
    }

    pub fn offset(self, rank_delta: i8, file_delta: i8) -> Option<Self> {
        Self::new(self.rank + rank_delta, self.file + file_delta)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub from: Square,
    pub to: Square,
    pub promotion: Option<PieceType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub squares: [Option<Piece>; 64],
    pub active_color: PieceColor,
}

impl Board {
    pub fn new_starting_position() -> Self {
        let mut board = Self {
            squares: [None; 64],
            active_color: PieceColor::White,
        };

        for file in 0..8 {
            board.set_piece(
                Square::new(1, file).unwrap(),
                Some(Piece {
                    piece_type: PieceType::Pawn,
                    color: PieceColor::White,
                }),
            );
            board.set_piece(
                Square::new(6, file).unwrap(),
                Some(Piece {
                    piece_type: PieceType::Pawn,
                    color: PieceColor::Black,
                }),
            );
        }

        let back_rank = [
            PieceType::Rook,
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Queen,
            PieceType::King,
            PieceType::Bishop,
            PieceType::Knight,
            PieceType::Rook,
        ];

        for (file, piece_type) in back_rank.into_iter().enumerate() {
            board.set_piece(
                Square::new(0, file as i8).unwrap(),
                Some(Piece {
                    piece_type,
                    color: PieceColor::White,
                }),
            );
            board.set_piece(
                Square::new(7, file as i8).unwrap(),
                Some(Piece {
                    piece_type,
                    color: PieceColor::Black,
                }),
            );
        }

        board
    }

    pub fn empty(active_color: PieceColor) -> Self {
        Self {
            squares: [None; 64],
            active_color,
        }
    }

    pub fn get_piece(&self, square: Square) -> Option<Piece> {
        self.squares[square.to_index()]
    }

    pub fn set_piece(&mut self, square: Square, piece: Option<Piece>) {
        self.squares[square.to_index()] = piece;
    }

    pub fn find_king(&self, color: PieceColor) -> Option<Square> {
        for rank in 0..8 {
            for file in 0..8 {
                let square = Square::new(rank, file).unwrap();
                if let Some(piece) = self.get_piece(square) {
                    if piece.piece_type == PieceType::King && piece.color == color {
                        return Some(square);
                    }
                }
            }
        }
        None
    }

    pub fn generate_legal_moves(&self) -> Vec<Move> {
        let pseudo = self.generate_pseudo_legal_moves_for(self.active_color);
        pseudo
            .into_iter()
            .filter(|mv| {
                let next_board = self.apply_move(mv);
                !next_board.is_king_in_check(self.active_color)
            })
            .collect()
    }

    pub fn apply_move(&self, mv: &Move) -> Board {
        let mut next = self.clone();
        let moving_piece = next
            .get_piece(mv.from)
            .expect("apply_move requires a piece on the from square");

        next.set_piece(mv.from, None);
        let placed_piece = if let Some(promotion) = mv.promotion {
            Piece {
                piece_type: promotion,
                color: moving_piece.color,
            }
        } else {
            moving_piece
        };
        next.set_piece(mv.to, Some(placed_piece));
        next.active_color = self.active_color.opposite();
        next
    }

    pub fn is_king_in_check(&self, king_color: PieceColor) -> bool {
        let Some(king_square) = self.find_king(king_color) else {
            return false;
        };

        self.is_square_attacked(king_square, king_color.opposite())
    }

    pub fn evaluate_board(&self) -> i32 {
        self.squares
            .iter()
            .flatten()
            .map(|piece| {
                let value = match piece.piece_type {
                    PieceType::Pawn => 100,
                    PieceType::Knight => 300,
                    PieceType::Bishop => 320,
                    PieceType::Rook => 500,
                    PieceType::Queen => 900,
                    PieceType::King => 0,
                };
                if piece.color == PieceColor::White {
                    value
                } else {
                    -value
                }
            })
            .sum()
    }

    fn generate_pseudo_legal_moves_for(&self, color: PieceColor) -> Vec<Move> {
        let mut moves = Vec::new();

        for rank in 0..8 {
            for file in 0..8 {
                let from = Square::new(rank, file).unwrap();
                let Some(piece) = self.get_piece(from) else {
                    continue;
                };
                if piece.color != color {
                    continue;
                }

                match piece.piece_type {
                    PieceType::Pawn => self.generate_pawn_moves(from, piece.color, &mut moves),
                    PieceType::Knight => self.generate_knight_moves(from, piece.color, &mut moves),
                    PieceType::Bishop => self.generate_sliding_moves(
                        from,
                        piece.color,
                        &[(1, 1), (1, -1), (-1, 1), (-1, -1)],
                        &mut moves,
                    ),
                    PieceType::Rook => self.generate_sliding_moves(
                        from,
                        piece.color,
                        &[(1, 0), (-1, 0), (0, 1), (0, -1)],
                        &mut moves,
                    ),
                    PieceType::Queen => self.generate_sliding_moves(
                        from,
                        piece.color,
                        &[
                            (1, 0),
                            (-1, 0),
                            (0, 1),
                            (0, -1),
                            (1, 1),
                            (1, -1),
                            (-1, 1),
                            (-1, -1),
                        ],
                        &mut moves,
                    ),
                    PieceType::King => self.generate_king_moves(from, piece.color, &mut moves),
                }
            }
        }

        moves
    }

    fn generate_pawn_moves(&self, from: Square, color: PieceColor, moves: &mut Vec<Move>) {
        let direction = if color == PieceColor::White { 1 } else { -1 };
        let start_rank = if color == PieceColor::White { 1 } else { 6 };
        let promotion_rank = if color == PieceColor::White { 7 } else { 0 };

        if let Some(one_forward) = from.offset(direction, 0) {
            if self.get_piece(one_forward).is_none() {
                self.push_pawn_move(from, one_forward, promotion_rank, moves);

                if from.rank == start_rank {
                    if let Some(two_forward) = from.offset(direction * 2, 0) {
                        if self.get_piece(two_forward).is_none() {
                            moves.push(Move {
                                from,
                                to: two_forward,
                                promotion: None,
                            });
                        }
                    }
                }
            }
        }

        for file_delta in [-1, 1] {
            if let Some(capture) = from.offset(direction, file_delta) {
                if let Some(target) = self.get_piece(capture) {
                    if target.color != color {
                        self.push_pawn_move(from, capture, promotion_rank, moves);
                    }
                }
            }
        }
    }

    fn push_pawn_move(
        &self,
        from: Square,
        to: Square,
        promotion_rank: i8,
        moves: &mut Vec<Move>,
    ) {
        if to.rank == promotion_rank {
            for promotion in [
                PieceType::Queen,
                PieceType::Rook,
                PieceType::Bishop,
                PieceType::Knight,
            ] {
                moves.push(Move {
                    from,
                    to,
                    promotion: Some(promotion),
                });
            }
        } else {
            moves.push(Move {
                from,
                to,
                promotion: None,
            });
        }
    }

    fn generate_knight_moves(&self, from: Square, color: PieceColor, moves: &mut Vec<Move>) {
        for (rank_delta, file_delta) in [
            (2, 1),
            (2, -1),
            (-2, 1),
            (-2, -1),
            (1, 2),
            (1, -2),
            (-1, 2),
            (-1, -2),
        ] {
            if let Some(to) = from.offset(rank_delta, file_delta) {
                match self.get_piece(to) {
                    Some(piece) if piece.color == color => {}
                    _ => moves.push(Move {
                        from,
                        to,
                        promotion: None,
                    }),
                }
            }
        }
    }

    fn generate_sliding_moves(
        &self,
        from: Square,
        color: PieceColor,
        directions: &[(i8, i8)],
        moves: &mut Vec<Move>,
    ) {
        for &(rank_delta, file_delta) in directions {
            let mut current = from;
            loop {
                let Some(next) = current.offset(rank_delta, file_delta) else {
                    break;
                };

                match self.get_piece(next) {
                    Some(piece) if piece.color == color => break,
                    Some(_) => {
                        moves.push(Move {
                            from,
                            to: next,
                            promotion: None,
                        });
                        break;
                    }
                    None => {
                        moves.push(Move {
                            from,
                            to: next,
                            promotion: None,
                        });
                        current = next;
                    }
                }
            }
        }
    }

    fn generate_king_moves(&self, from: Square, color: PieceColor, moves: &mut Vec<Move>) {
        for rank_delta in -1..=1 {
            for file_delta in -1..=1 {
                if rank_delta == 0 && file_delta == 0 {
                    continue;
                }
                if let Some(to) = from.offset(rank_delta, file_delta) {
                    match self.get_piece(to) {
                        Some(piece) if piece.color == color => {}
                        _ => moves.push(Move {
                            from,
                            to,
                            promotion: None,
                        }),
                    }
                }
            }
        }
    }

    fn is_square_attacked(&self, target: Square, attacker_color: PieceColor) -> bool {
        let pawn_rank_delta = if attacker_color == PieceColor::White {
            -1
        } else {
            1
        };
        for file_delta in [-1, 1] {
            if let Some(from) = target.offset(pawn_rank_delta, file_delta) {
                if let Some(piece) = self.get_piece(from) {
                    if piece.color == attacker_color && piece.piece_type == PieceType::Pawn {
                        return true;
                    }
                }
            }
        }

        for (rank_delta, file_delta) in [
            (2, 1),
            (2, -1),
            (-2, 1),
            (-2, -1),
            (1, 2),
            (1, -2),
            (-1, 2),
            (-1, -2),
        ] {
            if let Some(from) = target.offset(rank_delta, file_delta) {
                if let Some(piece) = self.get_piece(from) {
                    if piece.color == attacker_color && piece.piece_type == PieceType::Knight {
                        return true;
                    }
                }
            }
        }

        if self.has_sliding_attacker(
            target,
            attacker_color,
            &[(1, 0), (-1, 0), (0, 1), (0, -1)],
            &[PieceType::Rook, PieceType::Queen],
        ) {
            return true;
        }

        if self.has_sliding_attacker(
            target,
            attacker_color,
            &[(1, 1), (1, -1), (-1, 1), (-1, -1)],
            &[PieceType::Bishop, PieceType::Queen],
        ) {
            return true;
        }

        for rank_delta in -1..=1 {
            for file_delta in -1..=1 {
                if rank_delta == 0 && file_delta == 0 {
                    continue;
                }
                if let Some(from) = target.offset(rank_delta, file_delta) {
                    if let Some(piece) = self.get_piece(from) {
                        if piece.color == attacker_color && piece.piece_type == PieceType::King {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    fn has_sliding_attacker(
        &self,
        target: Square,
        attacker_color: PieceColor,
        directions: &[(i8, i8)],
        valid_attackers: &[PieceType],
    ) -> bool {
        for &(rank_delta, file_delta) in directions {
            let mut current = target;
            loop {
                let Some(next) = current.offset(rank_delta, file_delta) else {
                    break;
                };
                current = next;

                let Some(piece) = self.get_piece(next) else {
                    continue;
                };

                if piece.color == attacker_color && valid_attackers.contains(&piece.piece_type) {
                    return true;
                }
                break;
            }
        }

        false
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_board_setup() {
        let board = Board::new_starting_position();

        for file in 0..8 {
            assert_eq!(
                board.get_piece(Square::new(1, file).unwrap()),
                Some(Piece {
                    piece_type: PieceType::Pawn,
                    color: PieceColor::White,
                })
            );
            assert_eq!(
                board.get_piece(Square::new(6, file).unwrap()),
                Some(Piece {
                    piece_type: PieceType::Pawn,
                    color: PieceColor::Black,
                })
            );
        }

        assert_eq!(
            board.get_piece(Square::new(0, 4).unwrap()),
            Some(Piece {
                piece_type: PieceType::King,
                color: PieceColor::White,
            })
        );
        assert_eq!(
            board.get_piece(Square::new(7, 4).unwrap()),
            Some(Piece {
                piece_type: PieceType::King,
                color: PieceColor::Black,
            })
        );
        assert_eq!(board.get_piece(Square::new(3, 3).unwrap()), None);
    }

    #[test]
    fn test_pawn_moves() {
        let board = Board::new_starting_position();
        let d2 = Square::new(1, 3).unwrap();

        let moves: Vec<_> = board
            .generate_legal_moves()
            .into_iter()
            .filter(|mv| mv.from == d2)
            .collect();

        assert_eq!(moves.len(), 2);
        assert!(moves.contains(&Move {
            from: d2,
            to: Square::new(2, 3).unwrap(),
            promotion: None,
        }));
        assert!(moves.contains(&Move {
            from: d2,
            to: Square::new(3, 3).unwrap(),
            promotion: None,
        }));

        let mut capture_board = Board::empty(PieceColor::White);
        capture_board.set_piece(
            Square::new(0, 4).unwrap(),
            Some(Piece {
                piece_type: PieceType::King,
                color: PieceColor::White,
            }),
        );
        capture_board.set_piece(
            Square::new(7, 4).unwrap(),
            Some(Piece {
                piece_type: PieceType::King,
                color: PieceColor::Black,
            }),
        );
        capture_board.set_piece(
            Square::new(3, 3).unwrap(),
            Some(Piece {
                piece_type: PieceType::Pawn,
                color: PieceColor::White,
            }),
        );
        capture_board.set_piece(
            Square::new(4, 4).unwrap(),
            Some(Piece {
                piece_type: PieceType::Knight,
                color: PieceColor::Black,
            }),
        );

        let capture_moves: Vec<_> = capture_board
            .generate_legal_moves()
            .into_iter()
            .filter(|mv| mv.from == Square::new(3, 3).unwrap())
            .collect();

        assert!(capture_moves.contains(&Move {
            from: Square::new(3, 3).unwrap(),
            to: Square::new(4, 3).unwrap(),
            promotion: None,
        }));
        assert!(capture_moves.contains(&Move {
            from: Square::new(3, 3).unwrap(),
            to: Square::new(4, 4).unwrap(),
            promotion: None,
        }));
    }

    #[test]
    fn test_knight_moves() {
        let board = Board::new_starting_position();
        let b1 = Square::new(0, 1).unwrap();

        let moves: Vec<_> = board
            .generate_legal_moves()
            .into_iter()
            .filter(|mv| mv.from == b1)
            .collect();

        assert_eq!(moves.len(), 2);
        assert!(moves.contains(&Move {
            from: b1,
            to: Square::new(2, 0).unwrap(),
            promotion: None,
        }));
        assert!(moves.contains(&Move {
            from: b1,
            to: Square::new(2, 2).unwrap(),
            promotion: None,
        }));
    }

    #[test]
    fn test_check_detection() {
        let mut board = Board::empty(PieceColor::White);
        board.set_piece(
            Square::new(0, 4).unwrap(),
            Some(Piece {
                piece_type: PieceType::King,
                color: PieceColor::White,
            }),
        );
        board.set_piece(
            Square::new(7, 4).unwrap(),
            Some(Piece {
                piece_type: PieceType::King,
                color: PieceColor::Black,
            }),
        );

        assert!(!board.is_king_in_check(PieceColor::White));
        assert!(!board.is_king_in_check(PieceColor::Black));

        board.set_piece(
            Square::new(4, 4).unwrap(),
            Some(Piece {
                piece_type: PieceType::Rook,
                color: PieceColor::Black,
            }),
        );
        assert!(board.is_king_in_check(PieceColor::White));
        assert!(!board.is_king_in_check(PieceColor::Black));
    }

    #[test]
    fn test_move_application() {
        let board = Board::new_starting_position();
        let mv = Move {
            from: Square::new(1, 4).unwrap(),
            to: Square::new(3, 4).unwrap(),
            promotion: None,
        };

        let next = board.apply_move(&mv);

        assert_eq!(next.get_piece(Square::new(1, 4).unwrap()), None);
        assert_eq!(
            next.get_piece(Square::new(3, 4).unwrap()),
            Some(Piece {
                piece_type: PieceType::Pawn,
                color: PieceColor::White,
            })
        );
        assert_eq!(next.active_color, PieceColor::Black);
    }

    #[test]
    fn test_board_evaluation() {
        let mut board = Board::new_starting_position();
        assert_eq!(board.evaluate_board(), 0);

        board.set_piece(Square::new(1, 0).unwrap(), None);
        assert_eq!(board.evaluate_board(), -100);

        board.set_piece(Square::new(7, 1).unwrap(), None);
        assert_eq!(board.evaluate_board(), 200);
    }

    #[test]
    fn test_pawn_attack_does_not_count_forward_as_check() {
        let mut board = Board::empty(PieceColor::White);
        board.set_piece(
            Square::new(0, 4).unwrap(),
            Some(Piece {
                piece_type: PieceType::King,
                color: PieceColor::White,
            }),
        );
        board.set_piece(
            Square::new(7, 4).unwrap(),
            Some(Piece {
                piece_type: PieceType::King,
                color: PieceColor::Black,
            }),
        );
        board.set_piece(
            Square::new(1, 4).unwrap(),
            Some(Piece {
                piece_type: PieceType::Pawn,
                color: PieceColor::Black,
            }),
        );

        assert!(!board.is_king_in_check(PieceColor::White));
    }

    #[test]
    fn test_pinned_piece_move_is_illegal() {
        let mut board = Board::empty(PieceColor::White);
        board.set_piece(
            Square::new(0, 4).unwrap(),
            Some(Piece {
                piece_type: PieceType::King,
                color: PieceColor::White,
            }),
        );
        board.set_piece(
            Square::new(1, 4).unwrap(),
            Some(Piece {
                piece_type: PieceType::Rook,
                color: PieceColor::White,
            }),
        );
        board.set_piece(
            Square::new(7, 4).unwrap(),
            Some(Piece {
                piece_type: PieceType::Rook,
                color: PieceColor::Black,
            }),
        );
        board.set_piece(
            Square::new(7, 7).unwrap(),
            Some(Piece {
                piece_type: PieceType::King,
                color: PieceColor::Black,
            }),
        );

        let legal_moves = board.generate_legal_moves();

        assert!(!legal_moves.contains(&Move {
            from: Square::new(1, 4).unwrap(),
            to: Square::new(1, 5).unwrap(),
            promotion: None,
        }));
        assert!(legal_moves.contains(&Move {
            from: Square::new(1, 4).unwrap(),
            to: Square::new(2, 4).unwrap(),
            promotion: None,
        }));
    }
}
