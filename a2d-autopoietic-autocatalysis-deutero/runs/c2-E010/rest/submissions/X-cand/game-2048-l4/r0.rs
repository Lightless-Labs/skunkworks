#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction { Up, Left, Down, Right }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Shape,
    InvalidTile { index: usize, value: u32 },
    MergeOverflow { first: usize, second: usize },
    ScoreOverflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board { pub width: usize, pub height: usize, pub cells: Vec<u32> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merge { pub from: [usize; 2], pub to: usize, pub value: u32 }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveResult { pub board: Board, pub score: u64, pub changed: bool, pub merges: Vec<Merge> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawn { pub index: usize, pub value: u32 }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnResult { pub board: Board, pub rng: u64, pub spawn: Option<Spawn> }

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
pub struct State { pub board: Board, pub score: u64, pub rng: u64 }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game { current: State, previous: Option<State> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command { Move(Direction), Undo, Check }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event { Moved(Turn), Undone(bool), Checked(bool) }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replay { pub state: State, pub can_undo: bool, pub events: Vec<Event> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayError {
    pub index: Option<usize>,
    pub error: Error,
    pub state: State,
    pub can_undo: bool,
    pub events: Vec<Event>,
}

fn validate(board: &Board) -> Result<(), Error> {
    if !(1..=8).contains(&board.width) || !(1..=8).contains(&board.height) {
        return Err(Error::Shape);
    }
    let Some(size) = board.width.checked_mul(board.height) else {
        return Err(Error::Shape);
    };
    if board.cells.len() != size {
        return Err(Error::Shape);
    }
    for (index, &value) in board.cells.iter().enumerate() {
        if value != 0 && (!(2..=2_147_483_648).contains(&value) || !value.is_power_of_two()) {
            return Err(Error::InvalidTile { index, value });
        }
    }
    Ok(())
}

fn line_indices(board: &Board, direction: Direction, line: usize) -> Vec<usize> {
    let mut indices = Vec::with_capacity(match direction {
        Direction::Left | Direction::Right => board.width,
        Direction::Up | Direction::Down => board.height,
    });
    match direction {
        Direction::Left => {
            for x in 0..board.width { indices.push(line * board.width + x); }
        }
        Direction::Right => {
            for x in (0..board.width).rev() { indices.push(line * board.width + x); }
        }
        Direction::Up => {
            for y in 0..board.height { indices.push(y * board.width + line); }
        }
        Direction::Down => {
            for y in (0..board.height).rev() { indices.push(y * board.width + line); }
        }
    }
    indices
}

fn move_validated(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    let mut cells = vec![0; board.cells.len()];
    let mut merges = Vec::new();
    let mut score = 0u64;
    let line_count = match direction {
        Direction::Left | Direction::Right => board.height,
        Direction::Up | Direction::Down => board.width,
    };

    for line in 0..line_count {
        let indices = line_indices(board, direction, line);
        let inputs: Vec<usize> = indices.iter().copied().filter(|&i| board.cells[i] != 0).collect();
        let mut out_pos = 0;
        let mut i = 0;
        while i < inputs.len() {
            let first = inputs[i];
            if i + 1 < inputs.len() && board.cells[first] == board.cells[inputs[i + 1]] {
                let second = inputs[i + 1];
                let value = board.cells[first].checked_mul(2).ok_or(Error::MergeOverflow { first, second })?;
                let to = indices[out_pos];
                cells[to] = value;
                merges.push(Merge { from: [first, second], to, value });
                score = score.checked_add(value as u64).ok_or(Error::ScoreOverflow)?;
                out_pos += 1;
                i += 2;
            } else {
                cells[indices[out_pos]] = board.cells[first];
                out_pos += 1;
                i += 1;
            }
        }
    }

    let changed = cells != board.cells;
    if !changed {
        score = 0;
        merges.clear();
    }
    Ok(MoveResult {
        board: Board { width: board.width, height: board.height, cells },
        score,
        changed,
        merges,
    })
}

pub fn move_board(board: &Board, direction: Direction) -> Result<MoveResult, Error> {
    validate(board)?;
    move_validated(board, direction)
}

pub fn legal_moves(board: &Board) -> Result<Vec<Direction>, Error> {
    validate(board)?;
    let mut result = Vec::new();
    for direction in [Direction::Up, Direction::Left, Direction::Down, Direction::Right] {
        if let Ok(moved) = move_validated(board, direction) {
            if moved.changed { result.push(direction); }
        }
    }
    Ok(result)
}

fn next_rng(x: u64) -> u64 {
    x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407)
}

pub fn spawn_tile(board: &Board, rng: u64) -> Result<SpawnResult, Error> {
    validate(board)?;
    let empties: Vec<usize> = board.cells.iter().enumerate()
        .filter_map(|(i, &v)| if v == 0 { Some(i) } else { None })
        .collect();
    if empties.is_empty() {
        return Ok(SpawnResult { board: board.clone(), rng, spawn: None });
    }
    let a = next_rng(rng);
    let index = empties[(a % empties.len() as u64) as usize];
    let b = next_rng(a);
    let value = if b % 10 == 0 { 4 } else { 2 };
    let mut spawned = board.clone();
    spawned.cells[index] = value;
    Ok(SpawnResult { board: spawned, rng: b, spawn: Some(Spawn { index, value }) })
}

pub fn play_turn(board: &Board, rng: u64, direction: Direction) -> Result<Turn, Error> {
    let moved = move_board(board, direction)?;
    if !moved.changed {
        return Ok(Turn {
            board: moved.board, rng, score: 0, changed: false, merges: Vec::new(), spawn: None,
        });
    }
    let spawned = spawn_tile(&moved.board, rng)?;
    Ok(Turn {
        board: spawned.board, rng: spawned.rng, score: moved.score,
        changed: true, merges: moved.merges, spawn: spawned.spawn,
    })
}

pub fn new_game(initial: State) -> Result<Game, Error> {
    validate(&initial.board)?;
    Ok(Game { current: initial, previous: None })
}

pub fn snapshot(game: &Game) -> State { game.current.clone() }

pub fn step(game: &mut Game, direction: Direction) -> Result<Turn, Error> {
    let moved = move_board(&game.current.board, direction)?;
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
    let cumulative = game.current.score.checked_add(moved.score).ok_or(Error::ScoreOverflow)?;
    let spawned = spawn_tile(&moved.board, game.current.rng)?;
    let old = game.current.clone();
    game.current = State { board: spawned.board.clone(), score: cumulative, rng: spawned.rng };
    game.previous = Some(old);
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
    if let Some(previous) = game.previous.take() {
        game.current = previous;
        true
    } else {
        false
    }
}

pub fn can_undo(game: &Game) -> bool { game.previous.is_some() }

pub fn game_over(game: &Game) -> bool {
    legal_moves(&game.current.board).map_or(true, |moves| moves.is_empty())
}

pub fn replay(initial: State, commands: &[Command]) -> Result<Replay, ReplayError> {
    let mut game = match new_game(initial.clone()) {
        Ok(game) => game,
        Err(error) => return Err(ReplayError {
            index: None, error, state: initial, can_undo: false, events: Vec::new(),
        }),
    };
    let mut events = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        match command {
            Command::Move(direction) => match step(&mut game, *direction) {
                Ok(turn) => events.push(Event::Moved(turn)),
                Err(error) => return Err(ReplayError {
                    index: Some(index), error, state: snapshot(&game),
                    can_undo: can_undo(&game), events,
                }),
            },
            Command::Undo => events.push(Event::Undone(undo(&mut game))),
            Command::Check => events.push(Event::Checked(game_over(&game))),
        }
    }
    Ok(Replay { state: snapshot(&game), can_undo: can_undo(&game), events })
}
