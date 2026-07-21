use crate::{
    CELLS, HEIGHT, WIDTH, coordinates,
    data::{GEAR, GearId, GearKind, MOBS, MobId, SkillId, gear_flags},
    index,
    model::Item,
    pathfinding::{NavigationGrid, Pathfinder},
    world::{Game, Tile},
};

const HASTE: usize = 0;
const GRABBED: usize = 5;

#[derive(Clone, Debug)]
pub enum Action {
    Command(char),
    Fire(usize),
    Throw(u16, usize),
    Eat(u16),
    Use(u16),
    Buy(u16),
    Wield(u16),
    Wear(u16),
    None,
}

impl Action {
    pub fn identity(&self, game: &Game) -> String {
        match self {
            Self::Command(key) => format!("command:{key}"),
            Self::Fire(cell) => {
                let (x, y) = coordinates(*cell);
                format!("fire:{x},{y}")
            }
            Self::Throw(uid, cell) => {
                let (x, y) = coordinates(*cell);
                format!(
                    "throw:{}:{x},{y}",
                    inventory_name(game, *uid).unwrap_or("?")
                )
            }
            Self::Eat(uid) => format!("eat:{}", inventory_name(game, *uid).unwrap_or("?")),
            Self::Use(uid) => format!("use:{}", inventory_name(game, *uid).unwrap_or("?")),
            Self::Buy(uid) => {
                let name = game
                    .shop_room
                    .and_then(|room| game.rooms[room].stock.iter().find(|item| item.uid == *uid))
                    .map_or("?", |item| item.spec().name);
                format!("buy:{name}")
            }
            Self::Wield(uid) => format!("wield:{}", inventory_name(game, *uid).unwrap_or("?")),
            Self::Wear(uid) => format!("wear:{}", inventory_name(game, *uid).unwrap_or("?")),
            Self::None => "none".to_owned(),
        }
    }
}

#[derive(Clone)]
pub struct Bot {
    pathfinder: Pathfinder,
    ignored_items: Vec<u16>,
    cached_target: Option<u16>,
    cached_route: Vec<u16>,
    floor: u8,
    detours: u8,
    active_item: Option<u16>,
    active_kind: Option<GearKind>,
    active_item_steps: u8,
    last_food_route_steps: u8,
    item_target_switches: u8,
    last_position: Option<(u8, u16)>,
    stationary_actions: u8,
    loop_teleports: u8,
    force_depth_steps: u8,
    pending_loop_teleports: u8,
    boss_prep_bounce: bool,
    boss_was_active: bool,
    recent_positions: Vec<u16>,
    recent_hostiles: Vec<u8>,
    route_history: Vec<u16>,
    visit_counts: [u16; CELLS],
    exploration_recent: Vec<u16>,
    fresh_after_loop: bool,
    post_loop_depth: bool,
    floor_shop_purchases: u8,
    loop_poison: Vec<u16>,
    loop_poison_active: bool,
    poison_until: [u32; CELLS],
    under_fire_sidesteps: u8,
    floor11_post_teleport: u8,
    floor13_lookahead_heal: bool,
    combat_loop_break_steps: u8,
    choice_rng_extra: u8,
    choice_rng_skip: u8,
    lookahead_predictions: u16,
    resource_focus: u8,
    survival_focus: bool,
    depth_rush: bool,
    reckless_rush: bool,
    reckless_from_floor: u8,
    reckless_until_floor: u8,
}

impl Default for Bot {
    fn default() -> Self {
        Self {
            pathfinder: Pathfinder::default(),
            ignored_items: Vec::new(),
            cached_target: None,
            cached_route: Vec::with_capacity(96),
            floor: 0,
            detours: 0,
            active_item: None,
            active_kind: None,
            active_item_steps: 0,
            last_food_route_steps: 0,
            item_target_switches: 0,
            last_position: None,
            stationary_actions: 0,
            loop_teleports: 0,
            force_depth_steps: 0,
            pending_loop_teleports: 0,
            boss_prep_bounce: false,
            boss_was_active: false,
            recent_positions: Vec::with_capacity(8),
            recent_hostiles: Vec::with_capacity(8),
            route_history: Vec::with_capacity(16),
            visit_counts: [0; CELLS],
            exploration_recent: Vec::with_capacity(160),
            fresh_after_loop: false,
            post_loop_depth: false,
            floor_shop_purchases: 0,
            loop_poison: Vec::with_capacity(16),
            loop_poison_active: false,
            poison_until: [0; CELLS],
            under_fire_sidesteps: 0,
            floor11_post_teleport: 0,
            floor13_lookahead_heal: false,
            combat_loop_break_steps: 0,
            choice_rng_extra: 0,
            choice_rng_skip: 0,
            lookahead_predictions: 0,
            resource_focus: 0,
            survival_focus: false,
            depth_rush: false,
            reckless_rush: false,
            reckless_from_floor: 0,
            reckless_until_floor: 0,
        }
    }
}

impl Bot {
    pub fn resource_focused() -> Self {
        Self {
            resource_focus: 1,
            ..Self::default()
        }
    }

    pub fn resource_hoarder() -> Self {
        Self {
            resource_focus: 2,
            ..Self::default()
        }
    }

    pub fn resource_survival(level: u8) -> Self {
        Self {
            resource_focus: level,
            survival_focus: true,
            ..Self::default()
        }
    }

    pub fn depth_rush(level: u8) -> Self {
        Self {
            resource_focus: level,
            survival_focus: true,
            depth_rush: true,
            ..Self::default()
        }
    }

    pub fn reckless_rush() -> Self {
        Self {
            survival_focus: true,
            depth_rush: true,
            reckless_rush: true,
            reckless_until_floor: 15,
            ..Self::default()
        }
    }

    pub fn bounded_rush(level: u8, until_floor: u8) -> Self {
        Self {
            resource_focus: level,
            survival_focus: true,
            depth_rush: true,
            reckless_rush: true,
            reckless_until_floor: until_floor,
            ..Self::default()
        }
    }

    pub fn late_rush(level: u8, from_floor: u8) -> Self {
        Self {
            resource_focus: level,
            survival_focus: true,
            depth_rush: true,
            reckless_rush: true,
            reckless_from_floor: from_floor,
            reckless_until_floor: 15,
            ..Self::default()
        }
    }

    pub fn window_rush(level: u8, from_floor: u8, until_floor: u8) -> Self {
        Self {
            resource_focus: level,
            survival_focus: true,
            depth_rush: true,
            reckless_rush: true,
            reckless_from_floor: from_floor,
            reckless_until_floor: until_floor,
            ..Self::default()
        }
    }
}
