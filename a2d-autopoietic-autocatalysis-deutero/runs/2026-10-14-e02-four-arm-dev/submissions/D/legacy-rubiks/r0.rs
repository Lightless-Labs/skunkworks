#[derive(Clone, Debug, PartialEq, Eq)]
struct Cube {
    faces: [[u8; 9]; 6],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Move {
    U,
    UPrime,
    D,
    DPrime,
    L,
    LPrime,
    R,
    RPrime,
    F,
    FPrime,
    B,
    BPrime,
}

impl Cube {
    fn new() -> Self {
        let mut faces = [[0; 9]; 6];
        for (face, cells) in faces.iter_mut().enumerate() {
            cells.fill(face as u8);
        }
        Self { faces }
    }
}

fn rotate(cube: &mut Cube, mv: Move) {
    let (face, clockwise) = match mv {
        Move::U => (0, true),
        Move::UPrime => (0, false),
        Move::D => (1, true),
        Move::DPrime => (1, false),
        Move::L => (2, true),
        Move::LPrime => (2, false),
        Move::R => (3, true),
        Move::RPrime => (3, false),
        Move::F => (4, true),
        Move::FPrime => (4, false),
        Move::B => (5, true),
        Move::BPrime => (5, false),
    };

    let old = cube.faces;
    let mut next = old;
    let source = old[face];

    for row in 0..3 {
        for col in 0..3 {
            let (new_row, new_col) = if clockwise {
                (col, 2 - row)
            } else {
                (2 - col, row)
            };
            next[face][new_row * 3 + new_col] = source[row * 3 + col];
        }
    }

    // Face order: U, D, L, R, F, B. Each side's stickers are
    // cycled in strips corresponding to the selected face turn.
    let (strips, reverse) = match face {
        0 => ([[(4, 0), (4, 1), (4, 2)], [(3, 0), (3, 1), (3, 2)],
              [(5, 0), (5, 1), (5, 2)], [(2, 0), (2, 1), (2, 2)]], false),
        1 => ([[(4, 6), (4, 7), (4, 8)], [(2, 6), (2, 7), (2, 8)],
              [(5, 6), (5, 7), (5, 8)], [(3, 6), (3, 7), (3, 8)]], false),
        2 => ([[(0, 0), (0, 3), (0, 6)], [(4, 0), (4, 3), (4, 6)],
              [(1, 0), (1, 3), (1, 6)], [(5, 8), (5, 5), (5, 2)]], false),
        3 => ([[(0, 8), (0, 5), (0, 2)], [(5, 0), (5, 3), (5, 6)],
              [(1, 8), (1, 5), (1, 2)], [(4, 8), (4, 5), (4, 2)]], false),
        4 => ([[(0, 6), (0, 7), (0, 8)], [(3, 0), (3, 3), (3, 6)],
              [(1, 2), (1, 1), (1, 0)], [(2, 8), (2, 5), (2, 2)]], false),
        _ => ([[(0, 2), (0, 1), (0, 0)], [(2, 0), (2, 3), (2, 6)],
              [(1, 6), (1, 7), (1, 8)], [(3, 8), (3, 5), (3, 2)]], false),
    };

    let _ = reverse;
    for i in 0..3 {
        let value = old[strips[0][i].0][strips[0][i].1];
        let v1 = old[strips[1][i].0][strips[1][i].1];
        let v2 = old[strips[2][i].0][strips[2][i].1];
        let v3 = old[strips[3][i].0][strips[3][i].1];

        if clockwise {
            let cycle = if matches!(face, 2 | 3 | 4 | 5) {
                [v3, value, v1, v2]
            } else {
                [v3, value, v1, v2]
            };
            for (slot, val) in strips.iter().zip(cycle) {
                next[slot[i].0][slot[i].1] = val;
            }
        } else {
            let cycle = [v1, v2, v3, value];
            for (slot, val) in strips.iter().zip(cycle) {
                next[slot[i].0][slot[i].1] = val;
            }
        }
    }

    cube.faces = next;
}

fn is_solved(cube: &Cube) -> bool {
    cube.faces
        .iter()
        .enumerate()
        .all(|(face, cells)| cells.iter().all(|&cell| cell == face as u8))
}

fn scramble(cube: &mut Cube, len: usize, seed: u64) -> Vec<Move> {
    const MOVES: [Move; 12] = [
        Move::U, Move::UPrime, Move::D, Move::DPrime,
        Move::L, Move::LPrime, Move::R, Move::RPrime,
        Move::F, Move::FPrime, Move::B, Move::BPrime,
    ];
    let mut state = seed;
    let mut result = Vec::with_capacity(len);
    for _ in 0..len {
        state = state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^= z >> 31;
        let mv = MOVES[(z as usize) % MOVES.len()];
        rotate(cube, mv);
        result.push(mv);
    }
    result
}

fn solve(cube: &Cube) -> Option<Vec<Move>> {
    // A bounded IDA* solver over face-turn moves. Its heuristic counts
    // misplaced stickers and is admissible when divided by 20, since one
    // turn can affect at most 20 stickers.
    if is_solved(cube) {
        return Some(Vec::new());
    }

    fn heuristic(cube: &Cube) -> usize {
        let misplaced: usize = cube.faces.iter().enumerate()
            .map(|(f, cells)| cells.iter().filter(|&&x| x != f as u8).count())
            .sum();
        (misplaced + 19) / 20
    }

    fn search(
        cube: &Cube,
        remaining: usize,
        previous_face: Option<u8>,
        path: &mut Vec<Move>,
    ) -> bool {
        if is_solved(cube) {
            return true;
        }
        if remaining == 0 || heuristic(cube) > remaining {
            return false;
        }

        let moves = [
            Move::U, Move::UPrime, Move::D, Move::DPrime,
            Move::L, Move::LPrime, Move::R, Move::RPrime,
            Move::F, Move::FPrime, Move::B, Move::BPrime,
        ];
        for (index, &mv) in moves.iter().enumerate() {
            let face = (index / 2) as u8;
            if previous_face == Some(face) {
                continue;
            }
            let mut next = cube.clone();
            rotate(&mut next, mv);
            path.push(mv);
            if search(&next, remaining - 1, Some(face), path) {
                return true;
            }
            path.pop();
        }
        false
    }

    for depth in heuristic(cube)..=12 {
        let mut path = Vec::new();
        if search(cube, depth, None, &mut path) {
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solved_state_detection() {
        assert!(is_solved(&Cube::new()));
    }

    #[test]
    fn single_rotation_and_inverse_restore_state() {
        for mv in [
            Move::U, Move::D, Move::L, Move::R, Move::F, Move::B,
        ] {
            let inverse = match mv {
                Move::U => Move::UPrime,
                Move::D => Move::DPrime,
                Move::L => Move::LPrime,
                Move::R => Move::RPrime,
                Move::F => Move::FPrime,
                Move::B => Move::BPrime,
                _ => unreachable!(),
            };
            let mut cube = Cube::new();
            rotate(&mut cube, mv);
            assert!(!is_solved(&cube));
            rotate(&mut cube, inverse);
            assert_eq!(cube, Cube::new());
        }
    }

    #[test]
    fn scramble_changes_state() {
        let mut cube = Cube::new();
        scramble(&mut cube, 10, 42);
        assert!(!is_solved(&cube));
    }

    #[test]
    fn basic_solve_works() {
        let mut cube = Cube::new();
        let moves = scramble(&mut cube, 3, 7);
        let solution = solve(&cube).expect("scrambled cube should be solvable");
        for mv in solution {
            rotate(&mut cube, mv);
        }
        assert!(is_solved(&cube));
        assert_eq!(moves.len(), 3);
    }
}