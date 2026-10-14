#[test]
fn new_cube_is_solved() {
    assert!(is_solved(&Cube::new()));
}

#[test]
fn turn_then_inverse_restores() {
    let mut cube = Cube::new();
    rotate(&mut cube, Move::R);
    assert!(!is_solved(&cube));
    rotate(&mut cube, Move::RPrime);
    assert!(is_solved(&cube));
}

#[test]
fn scramble_returns_requested_length() {
    let mut cube = Cube::new();
    assert_eq!(scramble(&mut cube, 7, 42).len(), 7);
}
