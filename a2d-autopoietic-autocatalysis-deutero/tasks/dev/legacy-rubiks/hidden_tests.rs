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

fn all_moves() -> [Move; 12] {
    [
        Move::U, Move::UPrime,
        Move::D, Move::DPrime,
        Move::L, Move::LPrime,
        Move::R, Move::RPrime,
        Move::F, Move::FPrime,
        Move::B, Move::BPrime,
    ]
}

fn face_turns() -> [Move; 6] {
    [Move::U, Move::D, Move::L, Move::R, Move::F, Move::B]
}

fn apply_moves(cube: &mut Cube, moves: &[Move]) {
    for &mv in moves {
        rotate(cube, mv);
    }
}

#[test]
fn solved_cube_starts_solved() {
    let cube = Cube::new();
    assert!(is_solved(&cube), "Cube::new() must produce a solved cube");
}

#[test]
fn every_rotation_changes_solved_cube_and_inverse_restores() {
    for mv in all_moves() {
        let mut cube = Cube::new();
        rotate(&mut cube, mv);
        assert!(!is_solved(&cube), "{:?} must not be a no-op on a solved cube", mv);
        rotate(&mut cube, inverse(mv));
        assert!(is_solved(&cube), "{:?} followed by its inverse must restore solved state", mv);
    }
}

#[test]
fn quarter_turns_have_order_four_not_two() {
    for mv in face_turns() {
        let mut cube = Cube::new();
        rotate(&mut cube, mv);
        assert!(!is_solved(&cube), "{:?} once should not solve", mv);
        rotate(&mut cube, mv);
        assert!(!is_solved(&cube), "{:?} twice should not solve", mv);
        rotate(&mut cube, mv);
        assert!(!is_solved(&cube), "{:?} three times should not solve", mv);
        rotate(&mut cube, mv);
        assert!(is_solved(&cube), "{:?} four times should restore solved", mv);
    }
}

#[test]
fn known_sequence_inverse_roundtrip_restores_solved() {
    let sequence = [
        Move::U, Move::R, Move::F, Move::LPrime,
        Move::D, Move::B, Move::RPrime, Move::UPrime,
        Move::FPrime, Move::L,
    ];
    let mut cube = Cube::new();
    apply_moves(&mut cube, &sequence);
    assert!(!is_solved(&cube), "known scramble must change cube");
    for &mv in sequence.iter().rev() {
        rotate(&mut cube, inverse(mv));
    }
    assert!(is_solved(&cube), "reverse inverse sequence must restore solved state");
}

#[test]
fn solver_solves_known_scrambles_by_returning_moves() {
    let cases: &[&[Move]] = &[
        &[Move::U],
        &[Move::U, Move::R, Move::F],
        &[Move::U, Move::R, Move::F, Move::LPrime, Move::D, Move::B, Move::RPrime, Move::UPrime],
    ];
    for (index, sequence) in cases.iter().enumerate() {
        let mut cube = Cube::new();
        apply_moves(&mut cube, sequence);
        assert!(!is_solved(&cube), "case {} should be scrambled", index);
        let solution = solve(&cube).unwrap_or_else(|| panic!("solver returned None for case {}", index));
        assert!(solution.len() <= 500, "solution for case {} is unreasonably long: {} moves", index, solution.len());
        apply_moves(&mut cube, &solution);
        assert!(is_solved(&cube), "solver failed to solve case {}", index);
    }
}

#[test]
fn seeded_scramble_is_replayable_and_solvable() {
    let mut cube = Cube::new();
    let scramble_moves = scramble(&mut cube, 12, 0xA2D5_EED);
    assert_eq!(scramble_moves.len(), 12, "scramble must return len moves");
    assert!(!is_solved(&cube), "non-empty scramble must change cube");

    let mut replay = Cube::new();
    apply_moves(&mut replay, &scramble_moves);
    assert_eq!(cube, replay, "scramble must apply exactly the moves it returns");

    let solution = solve(&cube).expect("solver must solve seeded scramble");
    apply_moves(&mut cube, &solution);
    assert!(is_solved(&cube), "solver must solve scrambled cube");
}
