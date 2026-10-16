#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Left,
    Down,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Shape,
    InvalidTile { index: usize, value: u32 },
    MergeOverflow { first: usize, second: usize },
    ScoreOverflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merge {
    pub from: [usize; 2],
    pub to: usize,
    pub value: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveResult {
    pub board: Board,
    pub score: u64,
    pub changed: bool,
    pub merges: Vec<Merge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawn {
    pub index: usize,
    pub value: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnResult {
    pub board: Board,
    pub rng: u64,
    pub spawn: Option<Spawn>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub board: Board,
    pub rng: u64,
    pub score: u64,
    pub changed: bool,
    pub merges: Vec<Merge>,
    pub spawn: Option<Spawn>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub board: Board,
    pub score: u64,
    pub rng: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    current: State,
    undo: Option<State>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Move(Direction),
    Undo,
    Check,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Moved(Turn),
    Undone(bool),
    Checked(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replay {
    pub state: State,
    pub can_undo: bool,
    pub events: Vec<Event>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayError {
    pub index: Option<usize>,
    pub error: Error,
    pub state: State,
    pub can_undo: bool,
    pub events: Vec<Event>,
}

fn validate(board: &Board) -> Result<(), Error> {
    if !(1..=8).contains(&board.width)
        || !(1..=8).contains(&board.height)
        || board.cells.len() != board.width * board.height
    {
        return Err(Error::Shape);
    }

    for (index, &value) in board.cells.iter().enumerate() {
        if value != 0 && (value < 2 || value > 2_147_483_648 || !value.is_power_of_two()) {
            return Err(Error::InvalidTile { index, value });
        }
    }
    Ok(())
}

fn line_indices(board: &Board, direction: Direction, line: usize) -> Vec<usize> {
    match direction {
        Direction::Left => (0..board.width).map(|col| line * board.width + col).collect(),
        Direction::Right => (0..board.width)
            .rev()
            .map(|col| line * board.width + col)
            .collect(),
        Direction::Up => (0..board.height).map(|row| row * board.width + line).collect(),
        Direction::Down => (0..board.height)
            .rev()
            .map(|row| row * board.width + line)
            .collect(),
    }
}

pub fn move_board(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    validate(board)?;

    let mut cells = vec![0; board.cells.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;

    let line_count = match direction {
        Direction::Left | Direction::Right => board.height,
        Direction::Up | Direction::Down => board.width,
    };

    for line in 0..line_count {
        let indices = line_indices(board, direction, line);
        let tiles: Vec<(usize, u32)> = indices
            .iter()
            .filter_map(|&index| {
                let value = board.cells[index];
                (value != 0).then_some((index, value))
            })
            .collect();

        let mut output = Vec::new();
        let mut i = 0;
        while i < tiles.len() {
            let (first_index, value) = tiles[i];
            if i + 1 < tiles.len() && tiles[i + 1].1 == value {
                let second_index = tiles[i + 1].0;
                let Some(merged_value) = value.checked_mul(2) else {
                    return Err(Error::MergeOverflow {
                        first: first_index,
                        second: second_index,
                    });
                };
                score = score
                    .checked_add(u64::from(merged_value))
                    .ok_or(Error::ScoreOverflow)?;
                let to = indices[output.len()];
                output.push(merged_value);
                merges.push(Merge {
                    from: [first_index, second_index],
                    to,
                    value: merged_value,
                });
                i += 2;
            } else {
                output.push(value);
                i += 1;
            }
        }

        for (offset, value) in output.into_iter().enumerate() {
            cells[indices[offset]] = value;
        }
    }

    let changed = cells != board.cells;
    if !changed {
        score = 0;
        merges.clear();
    }

    Ok(MoveResult {
        board: Board {
            width: board.width,
            height: board.height,
            cells,
        },
        score,
        changed,
        merges,
    })
}

pub fn legal_moves(board: &Board) -> Result<Vec<Direction>, Error> {
    validate(board)?;
    let mut legal = Vec::new();
    for direction in [
        Direction::Up,
        Direction::Left,
        Direction::Down,
        Direction::Right,
    ] {
        if matches!(move_board(board, direction), Ok(result) if result.changed) {
            legal.push(direction);
        }
    }
    Ok(legal)
}

fn next_rng(rng: u64) -> u64 {
    rng.wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}

pub fn spawn_tile(board: &Board, rng: u64) -> Result<SpawnResult, Error> {
    validate(board)?;
    let empty: Vec<usize> = board
        .cells
        .iter()
        .enumerate()
        .filter_map(|(index, &value)| (value == 0).then_some(index))
        .collect();

    if empty.is_empty() {
        return Ok(SpawnResult {
            board: board.clone(),
            rng,
            spawn: None,
        });
    }

    let a = next_rng(rng);
    let index = empty[(a % empty.len() as u64) as usize];
    let b = next_rng(a);
    let value = if b % 10 == 0 { 4 } else { 2 };

    let mut spawned_board = board.clone();
    spawned_board.cells[index] = value;

    Ok(SpawnResult {
        board: spawned_board,
        rng: b,
        spawn: Some(Spawn { index, value }),
    })
}

pub fn play_turn(board: &Board, rng: u64, direction: Direction) -> Result<Turn, Error> {
    let moved = move_board(board, direction)?;
    if !moved.changed {
        return Ok(Turn {
            board: moved.board,
            rng,
            score: 0,
            changed: false,
            merges: Vec::new(),
            spawn: None,
        });
    }

    let spawned = spawn_tile(&moved.board, rng)?;
    Ok(Turn {
        board: spawned.board,
        rng: spawned.rng,
        score: moved.score,
        changed: true,
        merges: moved.merges,
        spawn: spawned.spawn,
    })
}

pub fn new_game(initial: State) -> Result<Game, Error> {
    validate(&initial.board)?;
    Ok(Game {
        current: initial,
        undo: None,
    })
}

pub fn snapshot(game: &Game) -> State {
    game.current.clone()
}

pub fn step(game: &mut Game, direction: Direction) -> Result<Turn, Error> {
    let moved = move_board(&game.current.board, direction)?;
    let new_score = game
        .current
        .score
        .checked_add(moved.score)
        .ok_or(Error::ScoreOverflow)?;

    if !moved.changed {
        return Ok(Turn {
            board: moved.board,
            rng: game.current.rng,
            score: 0,
            changed: false,
            merges: Vec::new(),
            spawn: None,
        });
    }

    let spawned = spawn_tile(&moved.board, game.current.rng)?;
    let previous = game.current.clone();
    game.current = State {
        board: spawned.board.clone(),
        score: new_score,
        rng: spawned.rng,
    };
    game.undo = Some(previous);

    Ok(Turn {
        board: spawned.board,
        rng: spawned.rng,
        score: moved.score,
        changed: true,
        merges: moved.merges,
        spawn: spawned.spawn,
    })
}

pub fn undo(game: &mut Game) -> bool {
    if let Some(previous) = game.undo.take() {
        game.current = previous;
        true
    } else {
        false
    }
}

pub fn can_undo(game: &Game) -> bool {
    game.undo.is_some()
}

pub fn game_over(game: &Game) -> bool {
    legal_moves(&game.current.board)
        .map(|directions| directions.is_empty())
        .unwrap_or(true)
}

pub fn replay(initial: State, commands: &[Command]) -> Result<Replay, ReplayError> {
    let mut game = match new_game(initial.clone()) {
        Ok(game) => game,
        Err(error) => {
            return Err(ReplayError {
                index: None,
                error,
                state: initial,
                can_undo: false,
                events: Vec::new(),
            });
        }
    };

    let mut events = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        match command {
            Command::Move(direction) => match step(&mut game, *direction) {
                Ok(turn) => events.push(Event::Moved(turn)),
                Err(error) => {
                    return Err(ReplayError {
                        index: Some(index),
                        error,
                        state: snapshot(&game),
                        can_undo: can_undo(&game),
                        events,
                    });
                }
            },
            Command::Undo => events.push(Event::Undone(undo(&mut game))),
            Command::Check => events.push(Event::Checked(game_over(&game))),
        }
    }

    Ok(Replay {
        state: snapshot(&game),
        can_undo: can_undo(&game),
        events,
    })
}