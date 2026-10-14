use std::collections::{HashMap, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Cube {
    faces: [[u8; 9]; 6],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
        Self {
            faces: std::array::from_fn(|face| [face as u8; 9]),
        }
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

    for row in 0..3 {
        for col in 0..3 {
            let source = row * 3 + col;
            let (new_row, new_col) = if clockwise {
                (col, 2 - row)
            } else {
                (2 - col, row)
            };
            next[face][new_row * 3 + new_col] = old[face][source];
        }
    }

    // Each side of the cube is represented by a 3-cell strip. The strip
    // ordering here makes each face turn a four-cycle of adjacent strips.
    let strips: [[(usize, [usize; 3]); 4]; 6] = [
        [(4, [0, 1, 2]), (3, [0, 1, 2]), (5, [0, 1, 2]), (2, [0, 1, 2])],
        [(4, [6, 7, 8]), (2, [6, 7, 8]), (5, [6, 7, 8]), (3, [6, 7, 8])],
        [(0, [0, 3, 6]), (4, [0, 3, 6]), (1, [0, 3, 6]), (5, [8, 5, 2])],
        [(0, [2, 5, 8]), (5, [6, 3, 0]), (1, [2, 5, 8]), (4, [2, 5, 8])],
        [(0, [6, 7, 8]), (2, [8, 5, 2]), (1, [2, 1, 0]), (3, [0, 3, 6])],
        [(0, [2, 1, 0]), (3, [8, 5, 2]), (1, [6, 7, 8]), (2, [0, 3, 6])],
    ];

    let cycle = strips[face];
    for position in 0..4 {
        let destination_position = if clockwise {
            (position + 1) % 4
        } else {
            (position + 3) % 4
        };
        let (source_face, source_indices) = cycle[position];
        let (dest_face, dest_indices) = cycle[destination_position];
        for offset in 0..3 {
            if let (Some(&source_index), Some(&dest_index)) =
                (source_indices.get(offset), dest_indices.get(offset))
            {
                next[dest_face][dest_index] = old[source_face][source_index];
            }
        }
    }

    cube.faces = next;
}

fn is_solved(cube: &Cube) -> bool {
    cube.faces.iter().all(|face| {
        face.first()
            .is_some_and(|first| face.iter().all(|cell| cell == first))
    })
}

fn scramble(cube: &mut Cube, len: usize, seed: u64) -> Vec<Move> {
    const MOVES: [Move; 12] = [
        Move::U,
        Move::UPrime,
        Move::D,
        Move::DPrime,
        Move::L,
        Move::LPrime,
        Move::R,
        Move::RPrime,
        Move::F,
        Move::FPrime,
        Move::B,
        Move::BPrime,
    ];

    let mut state = seed;
    let mut result = Vec::with_capacity(len);
    for _ in 0..len {
        // SplitMix64, followed by modulo over the actual move table length.
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^= value >> 31;

        let Some(mv) = MOVES.get((value as usize) % MOVES.len()).copied() else {
            break;
        };
        rotate(cube, mv);
        result.push(mv);
    }
    result
}

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

fn solve(cube: &Cube) -> Option<Vec<Move>> {
    if is_solved(cube) {
        return Some(Vec::new());
    }

    // A bounded breadth-first search is practical for short, reachable states.
    // The key stores the complete facelet state, so paths are not conflated.
    const SEARCH_LIMIT: usize = 250_000;
    const MOVES: [Move; 12] = [
        Move::U,
        Move::UPrime,
        Move::D,
        Move::DPrime,
        Move::L,
        Move::LPrime,
        Move::R,
        Move::RPrime,
        Move::F,
        Move::FPrime,
        Move::B,
        Move::BPrime,
    ];

    let start_key = cube.faces;
    let mut queue = VecDeque::new();
    let mut parent: HashMap<[[u8; 9]; 6], ([[u8; 9]; 6], Move)> = HashMap::new();
    queue.push_back(cube.clone());
    parent.insert(start_key, (start_key, Move::U));

    while let Some(current) = queue.pop_front() {
        let current_key = current.faces;
        if is_solved(&current) {
            let mut path = Vec::new();
            let mut key = current_key;
            while key != start_key {
                let (previous, mv) = *parent.get(&key)?;
                path.push(mv);
                key = previous;
            }
            path.reverse();
            return Some(path);
        }
        if parent.len() >= SEARCH_LIMIT {
            return None;
        }

        for &mv in &MOVES {
            let mut next = current.clone();
            rotate(&mut next, mv);
            let key = next.faces;
            if let std::collections::hash_map::Entry::Vacant(entry) = parent.entry(key) {
                entry.insert((current_key, mv));
                queue.push_back(next);
            }
        }
    }

    None
}

/// Runs a small demonstration of cube scrambling and solving.
pub fn main() {
    let mut cube = Cube::new();
    let moves = scramble(&mut cube, 5, 42);
    println!("Scrambled with {} moves; solved: {}", moves.len(), is_solved(&cube));
    if let Some(solution) = solve(&cube) {
        for mv in solution {
            rotate(&mut cube, mv);
        }
        println!("Solved after applying solution: {}", is_solved(&cube));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solved_state_detection() {
        assert!(is_solved(&Cube::new()));
    }

    #[test]
    fn rotation_and_inverse_restore_state() {
        for mv in [
            Move::U,
            Move::D,
            Move::L,
            Move::R,
            Move::F,
            Move::B,
        ] {
            let mut cube = Cube::new();
            rotate(&mut cube, mv);
            rotate(&mut cube, inverse(mv));
            assert_eq!(cube, Cube::new());
        }
    }

    #[test]
    fn scramble_is_deterministic_and_has_requested_length() {
        let mut first = Cube::new();
        let mut second = Cube::new();
        let moves1 = scramble(&mut first, 20, 42);
        let moves2 = scramble(&mut second, 20, 42);
        assert_eq!(moves1.len(), 20);
        assert_eq!(moves1, moves2);
        assert_eq!(first, second);
        assert_ne!(first, Cube::new());
    }

    #[test]
    fn zero_length_scramble_leaves_cube_unchanged() {
        let mut cube = Cube::new();
        assert!(scramble(&mut cube, 0, 7).is_empty());
        assert_eq!(cube, Cube::new());
    }

    #[test]
    fn basic_solve_works_for_solved_cube() {
        let cube = Cube::new();
        let solution = solve(&cube).expect("solved cube should have a solution");
        assert!(solution.is_empty());
    }

    #[test]
    fn solver_handles_short_scramble_end_to_end() {
        let mut cube = Cube::new();
        let moves = scramble(&mut cube, 2, 99);
        let solution = solve(&cube).expect("short scramble should be solvable");
        for mv in solution {
            rotate(&mut cube, mv);
        }
        assert!(is_solved(&cube));
        assert_eq!(moves.len(), 2);
    }
}