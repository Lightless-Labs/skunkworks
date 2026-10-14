#[test]
fn parse_reads_digits_row_major() {
    let grid = parse("530070000600195000098000060800060003400803001700020006060000280000419005000080079");
    assert_eq!(grid[0][0], 5);
    assert_eq!(grid[0][1], 3);
    assert_eq!(grid[0][2], 0);
    assert_eq!(grid[1][3], 1);
    assert_eq!(grid[8][8], 9);
}

#[test]
fn validate_accepts_complete_solution_and_rejects_duplicate() {
    let mut grid = parse("534678912672195348198342567859761423426853791713924856961537284287419635345286179");
    assert!(validate(&grid));
    grid[0][0] = grid[0][3];
    assert!(!validate(&grid));
}

#[test]
fn solves_a_relabelled_puzzle() {
    let solved = solve(parse("570030000400915000012000040200040007600207009300080004040000820000691005000020031")).expect("puzzle is solvable");
    let digits: String = solved
        .iter()
        .flat_map(|row| row.iter())
        .map(|&d| char::from_digit(d as u32, 10).unwrap())
        .collect();
    assert_eq!(digits, "576432198438915762912768543251349687684257319397186254149573826823691475765824931");
}
