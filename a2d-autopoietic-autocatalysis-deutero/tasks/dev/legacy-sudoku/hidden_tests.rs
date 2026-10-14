// Easy puzzle (many givens)
#[test]
fn solves_easy_puzzle() {
    let puzzle = "530070000600195000098000060800060003400803001700020006060000280000419005000080079";
    let expected = "534678912672195348198342567859761423426853791713924856961537284287419635345286179";
    let grid = parse(&puzzle);
    let solved = solve(grid);
    assert!(solved.is_some(), "Must solve easy puzzle");
    let solution = solved.unwrap();
    let result: String = solution.iter().flat_map(|row| row.iter()).map(|&c| char::from_digit(c as u32, 10).unwrap()).collect();
    assert_eq!(result, expected);
}

// Medium puzzle (fewer givens)
#[test]
fn solves_medium_puzzle() {
    let puzzle = "000260701680070090190004500820100040004602900050003028009300074040050036703018000";
    let expected = "435269781682571493197834562826195347374682915951743628519326874248957136763418259";
    let grid = parse(&puzzle);
    let solved = solve(grid);
    assert!(solved.is_some(), "Must solve medium puzzle");
    let solution = solved.unwrap();
    let result: String = solution.iter().flat_map(|row| row.iter()).map(|&c| char::from_digit(c as u32, 10).unwrap()).collect();
    assert_eq!(result, expected);
}

// Hard puzzle (minimal givens)
#[test]
fn solves_hard_puzzle() {
    let puzzle = "800000000003600000070090200050007000000045700000100030001000068008500010090000400";
    let expected = "812753649943682175675491283154237896369845721287169534521974368438526917796318452";
    let grid = parse(&puzzle);
    let solved = solve(grid);
    assert!(solved.is_some(), "Must solve hard puzzle");
    let solution = solved.unwrap();
    let result: String = solution.iter().flat_map(|row| row.iter()).map(|&c| char::from_digit(c as u32, 10).unwrap()).collect();
    assert_eq!(result, expected);
}

// Validation
#[test]
fn solved_board_validates() {
    let puzzle = "530070000600195000098000060800060003400803001700020006060000280000419005000080079";
    let grid = parse(&puzzle);
    let solution = solve(grid).unwrap();
    assert!(validate(&solution), "Solved board must validate");
}

#[test]
fn rejects_invalid_puzzle() {
    // Two 5s in the first row = invalid
    let bad = "550070000600195000098000060800060003400803001700020006060000280000419005000080079";
    let grid = parse(&bad);
    let result = solve(grid);
    assert!(result.is_none(), "Must reject invalid puzzles");
}

#[test]
fn empty_board_solvable() {
    let empty = "000000000000000000000000000000000000000000000000000000000000000000000000000000000";
    let grid = parse(&empty);
    let solved = solve(grid);
    assert!(solved.is_some(), "Empty board must be solvable");
    assert!(validate(&solved.unwrap()), "Solution of empty board must validate");
}
