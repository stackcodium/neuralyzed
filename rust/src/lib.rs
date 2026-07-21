#![forbid(unsafe_code)]

pub mod bitgrid;
pub mod bot;
pub mod data;
pub mod model;
pub mod pathfinding;
pub mod rng;
pub mod simulation;
pub mod trace;
pub mod world;

pub const WIDTH: usize = 64;
pub const HEIGHT: usize = 36;
pub const CELLS: usize = WIDTH * HEIGHT;

#[inline(always)]
pub const fn index(x: usize, y: usize) -> usize {
    y * WIDTH + x
}

#[inline(always)]
pub const fn coordinates(cell: usize) -> (usize, usize) {
    (cell % WIDTH, cell / WIDTH)
}

pub fn port_status() -> &'static [(&'static str, bool)] {
    &[
        ("golden trace parser", true),
        ("stored-game parser", true),
        ("Park-Miller RNG", true),
        ("packed 64x36 bit grid", true),
        ("straight shortest-path BFS", true),
        ("game data", true),
        ("floor generation", true),
        ("player and inventory", false),
        ("combat and game flow", false),
        ("baseline bot", false),
        ("lookahead bot", false),
        ("full trace equivalence", false),
        ("10x complete-simulation target", false),
    ]
}
