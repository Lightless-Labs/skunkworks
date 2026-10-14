#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}

impl Color {
    fn opposite(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
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
    white_king_side_castling: bool,
    white_queen_side_castling: bool,
    black_king_side_castling: bool,
    black_queen_side_castling: bool,
    en_passant: Option<(usize, usize)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Move {
    pub from_rank: usize,
    pub from_file: usize,
    pub to_rank: usize,
    pub to_file: usize,
    pub promotion: Option<PieceKind>,
    pub en_passant: bool,
    pub castle: bool,
}

impl Move {
    fn new(from_rank: usize, from_file: usize, to_rank: usize, to_file: usize) -> Self {
        Self {
            from_rank,
            from_file,
            to_rank,
            to_file,
            promotion: None,
            en_passant: false,
            castle: false,
        }
    }
}

impl Board {
    pub fn new() -> Self {
        let mut board = Self {
            squares: [[None; 8]; 8],
            white_king_side_castling: true,
            white_queen_side_castling: true,
            black_king_side_castling: true,
            black_queen_side_castling: true,
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
            board.squares[0][file] = Some(Piece::new(Color::Black, back_rank[file]));
            board.squares[1][file] = Some(Piece::new(Color::Black, PieceKind::Pawn));
            board.squares[6][file] = Some(Piece::new(Color::White, PieceKind::Pawn));
            board.squares[7][file] = Some(Piece::new(Color::White, back_rank[file]));
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

    fn attacked_by(&self, rank: usize, file: usize, attacker: Color) -> bool {
        let pawn_source_rank = match attacker {
            Color::White => rank.checked_add(1),
            Color::Black => rank.checked_sub(1),
        };

        if let Some(source_rank) = pawn_source_rank {
            if source_rank < 8 {
                for source_file in [file.checked_sub(1), file.checked_add(1)]
                    .into_iter()
                    .flatten()
                    .filter(|&f| f < 8)
                {
                    if self.squares[source_rank][source_file]
                        == Some(Piece::new(attacker, PieceKind::Pawn))
                    {
                        return true;
                    }
                }
            }
        }

        let knight_offsets = [
            (-2isize, -1isize),
            (-2, 1),
            (-1, -2),
            (-1, 2),
            (1, -2),
            (1, 2),
            (2, -1),
            (2, 1),
        ];
        for (dr, df) in knight_offsets {
            let r = rank as isize + dr;
            let f = file as isize + df;
            if (0..8).contains(&r)
                && (0..8).contains(&f)
                && self.squares[r as usize][f as usize]
                    == Some(Piece::new(attacker, PieceKind::Knight))
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
                if (0..8).contains(&r)
                    && (0..8).contains(&f)
                    && self.squares[r as usize][f as usize]
                        == Some(Piece::new(attacker, PieceKind::King))
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
        for (index, (dr, df)) in directions.into_iter().enumerate() {
            let mut r = rank as isize + dr;
            let mut f = file as isize + df;
            while (0..8).contains(&r) && (0..8).contains(&f) {
                if let Some(piece) = self.squares[r as usize][f as usize] {
                    if piece.color == attacker {
                        let diagonal = index >= 4;
                        if piece.kind == PieceKind::Queen
                            || (diagonal && piece.kind == PieceKind::Bishop)
                            || (!diagonal && piece.kind == PieceKind::Rook)
                        {
                            return true;
                        }
                    }
                    break;
                }
                r += dr;
                f += df;
            }
        }

        false
    }

    fn pseudo_moves(&self, color: Color) -> Vec<Move> {
        let mut moves = Vec::new();

        for rank in 0..8 {
            for file in 0..8 {
                let Some(piece) = self.squares[rank][file] else {
                    continue;
                };
                if piece.color != color {
                    continue;
                }

                match piece.kind {
                    PieceKind::Pawn => {
                        let direction: isize = if color == Color::White { -1 } else { 1 };
                        let start_rank = if color == Color::White { 6 } else { 1 };
                        let promotion_rank = if color == Color::White { 0 } else { 7 };
                        let one_rank = rank as isize + direction;

                        if (0..8).contains(&one_rank)
                            && self.squares[one_rank as usize][file].is_none()
                        {
                            let mut mv = Move::new(rank, file, one_rank as usize, file);
                            if one_rank as usize == promotion_rank {
                                mv.promotion = Some(PieceKind::Queen);
                            }
                            moves.push(mv);

                            let two_rank = rank as isize + 2 * direction;
                            if rank == start_rank
                                && self.squares[two_rank as usize][file].is_none()
                            {
                                moves.push(Move::new(rank, file, two_rank as usize, file));
                            }
                        }

                        for target_file in [file.checked_sub(1), file.checked_add(1)]
                            .into_iter()
                            .flatten()
                            .filter(|&f| f < 8)
                        {
                            if (0..8).contains(&one_rank) {
                                let target = self.squares[one_rank as usize][target_file];
                                if target.is_some_and(|p| p.color != color) {
                                    let mut mv =
                                        Move::new(rank, file, one_rank as usize, target_file);
                                    if one_rank as usize == promotion_rank {
                                        mv.promotion = Some(PieceKind::Queen);
                                    }
                                    moves.push(mv);
                                } else if self.en_passant
                                    == Some((one_rank as usize, target_file))
                                {
                                    let mut mv =
                                        Move::new(rank, file, one_rank as usize, target_file);
                                    mv.en_passant = true;
                                    moves.push(mv);
                                }
                            }
                        }
                    }
                    PieceKind::Knight => {
                        let offsets = [
                            (-2isize, -1isize),
                            (-2, 1),
                            (-1, -2),
                            (-1, 2),
                            (1, -2),
                            (1, 2),
                            (2, -1),
                            (2, 1),
                        ];
                        for (dr, df) in offsets {
                            let r = rank as isize + dr;
                            let f = file as isize + df;
                            if (0..8).contains(&r) && (0..8).contains(&f) {
                                let target = self.squares[r as usize][f as usize];
                                if target.is_none_or(|p| p.color != color) {
                                    moves.push(Move::new(rank, file, r as usize, f as usize));
                                }
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
                                if (0..8).contains(&r) && (0..8).contains(&f) {
                                    let target = self.squares[r as usize][f as usize];
                                    if target.is_none_or(|p| p.color != color) {
                                        moves.push(Move::new(
                                            rank,
                                            file,
                                            r as usize,
                                            f as usize,
                                        ));
                                    }
                                }
                            }
                        }

                        let home_rank = if color == Color::White { 7 } else { 0 };
                        if rank == home_rank && file == 4 {
                            let (king_side, queen_side) = match color {
                                Color::White => (
                                    self.white_king_side_castling,
                                    self.white_queen_side_castling,
                                ),
                                Color::Black => (
                                    self.black_king_side_castling,
                                    self.black_queen_side_castling,
                                ),
                            };

                            if king_side
                                && self.squares[home_rank][5].is_none()
                                && self.squares[home_rank][6].is_none()
                                && self.squares[home_rank][7]
                                    == Some(Piece::new(color, PieceKind::Rook))
                            {
                                let mut mv = Move::new(home_rank, 4, home_rank, 6);
                                mv.castle = true;
                                moves.push(mv);
                            }

                            if queen_side
                                && self.squares[home_rank][1].is_none()
                                && self.squares[home_rank][2].is_none()
                                && self.squares[home_rank][3].is_none()
                                && self.squares[home_rank][0]
                                    == Some(Piece::new(color, PieceKind::Rook))
                            {
                                let mut mv = Move::new(home_rank, 4, home_rank, 2);
                                mv.castle = true;
                                moves.push(mv);
                            }
                        }
                    }
                    PieceKind::Rook | PieceKind::Bishop | PieceKind::Queen => {
                        let directions: &[(isize, isize)] = match piece.kind {
                            PieceKind::Rook => &[(-1, 0), (1, 0), (0, -1), (0, 1)],
                            PieceKind::Bishop => &[(-1, -1), (-1, 1), (1, -1), (1, 1)],
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
                            while (0..8).contains(&r) && (0..8).contains(&f) {
                                let target = self.squares[r as usize][f as usize];
                                if target.is_none() {
                                    moves.push(Move::new(
                                        rank,
                                        file,
                                        r as usize,
                                        f as usize,
                                    ));
                                } else {
                                    if target.is_some_and(|p| p.color != color) {
                                        moves.push(Move::new(
                                            rank,
                                            file,
                                            r as usize,
                                            f as usize,
                                        ));
                                    }
                                    break;
                                }
                                r += dr;
                                f += df;
                            }
                        }
                    }
                }
            }
        }

        moves
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

pub fn is_check(board: &Board, white: bool) -> bool {
    let color = if white { Color::White } else { Color::Black };
    match board.king_position(color) {
        Some((rank, file)) => board.attacked_by(rank, file, color.opposite()),
        None => false,
    }
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

    let Some(mut piece) = next.squares[mv.from_rank][mv.from_file] else {
        return next;
    };

    let captured = if mv.en_passant {
        next.squares[mv.from_rank][mv.to_file]
    } else {
        next.squares[mv.to_rank][mv.to_file]
    };

    if piece.kind == PieceKind::King {
        match piece.color {
            Color::White => {
                next.white_king_side_castling = false;
                next.white_queen_side_castling = false;
            }
            Color::Black => {
                next.black_king_side_castling = false;
                next.black_queen_side_castling = false;
            }
        }
    }

    if piece.kind == PieceKind::Rook {
        match (piece.color, mv.from_rank, mv.from_file) {
            (Color::White, 7, 0) => next.white_queen_side_castling = false,
            (Color::White, 7, 7) => next.white_king_side_castling = false,
            (Color::Black, 0, 0) => next.black_queen_side_castling = false,
            (Color::Black, 0, 7) => next.black_king_side_castling = false,
            _ => {}
        }
    }

    if let Some(captured_piece) = captured {
        if captured_piece.kind == PieceKind::Rook {
            match (captured_piece.color, mv.to_rank, mv.to_file) {
                (Color::White, 7, 0) => next.white_queen_side_castling = false,
                (Color::White, 7, 7) => next.white_king_side_castling = false,
                (Color::Black, 0, 0) => next.black_queen_side_castling = false,
                (Color::Black, 0, 7) => next.black_king_side_castling = false,
                _ => {}
            }
        }
    }

    next.squares[mv.from_rank][mv.from_file] = None;
    if mv.en_passant {
        next.squares[mv.from_rank][mv.to_file] = None;
    }

    if mv.castle && piece.kind == PieceKind::King {
        if mv.to_file == 6 {
            next.squares[mv.to_rank][5] = next.squares[mv.to_rank][7];
            next.squares[mv.to_rank][7] = None;
        } else if mv.to_file == 2 {
            next.squares[mv.to_rank][3] = next.squares[mv.to_rank][0];
            next.squares[mv.to_rank][0] = None;
        }
    }

    if let Some(promotion) = mv.promotion {
        if piece.kind == PieceKind::Pawn {
            piece.kind = promotion;
        }
    }

    next.squares[mv.to_rank][mv.to_file] = Some(piece);
    next.en_passant = None;

    if piece.kind == PieceKind::Pawn
        && mv.from_file == mv.to_file
        && mv.from_rank.abs_diff(mv.to_rank) == 2
    {
        next.en_passant = Some(((mv.from_rank + mv.to_rank) / 2, mv.from_file));
    }

    next
}

pub fn generate_moves(board: &Board, white: bool) -> Vec<Move> {
    let color = if white { Color::White } else { Color::Black };
    let mut legal = Vec::new();

    for mv in board.pseudo_moves(color) {
        if mv.castle {
            if is_check(board, white) {
                continue;
            }
            let transit_file = if mv.to_file == 6 { 5 } else { 3 };
            let transit = Move::new(mv.from_rank, mv.from_file, mv.to_rank, transit_file);
            if is_check(&apply_move(board, &transit), white) {
                continue;
            }
        }

        let next = apply_move(board, &mv);
        if !is_check(&next, white) {
            legal.push(mv);
        }
    }

    legal
}

pub fn is_checkmate(board: &Board, white: bool) -> bool {
    is_check(board, white) && generate_moves(board, white).is_empty()
}

pub fn is_stalemate(board: &Board, white: bool) -> bool {
    !is_check(board, white) && generate_moves(board, white).is_empty()
}

pub fn evaluate_material(board: &Board) -> i32 {
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
                score += if piece.color == Color::White {
                    value
                } else {
                    -value
                };
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
            board.piece_at(7, 4),
            Some(Piece::new(Color::White, PieceKind::King))
        );
        assert_eq!(
            board.piece_at(0, 4),
            Some(Piece::new(Color::Black, PieceKind::King))
        );
        assert_eq!(
            board.piece_at(6, 3),
            Some(Piece::new(Color::White, PieceKind::Pawn))
        );
        assert_eq!(board.piece_at(4, 4), None);
    }

    #[test]
    fn initial_pawn_moves() {
        let board = Board::new();
        let moves = generate_moves(&board, true);
        assert!(moves.contains(&Move::new(6, 4, 5, 4)));
        assert!(moves.contains(&Move::new(6, 4, 4, 4)));
    }

    #[test]
    fn initial_knight_moves() {
        let board = Board::new();
        let moves = generate_moves(&board, true);
        assert!(moves.contains(&Move::new(7, 1, 5, 0)));
        assert!(moves.contains(&Move::new(7, 1, 5, 2)));
    }

    #[test]
    fn detects_check() {
        let mut board = Board::new();
        board.squares[6][5] = None;
        board.squares[1][4] = None;
        board.squares[1][5] = None;
        board.squares[3][5] = Some(Piece::new(Color::Black, PieceKind::Queen));
        assert!(is_check(&board, true));
    }

    #[test]
    fn applies_move_and_updates_en_passant() {
        let board = Board::new();
        let mv = Move::new(6, 4, 4, 4);
        let next = apply_move(&board, &mv);
        assert_eq!(
            next.piece_at(4, 4),
            Some(Piece::new(Color::White, PieceKind::Pawn))
        );
        assert_eq!(next.piece_at(6, 4), None);
        assert_eq!(next.en_passant, Some((5, 4)));
    }

    #[test]
    fn material_evaluation_is_zero_at_start() {
        assert_eq!(evaluate_material(&Board::new()), 0);
    }
}