fn neighbor_cells(cell: usize) -> impl Iterator<Item = usize> {
    let (x, y) = coordinates(cell);
    let mut cells = Vec::with_capacity(8);
    for dy in -1_i8..=1 {
        for dx in -1_i8..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = x as i16 + i16::from(dx);
            let ny = y as i16 + i16::from(dy);
            if nx >= 0 && ny >= 0 && nx < WIDTH as i16 && ny < HEIGHT as i16 {
                cells.push(index(nx as usize, ny as usize));
            }
        }
    }
    cells.into_iter()
}

fn is_unseen_walkable(game: &Game, cell: usize) -> bool {
    !game.seen[cell] && !matches!(game.map[cell], Tile::Wall | Tile::Trap)
}

fn unseen_neighbor_count(game: &Game, cell: usize) -> usize {
    neighbor_cells(cell)
        .filter(|&neighbor| is_unseen_walkable(game, neighbor))
        .count()
}

fn useful_item_neighbor_count(game: &Game, cell: usize) -> usize {
    game.items
        .iter()
        .filter(|item| {
            Game::distance(cell, item.cell as usize) <= 1 && game.is_auto_pickup_candidate(item)
        })
        .count()
}

fn recent_visit_penalty(recent: &[u16], cell: usize, scale: i32) -> i32 {
    recent
        .iter()
        .rposition(|&visited| visited as usize == cell)
        .map_or(0, |at| (recent.len() - at) as i32 * scale)
}

fn is_frontier_cell(game: &Game, grid: &NavigationGrid, cell: usize) -> bool {
    game.seen[cell]
        && !grid.poison.contains(cell)
        && neighbor_cells(cell).any(|neighbor| is_unseen_walkable(game, neighbor))
}

fn frontier_cell_score(game: &Game, visits: &[u16; CELLS], recent: &[u16], cell: usize) -> i32 {
    unseen_neighbor_count(game, cell) as i32 * 18
        + useful_item_neighbor_count(game, cell) as i32 * 12
        - i32::from(visits[cell]) * 10
        - recent_visit_penalty(recent, cell, 5)
}

fn hidden_branch_potential(game: &Game, start: usize) -> i32 {
    let mut queue = [(0_u16, 0_u8); CELLS];
    let mut seen = crate::bitgrid::BitGrid::EMPTY;
    let mut head = 0;
    let mut tail = 0;
    for neighbor in neighbor_cells(start).filter(|&cell| is_unseen_walkable(game, cell)) {
        queue[tail] = (neighbor as u16, 1);
        tail += 1;
    }
    let mut score = 0_i32;
    while head < tail {
        let (cell, depth) = queue[head];
        head += 1;
        let cell = cell as usize;
        if depth > 4 || seen.contains(cell) {
            continue;
        }
        seen.insert(cell);
        score += i32::from(5 - depth);
        for neighbor in neighbor_cells(cell).filter(|&next| is_unseen_walkable(game, next)) {
            if tail < CELLS {
                queue[tail] = (neighbor as u16, depth + 1);
                tail += 1;
            }
        }
    }
    score
}

fn step_action(from: usize, to: usize) -> Action {
    let (fx, fy) = coordinates(from);
    let (tx, ty) = coordinates(to);
    let key = match (tx as isize - fx as isize, ty as isize - fy as isize) {
        (-1, -1) => 'y',
        (0, -1) => 'k',
        (1, -1) => 'u',
        (-1, 0) => 'h',
        (1, 0) => 'l',
        (-1, 1) => 'b',
        (0, 1) => 'j',
        (1, 1) => 'n',
        _ => '.',
    };
    Action::Command(key)
}

fn fresh_detour_step(game: &Game, target: usize, history: &[u16]) -> Option<Action> {
    let (x, y) = coordinates(game.player.cell as usize);
    const DIRECTIONS: [(i8, i8); 8] = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ];
    DIRECTIONS
        .iter()
        .filter_map(|&(dx, dy)| {
            let nx = x as i16 + i16::from(dx);
            let ny = y as i16 + i16::from(dy);
            if nx < 0 || ny < 0 || nx >= WIDTH as i16 || ny >= HEIGHT as i16 {
                return None;
            }
            let cell = index(nx as usize, ny as usize);
            (!game.blocked(cell)
                && !history.contains(&(cell as u16))
                && !game
                    .mobs
                    .iter()
                    .any(|mob| mob.hp > 0 && mob.cell as usize == cell))
            .then_some(cell)
        })
        .min_by_key(|&cell| Game::distance(cell, target))
        .map(|cell| step_action(game.player.cell as usize, cell))
}

fn inventory_name(game: &Game, uid: u16) -> Option<&'static str> {
    game.player
        .inventory
        .iter()
        .find(|item| item.uid == uid)
        .map(|item| GEAR[item.gear as usize].name)
}

fn equipped_item(game: &Game, uid: Option<u16>) -> Option<&Item> {
    game.player
        .inventory
        .iter()
        .find(|item| Some(item.uid) == uid)
}

pub fn run_episode(seed: u64, class: crate::data::ClassId, turn_cap: u32) -> Game {
    let mut game = Game::start(seed, class);
    let mut bot = Bot::default();
    for _ in 0..turn_cap {
        if game.player.dead {
            break;
        }
        let action = bot.choose(&mut game);
        bot.apply(&mut game, action);
    }
    game
}

/// Runs only the lookahead policy. Keep this available for diagnostics and
/// policy tuning; callers that need the strongest completed result should use
/// [`run_episode_ensemble`].
pub fn run_episode_lookahead_raw(seed: u64, class: crate::data::ClassId, turn_cap: u32) -> Game {
    let mut game = Game::start(seed, class);
    let mut bot = Bot::default();
    for _ in 0..turn_cap {
        if game.player.dead {
            break;
        }
        let action = bot.choose_lookahead(&mut game);
        bot.apply(&mut game, action);
    }
    game
}

pub fn run_episode_wide48(seed: u64, class: crate::data::ClassId, turn_cap: u32) -> Game {
    run_episode_wide48_attributed(seed, class, turn_cap).0
}

fn run_episode_wide48_attributed(
    seed: u64,
    class: crate::data::ClassId,
    turn_cap: u32,
) -> (Game, u8) {
    let mut best = run_episode_wide48_raw(seed, class, turn_cap);
    let mut selected = 0;

    // Direct-stairs routing often reaches the boss with fewer incidental kills.
    // Compare it even for fast wins, but retain the guarded completed-episode
    // ranking so a shorter death can never replace a win.
    let direct = run_episode_wide48_variant(seed, class, turn_cap, 9);
    if direct_stairs_is_better(&best, &direct) {
        best = direct;
        selected = 9;
    }
    if best.player.won && best.turns <= 700 {
        return (best, selected);
    }

    // The direct-stairs policy is both a rescue path and a substantial
    // turn-count improvement on otherwise successful games. Always compare it
    // with raw wide48, then spend the larger fallback budget only on raw losses.
    // The class-specific candidates cover the accepted parity and slow-game
    // cases. The remaining variants stay available through WIDE48_VARIANT for
    // focused diagnostics, but do not consume production simulation time.
    let variants = production_wide48_variants(class);
    for &variant in variants.iter().filter(|&&variant| variant != 9) {
        let candidate = run_episode_wide48_variant(seed, class, turn_cap, variant);
        if completed_episode_rank(&candidate) > completed_episode_rank(&best) {
            best = candidate;
            selected = variant;
        }
    }
    (best, selected)
}

fn production_wide48_variants(class: crate::data::ClassId) -> &'static [u8] {
    match class {
        crate::data::ClassId::Agent => &[9, 11, 12, 49],
        crate::data::ClassId::Rookie => &[6, 7, 8, 9, 13, 14, 15, 18, 19, 26, 37, 49, 86],
        crate::data::ClassId::Veteran => &[9, 11, 12, 13, 32, 49, 54, 107],
        crate::data::ClassId::Tech => &[
            3, 4, 6, 9, 12, 13, 16, 18, 22, 24, 31, 37, 41, 49, 54, 66, 67, 84, 107,
        ],
        crate::data::ClassId::Morphed => &[9, 16, 35, 46, 49, 50, 105],
    }
}

pub fn run_episode_wide48_raw(seed: u64, class: crate::data::ClassId, turn_cap: u32) -> Game {
    run_episode_wide48_variant(seed, class, turn_cap, 0)
}

pub fn run_episode_wide48_variant(
    seed: u64,
    class: crate::data::ClassId,
    turn_cap: u32,
    variant: u8,
) -> Game {
    let mut game = Game::start(seed, class);
    let mut bot = wide48_variant_bot(variant);
    for _ in 0..turn_cap {
        if game.player.dead {
            break;
        }
        let action = if (97..=104).contains(&variant) {
            let limits = [32, 48, 64, 80, 96, 120, 150, 180];
            bot.choose_lookahead_wide48_limited(&mut game, limits[(variant - 97) as usize])
        } else if variant == 9 {
            bot.choose_lookahead_reckless(&mut game)
        } else {
            bot.choose_lookahead_wide48(&mut game)
        };
        bot.apply(&mut game, action);
    }
    game
}

fn wide48_variant_bot(variant: u8) -> Bot {
    match variant {
        1 => Bot::resource_focused(),
        2 => Bot::resource_hoarder(),
        3 => Bot::resource_survival(0),
        4 => Bot::resource_survival(1),
        5 => Bot::resource_survival(2),
        6 => Bot::depth_rush(0),
        7 => Bot::depth_rush(1),
        8 => Bot::depth_rush(2),
        9 => Bot::reckless_rush(),
        10 => Bot::bounded_rush(0, 5),
        11 => Bot::bounded_rush(0, 10),
        12 => Bot::bounded_rush(0, 12),
        13 => Bot::bounded_rush(0, 14),
        14 => Bot::bounded_rush(1, 5),
        15 => Bot::bounded_rush(1, 10),
        16 => Bot::bounded_rush(1, 12),
        17 => Bot::bounded_rush(1, 14),
        18 => Bot::bounded_rush(2, 5),
        19 => Bot::bounded_rush(2, 10),
        20 => Bot::bounded_rush(2, 12),
        21 => Bot::bounded_rush(2, 14),
        22 => Bot::bounded_rush(0, 2),
        23 => Bot::bounded_rush(0, 3),
        24 => Bot::bounded_rush(0, 4),
        25 => Bot::bounded_rush(1, 2),
        26 => Bot::bounded_rush(1, 3),
        27 => Bot::bounded_rush(1, 4),
        28 => Bot::bounded_rush(2, 2),
        29 => Bot::bounded_rush(2, 3),
        30 => Bot::bounded_rush(2, 4),
        31 => Bot::bounded_rush(0, 6),
        32 => Bot::bounded_rush(0, 7),
        33 => Bot::bounded_rush(0, 8),
        34 => Bot::bounded_rush(0, 9),
        35 => Bot::bounded_rush(0, 11),
        36 => Bot::bounded_rush(0, 13),
        37 => Bot::bounded_rush(1, 6),
        38 => Bot::bounded_rush(1, 7),
        39 => Bot::bounded_rush(1, 8),
        40 => Bot::bounded_rush(1, 9),
        41 => Bot::bounded_rush(1, 11),
        42 => Bot::bounded_rush(1, 13),
        43 => Bot::bounded_rush(2, 6),
        44 => Bot::bounded_rush(2, 7),
        45 => Bot::bounded_rush(2, 8),
        46 => Bot::bounded_rush(2, 9),
        47 => Bot::bounded_rush(2, 11),
        48 => Bot::bounded_rush(2, 13),
        49 => Bot::late_rush(0, 5),
        50 => Bot::late_rush(0, 10),
        51 => Bot::late_rush(0, 12),
        52 => Bot::late_rush(0, 13),
        53 => Bot::late_rush(0, 14),
        54 => Bot::late_rush(1, 5),
        55 => Bot::late_rush(1, 10),
        56 => Bot::late_rush(1, 12),
        57 => Bot::late_rush(1, 13),
        58 => Bot::late_rush(1, 14),
        59 => Bot::late_rush(2, 5),
        60 => Bot::late_rush(2, 10),
        61 => Bot::late_rush(2, 12),
        62 => Bot::late_rush(2, 13),
        63 => Bot::late_rush(2, 14),
        64 => Bot::late_rush(0, 6),
        65 => Bot::late_rush(0, 7),
        66 => Bot::late_rush(0, 8),
        67 => Bot::late_rush(0, 9),
        68 => Bot::late_rush(0, 11),
        69 => Bot::late_rush(0, 13),
        70 => Bot::late_rush(1, 6),
        71 => Bot::late_rush(1, 7),
        72 => Bot::late_rush(1, 8),
        73 => Bot::late_rush(1, 9),
        74 => Bot::late_rush(1, 11),
        75 => Bot::late_rush(1, 13),
        76 => Bot::late_rush(2, 6),
        77 => Bot::late_rush(2, 7),
        78 => Bot::late_rush(2, 8),
        79 => Bot::late_rush(2, 9),
        80 => Bot::late_rush(2, 11),
        81 => Bot::late_rush(2, 13),
        82 => Bot::window_rush(0, 5, 10),
        83 => Bot::window_rush(0, 5, 12),
        84 => Bot::window_rush(0, 5, 14),
        85 => Bot::window_rush(0, 10, 12),
        86 => Bot::window_rush(0, 10, 14),
        87 => Bot::window_rush(1, 5, 10),
        88 => Bot::window_rush(1, 5, 12),
        89 => Bot::window_rush(1, 5, 14),
        90 => Bot::window_rush(1, 10, 12),
        91 => Bot::window_rush(1, 10, 14),
        92 => Bot::window_rush(2, 5, 10),
        93 => Bot::window_rush(2, 5, 12),
        94 => Bot::window_rush(2, 5, 14),
        95 => Bot::window_rush(2, 10, 12),
        96 => Bot::window_rush(2, 10, 14),
        97..=104 => Bot::late_rush(2, 5),
        105 => Bot::late_rush(0, 2),
        106 => Bot::late_rush(0, 3),
        107 => Bot::late_rush(0, 4),
        108 => Bot::late_rush(1, 2),
        109 => Bot::late_rush(1, 3),
        110 => Bot::late_rush(1, 4),
        111 => Bot::late_rush(2, 2),
        112 => Bot::late_rush(2, 3),
        113 => Bot::late_rush(2, 4),
        _ => Bot::default(),
    }
}

fn recorded_episode(
    seed: u64,
    class: crate::data::ClassId,
    turn_cap: u32,
    mut bot: Bot,
    chooser: u8,
    variant: u8,
) -> (Game, Vec<(Game, Action)>) {
    let mut game = Game::start(seed, class);
    let mut frames = Vec::new();
    for _ in 0..turn_cap {
        if game.player.dead {
            break;
        }
        let action = match chooser {
            0 => bot.choose(&mut game),
            1 => bot.choose_lookahead(&mut game),
            _ if (97..=104).contains(&variant) => {
                let limits = [32, 48, 64, 80, 96, 120, 150, 180];
                bot.choose_lookahead_wide48_limited(&mut game, limits[(variant - 97) as usize])
            }
            _ if variant == 9 => bot.choose_lookahead_reckless(&mut game),
            _ => bot.choose_lookahead_wide48(&mut game),
        };
        bot.apply(&mut game, action.clone());
        frames.push((game.clone(), action));
    }
    (game, frames)
}

fn recorded_episode_from(
    initial: &Game,
    turn_cap: u32,
    mut bot: Bot,
    chooser: u8,
    variant: u8,
) -> (Game, Vec<(Game, Action)>) {
    let mut game = initial.clone();
    let mut frames = Vec::new();
    for _ in 0..turn_cap {
        if game.player.won || game.player.dead {
            break;
        }
        let action = match chooser {
            0 => bot.choose(&mut game),
            1 => bot.choose_lookahead(&mut game),
            _ if (97..=104).contains(&variant) => {
                let limits = [32, 48, 64, 80, 96, 120, 150, 180];
                bot.choose_lookahead_wide48_limited(&mut game, limits[(variant - 97) as usize])
            }
            _ if variant == 9 => bot.choose_lookahead_reckless(&mut game),
            _ => bot.choose_lookahead_wide48(&mut game),
        };
        bot.apply(&mut game, action.clone());
        frames.push((game.clone(), action));
    }
    (game, frames)
}

fn episode_from(initial: &Game, turn_cap: u32, mut bot: Bot, chooser: u8, variant: u8) -> Game {
    let mut game = initial.clone();
    for _ in 0..turn_cap {
        if game.player.won || game.player.dead {
            break;
        }
        let action = match chooser {
            0 => bot.choose(&mut game),
            1 => bot.choose_lookahead(&mut game),
            _ if (97..=104).contains(&variant) => {
                let limits = [32, 48, 64, 80, 96, 120, 150, 180];
                bot.choose_lookahead_wide48_limited(&mut game, limits[(variant - 97) as usize])
            }
            _ if variant == 9 => bot.choose_lookahead_reckless(&mut game),
            _ => bot.choose_lookahead_wide48(&mut game),
        };
        bot.apply(&mut game, action);
    }
    game
}

/// Builds a new guarded ensemble trajectory from an arbitrary live state.
/// The caller owns the authoritative game and may discard this plan whenever a
/// human action changes it. The live bot is cloned so its navigation memory is
/// retained by the baseline and lookahead branches.
pub fn plan_ensemble_from_state(
    initial: &Game,
    live_bot: &Bot,
    class: crate::data::ClassId,
    turn_cap: u32,
) -> (Vec<(Game, Action)>, String) {
    plan_ensemble_from_state_with_workers(initial, live_bot, class, turn_cap, 1)
}

type EnsembleCandidate = (String, Bot, u8, u8);
type EnsembleResult = (Game, Vec<(Game, Action)>, String);

fn ensemble_candidates(class: crate::data::ClassId, live_bot: &Bot) -> Vec<EnsembleCandidate> {
    let mut candidates = vec![
        ("baseline".to_owned(), live_bot.clone(), 0_u8, 0_u8),
        ("raw-lookahead".to_owned(), live_bot.clone(), 1, 0),
        ("resource".to_owned(), Bot::resource_focused(), 1, 0),
        ("hoarder".to_owned(), Bot::resource_hoarder(), 1, 0),
        ("survival-raw".to_owned(), Bot::resource_survival(0), 1, 0),
        (
            "survival-focused".to_owned(),
            Bot::resource_survival(1),
            1,
            0,
        ),
        (
            "survival-strong".to_owned(),
            Bot::resource_survival(2),
            1,
            0,
        ),
        ("wide48-0".to_owned(), live_bot.clone(), 2, 0),
    ];
    candidates.extend(production_wide48_variants(class).iter().map(|&variant| {
        (
            format!("wide48-{variant}"),
            wide48_variant_bot(variant),
            2,
            variant,
        )
    }));
    candidates
}

fn evaluate_ensemble_candidate(
    initial: &Game,
    turn_cap: u32,
    candidate: EnsembleCandidate,
) -> EnsembleResult {
    let (name, bot, chooser, variant) = candidate;
    let (game, frames) = recorded_episode_from(initial, turn_cap, bot, chooser, variant);
    (game, frames, name)
}

pub fn ensemble_candidate_count(class: crate::data::ClassId) -> usize {
    ensemble_candidates(class, &Bot::default()).len()
}

pub fn evaluate_ensemble_candidate_from_state(
    initial: &Game,
    live_bot: &Bot,
    class: crate::data::ClassId,
    turn_cap: u32,
    index: usize,
) -> Option<(Game, String)> {
    let (name, bot, chooser, variant) = ensemble_candidates(class, live_bot)
        .into_iter()
        .nth(index)?;
    let game = episode_from(initial, turn_cap, bot, chooser, variant);
    Some((game, name))
}

pub fn plan_ensemble_candidate_from_state(
    initial: &Game,
    live_bot: &Bot,
    class: crate::data::ClassId,
    turn_cap: u32,
    index: usize,
) -> Option<(Vec<(Game, Action)>, String)> {
    let candidate = ensemble_candidates(class, live_bot)
        .into_iter()
        .nth(index)?;
    let (_, frames, name) = evaluate_ensemble_candidate(initial, turn_cap, candidate);
    Some((frames, name))
}

fn select_ensemble_result(
    results: impl IntoIterator<Item = EnsembleResult>,
) -> (Vec<(Game, Action)>, String) {
    let mut best: Option<EnsembleResult> = None;
    for result in results {
        if best.as_ref().is_none_or(|(current, _, _)| {
            completed_episode_rank(&result.0) > completed_episode_rank(current)
        }) {
            best = Some(result);
        }
    }
    best.map_or_else(
        || (Vec::new(), "none".to_owned()),
        |(_, frames, name)| (frames, name),
    )
}

/// Evaluates independent policy trajectories concurrently on native targets.
/// Results retain candidate order before ranking, preserving deterministic tie
/// behavior. WASM callers remain serial because browser workers, rather than
/// unavailable shared-memory Rust threads, own browser parallelism.
pub fn plan_ensemble_from_state_with_workers(
    initial: &Game,
    live_bot: &Bot,
    class: crate::data::ClassId,
    turn_cap: u32,
    workers: usize,
) -> (Vec<(Game, Action)>, String) {
    plan_ensemble_level_from_state_with_workers(
        initial,
        live_bot,
        class,
        turn_cap,
        workers,
        "strongest",
    )
}

pub fn plan_ensemble_level_from_state_with_workers(
    initial: &Game,
    live_bot: &Bot,
    class: crate::data::ClassId,
    turn_cap: u32,
    workers: usize,
    strategy: &str,
) -> (Vec<(Game, Action)>, String) {
    let mut candidates = ensemble_candidates(class, live_bot);
    match strategy {
        "baseline" => candidates.truncate(1),
        "balanced" => candidates.truncate(7),
        _ => {}
    }
    #[cfg(not(target_arch = "wasm32"))]
    let worker_count = workers
        .max(1)
        .min(planner_physical_core_limit())
        .min(candidates.len());
    #[cfg(target_arch = "wasm32")]
    let worker_count = 1;
    #[cfg(target_arch = "wasm32")]
    let _ = workers;
    if worker_count == 1 {
        return select_ensemble_result(
            candidates
                .into_iter()
                .map(|candidate| evaluate_ensemble_candidate(initial, turn_cap, candidate)),
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut indexed = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(worker_count);
            let candidates = &candidates;
            for worker in 0..worker_count {
                handles.push(scope.spawn(move || {
                    let mut results = Vec::new();
                    for index in (worker..candidates.len()).step_by(worker_count) {
                        results.push((
                            index,
                            evaluate_ensemble_candidate(
                                initial,
                                turn_cap,
                                candidates[index].clone(),
                            ),
                        ));
                    }
                    results
                }));
            }
            handles
                .into_iter()
                .flat_map(|handle| handle.join().expect("planner worker panicked"))
                .collect::<Vec<_>>()
        });
        indexed.sort_unstable_by_key(|(index, _)| *index);
        return select_ensemble_result(indexed.into_iter().map(|(_, result)| result));
    }

    #[cfg(target_arch = "wasm32")]
    {
        select_ensemble_result(
            candidates
                .into_iter()
                .map(|candidate| evaluate_ensemble_candidate(initial, turn_cap, candidate)),
        )
    }
}

/// Maximum native planner concurrency in physical CPU cores, not SMT threads.
#[cfg(not(target_arch = "wasm32"))]
pub fn planner_physical_core_limit() -> usize {
    let logical = std::thread::available_parallelism().map_or(1, usize::from);
    #[cfg(target_os = "linux")]
    {
        use std::collections::HashSet;
        let mut cores = HashSet::new();
        if let Ok(entries) = std::fs::read_dir("/sys/devices/system/cpu") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.strip_prefix("cpu").is_none_or(|suffix| {
                    suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_digit())
                }) {
                    continue;
                }
                let topology = entry.path().join("topology");
                let package = std::fs::read_to_string(topology.join("physical_package_id"));
                let core = std::fs::read_to_string(topology.join("core_id"));
                if let (Ok(package), Ok(core)) = (package, core) {
                    cores.insert((package.trim().to_owned(), core.trim().to_owned()));
                }
            }
        }
        if !cores.is_empty() {
            return cores.len().min(logical).max(1);
        }
    }
    logical.div_ceil(2).max(1)
}

/// Returns the exact state/action stream selected by the guarded episode bot.
/// Native renderers consume these snapshots instead of duplicating policy
/// selection or applying actions with a different bot state.
pub fn run_episode_ensemble_frames(
    seed: u64,
    class: crate::data::ClassId,
    turn_cap: u32,
) -> Vec<(Game, Action)> {
    let expected = run_episode_ensemble(seed, class, turn_cap);
    let matches = |game: &Game| {
        game.player.won == expected.player.won
            && game.player.deepest == expected.player.deepest
            && game.turns == expected.turns
            && game.score() == expected.score()
    };
    let candidates = [
        (Bot::default(), 0_u8, 0_u8),
        (Bot::default(), 1, 0),
        (Bot::resource_focused(), 1, 0),
        (Bot::resource_hoarder(), 1, 0),
        (Bot::resource_survival(0), 1, 0),
        (Bot::resource_survival(1), 1, 0),
        (Bot::resource_survival(2), 1, 0),
        (Bot::default(), 2, 0),
    ];
    for (bot, chooser, variant) in candidates {
        let (game, frames) = recorded_episode(seed, class, turn_cap, bot, chooser, variant);
        if matches(&game) {
            return frames;
        }
    }
    for &variant in production_wide48_variants(class) {
        let (game, frames) = recorded_episode(
            seed,
            class,
            turn_cap,
            wide48_variant_bot(variant),
            2,
            variant,
        );
        if matches(&game) {
            return frames;
        }
    }
    Vec::new()
}

fn run_episode_resource_survival(
    seed: u64,
    class: crate::data::ClassId,
    turn_cap: u32,
    level: u8,
) -> Game {
    let mut game = Game::start(seed, class);
    let mut bot = Bot::resource_survival(level);
    for _ in 0..turn_cap {
        if game.player.dead {
            break;
        }
        let action = bot.choose_lookahead(&mut game);
        bot.apply(&mut game, action);
    }
    game
}

fn run_episode_resource_focused(seed: u64, class: crate::data::ClassId, turn_cap: u32) -> Game {
    let mut game = Game::start(seed, class);
    let mut bot = Bot::resource_focused();
    for _ in 0..turn_cap {
        if game.player.dead {
            break;
        }
        let action = bot.choose_lookahead(&mut game);
        bot.apply(&mut game, action);
    }
    game
}

fn completed_episode_rank(game: &Game) -> (bool, u8, i64, u32) {
    let primary = if game.player.won {
        i64::from(u32::MAX - game.turns)
    } else {
        i64::from(game.score())
    };
    (game.player.won, game.player.deepest, primary, game.score())
}

/// Runs the accepted independent candidates and returns the strongest result.
pub fn run_episode_ensemble(seed: u64, class: crate::data::ClassId, turn_cap: u32) -> Game {
    run_episode_ensemble_attributed(seed, class, turn_cap).0
}

pub fn run_episode_ensemble_attributed(
    seed: u64,
    class: crate::data::ClassId,
    turn_cap: u32,
) -> (Game, String) {
    let lookahead = run_episode_lookahead_raw(seed, class, turn_cap);
    let baseline = run_episode(seed, class, turn_cap);
    let (mut best, mut selected) =
        if completed_episode_rank(&baseline) > completed_episode_rank(&lookahead) {
            (baseline, "baseline".to_owned())
        } else {
            (lookahead, "raw-lookahead".to_owned())
        };
    if best.player.won {
        if best.turns <= 700 {
            return prefer_direct_stairs(seed, class, turn_cap, best, selected);
        }
        let (wide48, variant) = run_episode_wide48_attributed(seed, class, turn_cap);
        return if completed_episode_rank(&wide48) > completed_episode_rank(&best) {
            (wide48, format!("wide48-{variant}"))
        } else {
            (best, selected)
        };
    }

    let resource = run_episode_resource_focused(seed, class, turn_cap);
    if completed_episode_rank(&resource) > completed_episode_rank(&best) {
        best = resource;
        selected = "resource".to_owned();
    }
    if best.player.won && best.turns <= 700 {
        return prefer_direct_stairs(seed, class, turn_cap, best, selected);
    }

    let raw_survival = run_episode_resource_survival(seed, class, turn_cap, 0);
    if completed_episode_rank(&raw_survival) > completed_episode_rank(&best) {
        best = raw_survival;
        selected = "survival-raw".to_owned();
    }
    if best.player.won && best.turns <= 700 {
        return prefer_direct_stairs(seed, class, turn_cap, best, selected);
    }
    let focused_survival = run_episode_resource_survival(seed, class, turn_cap, 1);
    if completed_episode_rank(&focused_survival) > completed_episode_rank(&best) {
        best = focused_survival;
        selected = "survival-focused".to_owned();
    }
    if best.player.won && best.turns <= 700 {
        return prefer_direct_stairs(seed, class, turn_cap, best, selected);
    }
    let (wide48, variant) = run_episode_wide48_attributed(seed, class, turn_cap);
    if completed_episode_rank(&wide48) > completed_episode_rank(&best) {
        (wide48, format!("wide48-{variant}"))
    } else {
        (best, selected)
    }
}

fn prefer_direct_stairs(
    seed: u64,
    class: crate::data::ClassId,
    turn_cap: u32,
    best: Game,
    selected: String,
) -> (Game, String) {
    let direct = run_episode_wide48_variant(seed, class, turn_cap, 9);
    if direct_stairs_is_better(&best, &direct) {
        (direct, "wide48-9".to_owned())
    } else {
        (best, selected)
    }
}

fn direct_stairs_is_better(best: &Game, direct: &Game) -> bool {
    completed_episode_rank(direct) > completed_episode_rank(best)
}

#[cfg(test)]
mod direct_stairs_tests {
    use super::*;
    use crate::data::ClassId;

    #[test]
    fn guarded_direct_stairs_keeps_only_a_better_win() {
        let mut best = Game::start(1_705_700, ClassId::Morphed);
        best.player.won = true;
        best.player.deepest = 15;
        best.turns = 694;
        let mut direct = best.clone();
        direct.turns = 413;

        assert!(direct_stairs_is_better(&best, &direct));
    }

    #[test]
    fn guarded_direct_stairs_cannot_replace_a_win_with_a_loss() {
        let mut best = Game::start(1_705_701, ClassId::Tech);
        best.player.won = true;
        best.player.deepest = 15;
        best.turns = 414;
        let mut direct = best.clone();
        direct.player.won = false;
        direct.player.dead = false;
        direct.player.deepest = 9;
        direct.turns = 3_600;

        assert!(!direct_stairs_is_better(&best, &direct));
    }
}

pub struct EpisodeDiagnostics {
    pub game: Game,
    pub floor_actions: [u32; 16],
    pub floor_entries: [u16; 16],
    pub floor_flow: [FloorFlow; 16],
    pub backtracks: u32,
    pub stationary: u32,
    pub uses: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FloorFlow {
    pub entry_turn: u32,
    pub entry_hp: i16,
    pub max_hp: i16,
    pub min_hp: i16,
    pub entry_weapon: i32,
    pub entry_ammo: u32,
    pub entry_heals: u32,
    pub entry_control: u32,
    pub moves: u32,
    pub fires: u32,
    pub throws: u32,
    pub eats: u32,
    pub tools: u32,
    pub gear: u32,
    pub stationary: u32,
    pub exit_weapon: i32,
    pub exit_ammo: u32,
    pub exit_heals: u32,
    pub exit_control: u32,
}

fn record_flow_state(flow: &mut FloorFlow, game: &Game, entering: bool) {
    let weapon = current_weapon_score(game);
    let ammo = matching_ammo_count(game);
    let heals = healing_count(game);
    let control = boss_control_count(game);
    if entering && flow.max_hp == 0 {
        flow.entry_turn = game.turns;
        flow.entry_hp = game.player.hp;
        flow.max_hp = game.player.max_hp;
        flow.min_hp = game.player.hp;
        flow.entry_weapon = weapon;
        flow.entry_ammo = ammo;
        flow.entry_heals = heals;
        flow.entry_control = control;
    }
    flow.min_hp = flow.min_hp.min(game.player.hp);
    flow.exit_weapon = weapon;
    flow.exit_ammo = ammo;
    flow.exit_heals = heals;
    flow.exit_control = control;
}

pub fn run_episode_lookahead_diagnostics(
    seed: u64,
    class: crate::data::ClassId,
    turn_cap: u32,
) -> EpisodeDiagnostics {
    let mut game = Game::start(seed, class);
    let diagnostic_variant = std::env::var("WIDE48_DIAG_VARIANT")
        .ok()
        .and_then(|value| value.parse::<u8>().ok());
    let mut bot = diagnostic_variant
        .map(wide48_variant_bot)
        .unwrap_or_default();
    let mut floor_actions = [0_u32; 16];
    let mut floor_entries = [0_u16; 16];
    let mut floor_flow = [FloorFlow::default(); 16];
    floor_entries[game.floor as usize] = 1;
    record_flow_state(&mut floor_flow[game.floor as usize], &game, true);
    let mut previous = None;
    let mut previous_previous = None;
    let mut backtracks = 0_u32;
    let mut stationary = 0_u32;
    let mut uses = 0_u32;
    for _ in 0..turn_cap {
        if game.player.dead {
            break;
        }
        let before = (game.floor, game.player.cell);
        floor_actions[game.floor as usize] += 1;
        let action = if diagnostic_variant.is_some() {
            bot.choose_lookahead_wide48(&mut game)
        } else {
            bot.choose_lookahead(&mut game)
        };
        match action {
            Action::Command(_) => floor_flow[game.floor as usize].moves += 1,
            Action::Fire(_) => floor_flow[game.floor as usize].fires += 1,
            Action::Throw(_, _) => floor_flow[game.floor as usize].throws += 1,
            Action::Eat(_) => floor_flow[game.floor as usize].eats += 1,
            Action::Use(_) => floor_flow[game.floor as usize].tools += 1,
            Action::Buy(_) | Action::Wield(_) | Action::Wear(_) => {
                floor_flow[game.floor as usize].gear += 1;
            }
            Action::None => {}
        }
        uses += u32::from(matches!(action, Action::Use(_) | Action::Throw(_, _)));
        bot.apply(&mut game, action);
        let after = (game.floor, game.player.cell);
        let stayed = u32::from(after == before);
        stationary += stayed;
        floor_flow[before.0 as usize].stationary += stayed;
        record_flow_state(&mut floor_flow[before.0 as usize], &game, false);
        backtracks += u32::from(previous_previous == Some(after) && previous != Some(after));
        if game.floor != before.0 {
            floor_entries[game.floor as usize] += 1;
            record_flow_state(&mut floor_flow[game.floor as usize], &game, true);
        }
        previous_previous = previous;
        previous = Some(after);
    }
    EpisodeDiagnostics {
        game,
        floor_actions,
        floor_entries,
        floor_flow,
        backtracks,
        stationary,
        uses,
    }
}
