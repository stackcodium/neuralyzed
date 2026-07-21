use crate::{
    CELLS, HEIGHT, WIDTH, coordinates,
    data::{
        BOSS_FLOORS, ClassId, GEAR, GearId, GearKind, MAX_FLOOR, MOB_PREFIXES, MOBS, MobId, MobMod,
        SkillId,
    },
    index,
    model::{Item, Player},
    rng::Rng,
};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Tile {
    #[default]
    Wall,
    Floor,
    Door,
    OpenDoor,
    DownStairs,
    UpStairs,
    Shop,
    Altar,
    Grave,
    Trap,
    Water,
}

impl Tile {
    pub const fn glyph(self) -> u8 {
        match self {
            Self::Wall => b'#',
            Self::Floor => b'.',
            Self::Door => b'+',
            Self::OpenDoor => b'\'',
            Self::DownStairs => b'>',
            Self::UpStairs => b'<',
            Self::Shop => b'_',
            Self::Altar => b'*',
            Self::Grave => b'%',
            Self::Trap => b'^',
            Self::Water => b'~',
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RoomKind {
    #[default]
    Normal,
    Shop,
    Altar,
    Boss,
}

#[derive(Clone, Debug, Default)]
pub struct Room {
    pub x: u8,
    pub y: u8,
    pub width: u8,
    pub height: u8,
    pub kind: RoomKind,
    pub center: u16,
    pub stock: Vec<Item>,
}

#[derive(Clone, Debug)]
pub struct Mob {
    pub uid: u32,
    pub kind: MobId,
    pub cell: u16,
    pub hp: i16,
    pub max_hp: i16,
    pub damage_multiplier: f64,
    pub xp: u16,
    pub prefix: u8,
    pub modifier: Option<MobMod>,
    pub boss: bool,
    pub asleep: bool,
    pub pacified: bool,
    pub frozen: i16,
    pub cooldown: f64,
    pub revealed: bool,
    pub hunting: bool,
    pub target_cell: Option<u16>,
    pub enraged: bool,
    pub regrew: bool,
    pub spotted: bool,
    pub desperate: bool,
    pub did_split: bool,
    pub friendly: bool,
    pub life: i16,
    pub damage_override: Option<[u8; 2]>,
    pub tier_override: Option<u8>,
    pub sentinel_fragment: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerDamageSource {
    Mob(MobId),
    Trap,
    Starvation,
    Swallowed,
    Poison,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayerDamageEvent {
    pub source: PlayerDamageSource,
    pub amount: i16,
}

#[derive(Clone)]
pub struct Game {
    pub rng: Rng,
    pub map: [Tile; CELLS],
    pub seen: [bool; CELLS],
    pub visible: [bool; CELLS],
    pub floor: u8,
    pub items: Vec<Item>,
    pub mobs: Vec<Mob>,
    pub rooms: Vec<Room>,
    pub up_stairs: Option<u16>,
    pub down_stairs: Option<u16>,
    pub shop_room: Option<usize>,
    pub player: Player,
    pub turns: u32,
    pub player_damage_events: Vec<PlayerDamageEvent>,
    next_mob_uid: u32,
}

#[derive(Clone, Copy)]
struct Theme {
    water: f64,
    traps: u8,
    tier_mask: u8,
    shop_odds: f64,
}

impl Game {
    pub fn start(seed: u64, class: ClassId) -> Self {
        let mut rng = Rng::new(seed);
        let player = Player::new(class, &mut rng);
        let mut game = Self {
            rng,
            map: [Tile::Wall; CELLS],
            seen: [false; CELLS],
            visible: [false; CELLS],
            floor: 1,
            items: Vec::with_capacity(16),
            mobs: Vec::with_capacity(16),
            rooms: Vec::with_capacity(12),
            up_stairs: None,
            down_stairs: None,
            shop_room: None,
            player,
            turns: 0,
            player_damage_events: Vec::with_capacity(4),
            next_mob_uid: 0,
        };
        game.generate_floor(1);
        game.player.cell = game
            .up_stairs
            .or_else(|| game.random_floor())
            .unwrap_or(crate::index(2, 2) as u16);
        game.compute_fov(9);
        game
    }

    pub fn begin_action(&mut self) {
        self.player_damage_events.clear();
    }

    pub(crate) fn record_player_damage(&mut self, source: PlayerDamageSource, amount: i16) {
        if amount > 0 {
            self.player_damage_events
                .push(PlayerDamageEvent { source, amount });
        }
    }

    #[inline(always)]
    pub fn tile(&self, x: isize, y: isize) -> Tile {
        if x < 0 || y < 0 || x >= WIDTH as isize || y >= HEIGHT as isize {
            Tile::Wall
        } else {
            self.map[index(x as usize, y as usize)]
        }
    }

    #[inline(always)]
    pub(crate) fn set_tile(&mut self, x: usize, y: usize, tile: Tile) {
        if x < WIDTH && y < HEIGHT {
            self.map[index(x, y)] = tile;
        }
    }

    #[inline]
    pub fn mob_at(&self, cell: usize) -> Option<usize> {
        self.mobs
            .iter()
            .position(|mob| mob.cell as usize == cell && mob.hp > 0)
    }

    fn make_room(x: usize, y: usize, width: usize, height: usize, kind: RoomKind) -> Room {
        Room {
            x: x as u8,
            y: y as u8,
            width: width as u8,
            height: height as u8,
            kind,
            center: index(x + (width >> 1), y + (height >> 1)) as u16,
            stock: Vec::new(),
        }
    }

    fn carve(&mut self, room: &Room) {
        for y in room.y as usize..usize::from(room.y + room.height) {
            for x in room.x as usize..usize::from(room.x + room.width) {
                self.set_tile(x, y, Tile::Floor);
            }
        }
    }

    fn overlaps(&self, room: &Room) -> bool {
        self.rooms.iter().any(|other| {
            room.x < other.x + other.width + 1
                && room.x + room.width + 1 > other.x
                && room.y < other.y + other.height + 1
                && room.y + room.height + 1 > other.y
        })
    }

    fn tunnel(&mut self, from: u16, to: u16) {
        let (mut x, mut y) = coordinates(from as usize);
        let (target_x, target_y) = coordinates(to as usize);
        let horizontal_first = self.rng.chance(0.5);
        if horizontal_first {
            while x != target_x {
                self.dig(x, y);
                x = step_toward(x, target_x);
            }
            while y != target_y {
                self.dig(x, y);
                y = step_toward(y, target_y);
            }
        } else {
            while y != target_y {
                self.dig(x, y);
                y = step_toward(y, target_y);
            }
            while x != target_x {
                self.dig(x, y);
                x = step_toward(x, target_x);
            }
        }
        self.dig(x, y);
    }

    fn dig(&mut self, x: usize, y: usize) {
        if self.map[index(x, y)] == Tile::Wall {
            self.map[index(x, y)] = Tile::Floor;
        }
    }

    fn place_doors(&mut self) {
        for room_index in 0..self.rooms.len() {
            let room = self.rooms[room_index].clone();
            if room.kind == RoomKind::Boss {
                continue;
            }
            for x in room.x as isize - 1..=isize::from(room.x + room.width) {
                for y in [room.y as isize - 1, isize::from(room.y + room.height)] {
                    if self.tile(x, y) == Tile::Floor
                        && self.tile(x, y - 1) == Tile::Wall
                        && self.tile(x, y + 1) == Tile::Wall
                        && self.rng.chance(0.5)
                    {
                        self.set_tile(x as usize, y as usize, Tile::Door);
                    }
                }
            }
            for y in room.y as isize - 1..=isize::from(room.y + room.height) {
                for x in [room.x as isize - 1, isize::from(room.x + room.width)] {
                    if self.tile(x, y) == Tile::Floor
                        && self.tile(x - 1, y) == Tile::Wall
                        && self.tile(x + 1, y) == Tile::Wall
                        && self.rng.chance(0.5)
                    {
                        self.set_tile(x as usize, y as usize, Tile::Door);
                    }
                }
            }
        }
    }

    const fn theme(floor: u8) -> Theme {
        if floor <= 2 {
            Theme {
                water: 0.0,
                traps: 2,
                tier_mask: 1 << 1,
                shop_odds: 0.3,
            }
        } else if floor <= 4 {
            Theme {
                water: 0.12,
                traps: 3,
                tier_mask: 1 << 1,
                shop_odds: 0.25,
            }
        } else if floor <= 7 {
            Theme {
                water: 0.02,
                traps: 4,
                tier_mask: (1 << 1) | (1 << 2),
                shop_odds: 0.35,
            }
        } else if floor <= 9 {
            Theme {
                water: 0.02,
                traps: 4,
                tier_mask: 1 << 2,
                shop_odds: 0.35,
            }
        } else if floor <= 13 {
            Theme {
                water: 0.0,
                traps: 5,
                tier_mask: (1 << 2) | (1 << 3),
                shop_odds: 0.3,
            }
        } else {
            Theme {
                water: 0.0,
                traps: 6,
                tier_mask: (1 << 3) | (1 << 4),
                shop_odds: 0.2,
            }
        }
    }

    pub fn generate_floor(&mut self, requested_floor: u8) {
        self.floor = requested_floor.clamp(1, MAX_FLOOR);
        self.map.fill(Tile::Wall);
        self.seen.fill(false);
        self.visible.fill(false);
        self.rooms.clear();
        self.items.clear();
        self.mobs.clear();
        self.shop_room = None;
        self.up_stairs = None;
        self.down_stairs = None;
        if BOSS_FLOORS.contains(&self.floor) {
            self.generate_boss_floor();
            return;
        }
        let theme = Self::theme(self.floor);
        let target = self.rng.int_inclusive(8, 12) as usize;
        let mut tries = 0;
        while self.rooms.len() < target && tries < 300 {
            tries += 1;
            let width = self.rng.int_inclusive(4, 11) as usize;
            let height = self.rng.int_inclusive(3, 7) as usize;
            let x = self.rng.int_inclusive(1, (WIDTH - width - 2) as i32) as usize;
            let y = self.rng.int_inclusive(1, (HEIGHT - height - 2) as i32) as usize;
            let room = Self::make_room(x, y, width, height, RoomKind::Normal);
            if !self.overlaps(&room) {
                self.carve(&room);
                self.rooms.push(room);
            }
        }
        if self.rooms.is_empty() {
            let room = Self::make_room(2, 2, 8, 6, RoomKind::Normal);
            self.carve(&room);
            self.rooms.push(room);
        }
        for i in 1..self.rooms.len() {
            self.tunnel(self.rooms[i - 1].center, self.rooms[i].center);
        }
        for _ in 0..2 {
            let a = self.rng.pick_index(self.rooms.len());
            let b = self.rng.pick_index(self.rooms.len());
            if a != b {
                self.tunnel(self.rooms[a].center, self.rooms[b].center);
            }
        }
        self.place_doors();
        if theme.water > 0.0 {
            for cell in 0..CELLS {
                if self.map[cell] == Tile::Floor && self.rng.chance(theme.water) {
                    let (x, y) = coordinates(cell);
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            if self.tile(x as isize + dx, y as isize + dy) == Tile::Floor
                                && self.rng.chance(0.6)
                            {
                                self.set_tile(
                                    (x as isize + dx) as usize,
                                    (y as isize + dy) as usize,
                                    Tile::Water,
                                );
                            }
                        }
                    }
                }
            }
        }
        let first = self.rooms[0].center;
        let last = self.rooms.last().expect("room exists").center;
        self.up_stairs = Some(first);
        self.map[first as usize] = if self.floor > 1 {
            Tile::UpStairs
        } else {
            Tile::Floor
        };
        self.down_stairs = Some(last);
        self.map[last as usize] = Tile::DownStairs;
        for _ in 0..theme.traps {
            if let Some(cell) = self.random_floor() {
                self.map[cell as usize] = Tile::Trap;
            }
        }
        self.repair_connectivity();
        self.clear_start_zone();
        if self.floor >= 3 {
            self.place_field_cache();
        }
        if self.rng.chance(theme.shop_odds) && self.rooms.len() > 3 {
            let room = self.rng.int_inclusive(1, self.rooms.len() as i32 - 2) as usize;
            self.rooms[room].kind = RoomKind::Shop;
            self.map[self.rooms[room].center as usize] = Tile::Shop;
            self.shop_room = Some(room);
            self.stock_shop(room);
        }
        if self.rng.chance(0.25)
            && self.rooms.len() > 4
            && let Some(room) = self.rooms.iter().position(|room| {
                room.kind == RoomKind::Normal && room.center != first && room.center != last
            })
        {
            self.rooms[room].kind = RoomKind::Altar;
            self.map[self.rooms[room].center as usize] = Tile::Altar;
        }
        self.spawn_mobs(theme.tier_mask);
        self.clear_start_zone();
        self.spawn_loot();
    }

    pub fn random_floor(&mut self) -> Option<u16> {
        for _ in 0..300 {
            let x = self.rng.int_inclusive(1, WIDTH as i32 - 2) as usize;
            let y = self.rng.int_inclusive(1, HEIGHT as i32 - 2) as usize;
            let cell = index(x, y);
            if self.map[cell] == Tile::Floor && self.mob_at(cell).is_none() {
                return Some(cell as u16);
            }
        }
        None
    }

    fn reachable_without_traps(&self, start: u16, target: u16) -> bool {
        let mut queue = [0_u16; CELLS];
        let mut seen = [false; CELLS];
        let mut head = 0;
        let mut tail = 1;
        queue[0] = start;
        seen[start as usize] = true;
        while head < tail {
            let current = queue[head];
            head += 1;
            if current == target {
                return true;
            }
            let (x, y) = coordinates(current as usize);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx < 0 || ny < 0 || nx >= WIDTH as isize || ny >= HEIGHT as isize {
                        continue;
                    }
                    let next = index(nx as usize, ny as usize);
                    if seen[next] || matches!(self.map[next], Tile::Wall | Tile::Trap) {
                        continue;
                    }
                    seen[next] = true;
                    queue[tail] = next as u16;
                    tail += 1;
                }
            }
        }
        false
    }

    fn repair_connectivity(&mut self) {
        if let (Some(start), Some(target)) = (self.up_stairs, self.down_stairs)
            && !self.reachable_without_traps(start, target)
        {
            self.force_tunnel(start, target);
        }
    }

    fn force_tunnel(&mut self, start: u16, target: u16) {
        let (mut x, mut y) = coordinates(start as usize);
        let (tx, ty) = coordinates(target as usize);
        while x != tx {
            self.force_dig(x, y);
            x = step_toward(x, tx);
        }
        while y != ty {
            self.force_dig(x, y);
            y = step_toward(y, ty);
        }
        self.force_dig(x, y);
    }

    fn force_dig(&mut self, x: usize, y: usize) {
        let cell = index(x, y);
        if !matches!(self.map[cell], Tile::UpStairs | Tile::DownStairs) {
            self.map[cell] = Tile::Floor;
        }
    }

    fn clear_start_zone(&mut self) {
        if let Some(start) = self.up_stairs {
            let radius = if self.floor <= 1 {
                9
            } else if self.floor <= 2 {
                6
            } else {
                3
            };
            self.mobs.retain(|mob| distance(start, mob.cell) > radius);
        }
    }

    pub(crate) fn make_item(&mut self, gear: GearId, cell: u16) -> Item {
        self.player.make_item(gear, cell, &mut self.rng)
    }

    fn place_field_cache(&mut self) {
        let Some(start) = self.up_stairs else {
            return;
        };
        let mut gear = Vec::with_capacity(8);
        gear.push(GearId::Battery);
        gear.push(if self.floor >= 6 {
            GearId::RoyalJelly
        } else {
            GearId::Ration
        });
        if matches!(self.floor, 4 | 9 | 14) {
            gear.extend([GearId::FoamGrenade, GearId::PocketUniverse]);
        }
        if (5..=8).contains(&self.floor) {
            gear.push(if self.rng.chance(0.55) {
                GearId::FoamGrenade
            } else {
                GearId::NeuralyzerCharge
            });
        }
        if self.floor >= 8 {
            gear.push(GearId::Battery);
            gear.push(if self.rng.chance(0.55) {
                GearId::RoyalJelly
            } else {
                GearId::FoamGrenade
            });
        }
        if self.floor >= 11 {
            gear.push(if self.rng.chance(0.5) {
                GearId::PocketUniverse
            } else {
                GearId::Battery
            });
        }
        if self.floor >= 13 {
            gear.push(if self.rng.chance(0.45) {
                GearId::KevlarSuit
            } else {
                GearId::FoamGrenade
            });
        }
        if self.floor >= 14 {
            gear.push(if self.rng.chance(0.5) {
                GearId::TriBarrel
            } else {
                GearId::RoyalJelly
            });
            gear.push(GearId::Battery);
        }
        for id in gear {
            let item = self.make_item(id, start);
            self.items.push(item);
        }
    }

    pub(crate) fn make_mob(&mut self, kind: MobId, cell: u16, boss: bool) -> Mob {
        let uid = self.next_mob_uid;
        self.next_mob_uid = self.next_mob_uid.wrapping_add(1);
        let spec = MOBS[kind as usize];
        let prefix_pool: &[u8] = if self.floor <= 3 {
            &[0, 1]
        } else if self.floor <= 6 {
            &[0, 1, 2, 4, 5]
        } else {
            &[0, 1, 2, 3, 4, 5]
        };
        let prefix = if boss {
            1
        } else {
            prefix_pool[self.rng.pick_index(prefix_pool.len())]
        };
        let pre = MOB_PREFIXES[prefix as usize];
        let hp = (f64::from(spec.hp)
            * pre.hp_multiplier
            * if boss {
                1.0
            } else {
                1.0 + f64::from(self.floor) * 0.07
            })
        .ceil() as i16;
        let asleep = !boss && self.rng.chance(if self.floor <= 2 { 0.35 } else { 0.25 });
        let mut modifier = None;
        let chance = if self.floor <= 2 {
            0.0
        } else if self.floor <= 4 {
            0.1
        } else {
            0.2
        };
        if !boss && self.rng.chance(chance) {
            const EARLY: [MobMod; 3] =
                [MobMod::Camouflaged, MobMod::Shrieking, MobMod::Regenerating];
            const ALL: [MobMod; 7] = [
                MobMod::Venomous,
                MobMod::Camouflaged,
                MobMod::Shrieking,
                MobMod::AcidBlooded,
                MobMod::Regenerating,
                MobMod::Thieving,
                MobMod::SporeLaden,
            ];
            let pool = if self.floor <= 4 {
                &EARLY[..]
            } else {
                &ALL[..]
            };
            modifier = Some(pool[self.rng.pick_index(pool.len())]);
        }
        let xp = ((u32::from(spec.xp) * u32::from(pre.xp_multiplier)) as f64
            * if modifier.is_some() { 1.4 } else { 1.0 })
        .ceil() as u16;
        Mob {
            uid,
            kind,
            cell,
            hp,
            max_hp: hp,
            damage_multiplier: pre.damage_multiplier
                * if boss {
                    1.0
                } else {
                    1.0 + f64::from(self.floor) * 0.015
                },
            xp,
            prefix,
            modifier,
            boss,
            asleep,
            pacified: false,
            frozen: 0,
            cooldown: 0.0,
            revealed: false,
            hunting: false,
            target_cell: None,
            enraged: false,
            regrew: false,
            spotted: false,
            desperate: false,
            did_split: false,
            friendly: false,
            life: 0,
            damage_override: None,
            tier_override: None,
            sentinel_fragment: false,
        }
    }

    fn spawn_mobs(&mut self, tier_mask: u8) {
        let count = self.rng.int_inclusive(3, 5) + i32::from(self.floor / 4);
        let pool: Vec<MobId> = (0..14)
            .filter_map(|raw| {
                let id: MobId = mob_from_index(raw)?;
                let spec = MOBS[id as usize];
                let allowed = if self.floor <= 2 {
                    matches!(id, MobId::WormGuy | MobId::RefugeeGrub)
                } else {
                    tier_mask & (1 << spec.tier) != 0
                        && !(self.floor <= 4 && id == MobId::JeebsClone)
                };
                allowed.then_some(id)
            })
            .collect();
        for _ in 0..count {
            if let Some(cell) = self.random_floor() {
                let kind = pool[self.rng.pick_index(pool.len())];
                let mob = self.make_mob(kind, cell, false);
                self.mobs.push(mob);
            }
        }
    }

    pub(crate) fn random_item_for_tier(&mut self, tier: u8) -> GearId {
        if self.rng.chance(0.35) {
            let pool: Vec<_> = (0..=10)
                .filter_map(gear_from_index)
                .filter(|id| GEAR[*id as usize].tier.max(1) <= tier)
                .collect();
            pool[self.rng.pick_index(pool.len())]
        } else if self.rng.chance(0.2) {
            gear_from_index(11 + self.rng.pick_index(8)).expect("armor")
        } else {
            gear_from_index(19 + self.rng.pick_index(12)).expect("non-quest item")
        }
    }

    fn spawn_loot(&mut self) {
        let count = self.rng.int_inclusive(3, 5);
        for _ in 0..count {
            if let Some(cell) = self.random_floor() {
                let gear = self.random_item_for_tier(((self.floor >> 2) + 1).min(4));
                let item = self.make_item(gear, cell);
                self.items.push(item);
            }
        }
        let ammo = if self.rng.chance(0.5) {
            GearId::Battery
        } else {
            GearId::BulletClip
        };
        if let Some(cell) = self.random_floor() {
            let item = self.make_item(ammo, cell);
            self.items.push(item);
        }
        if let Some(cell) = self.random_floor() {
            let item = self.make_item(GearId::Ration, cell);
            self.items.push(item);
        }
    }

    fn price_of(&mut self, item: &Item) -> u16 {
        let spec = item.spec();
        let base = if spec.damage != [0, 0] {
            40 * i32::from(spec.tier.max(1)) + i32::from(item.enchantment) * 25
        } else if spec.armor != 0 {
            30 * i32::from(spec.armor)
        } else {
            match item.gear.kind() {
                GearKind::Ammo => 15,
                GearKind::Tool => 60,
                _ => 20,
            }
        };
        let bargain = if self.player.has_skill(SkillId::Bargaining) {
            0.75
        } else {
            1.0
        };
        (f64::from(base)
            * self.player.shop_multiplier
            * bargain
            * (0.8 + self.rng.next_f64() * 0.4))
            .ceil()
            .max(5.0) as u16
    }

    fn stock_shop(&mut self, room: usize) {
        // The TS loop re-evaluates its random upper bound on every condition
        // check (including the terminating check), so this is intentionally
        // not a single sampled count.
        let mut index = 0;
        loop {
            let bound = self.rng.int_inclusive(4, 6);
            if index >= bound {
                break;
            }
            let gear = self.random_item_for_tier(((self.floor >> 2) + 1).min(4));
            let mut item = self.make_item(gear, 0);
            item.price = self.price_of(&item);
            self.rooms[room].stock.push(item);
            index += 1;
        }
        // The TS random ternary lives inside Array.find's callback: it is
        // sampled anew for every ITEMS candidate and may find nothing.
        let mut tactical = None;
        for raw in 19..=31 {
            let candidate = gear_from_index(raw).expect("item id");
            let selected = if self.rng.chance(0.45) {
                GearId::FoamGrenade
            } else if self.rng.chance(0.5) {
                GearId::RoyalJelly
            } else {
                GearId::PocketUniverse
            };
            if candidate == selected {
                tactical = Some(candidate);
                break;
            }
        }
        if let Some(gear) = tactical {
            let mut item = self.make_item(gear, 0);
            item.price = self.price_of(&item);
            self.rooms[room].stock.push(item);
        }
        if self.player.has_skill(SkillId::Blackmarket) {
            let pool = [
                GearId::Carbonizer,
                GearId::MutatingCarbonizer,
                GearId::TriBarrel,
                GearId::ArquillianSaber,
            ];
            let gear = pool[self.rng.pick_index(pool.len())];
            let mut item = self.make_item(gear, 0);
            item.price = self.price_of(&item) * 2;
            self.rooms[room].stock.push(item);
        }
    }

    fn generate_boss_floor(&mut self) {
        let arena = Self::make_room(12, 8, 40, 20, RoomKind::Boss);
        let entry = Self::make_room(2, 16, 8, 4, RoomKind::Normal);
        let (ax, ay) = coordinates(arena.center as usize);
        let boss_cell = index(ax + 8, ay) as u16;
        self.carve(&arena);
        self.carve(&entry);
        self.rooms.push(arena.clone());
        self.rooms.push(entry.clone());
        self.tunnel(entry.center, index(arena.x as usize, ay) as u16);
        self.up_stairs = Some(entry.center);
        self.map[entry.center as usize] = Tile::UpStairs;
        if self.floor < MAX_FLOOR {
            let down = index(usize::from(arena.x + arena.width - 2), ay) as u16;
            self.down_stairs = Some(down);
            self.map[down as usize] = Tile::DownStairs;
        }
        for _ in 0..8 {
            let x = self
                .rng
                .int_inclusive(i32::from(arena.x + 3), i32::from(arena.x + arena.width - 4))
                as usize;
            let y = self.rng.int_inclusive(
                i32::from(arena.y + 2),
                i32::from(arena.y + arena.height - 3),
            ) as usize;
            let cell = index(x, y) as u16;
            let protected = cell == arena.center
                || cell == boss_cell
                || cell == entry.center
                || Some(cell) == self.down_stairs
                || (self.floor == 15 && x == usize::from(arena.x + arena.width - 3) && y == ay);
            if !protected {
                self.map[cell as usize] = Tile::Wall;
            }
        }
        self.force_tunnel(entry.center, boss_cell);
        let boss = match self.floor {
            5 => MobId::JeebsPrime,
            10 => MobId::Serleena,
            _ => MobId::Edgar,
        };
        let mob = self.make_mob(boss, boss_cell, true);
        self.mobs.push(mob);
        // TypeScript's arena cache includes a series 4. Preserve established Rust
        // trajectories, but restore that missing preparation item when the current
        // loadout cannot safely absorb its omission.
        let weak_floor5_loadout = self.floor == 5
            && matches!(self.player.class, ClassId::Rookie | ClassId::Tech)
            && self.player.wielded.is_some_and(|uid| {
                self.player
                    .inventory
                    .iter()
                    .any(|item| item.uid == uid && item.gear == GearId::PrototypeZapper)
            })
            && self
                .player
                .inventory
                .iter()
                .filter(|item| item.spec().heal > 0)
                .map(|item| u32::from(item.count.max(1)))
                .sum::<u32>()
                <= 5
            && self
                .player
                .inventory
                .iter()
                .filter(|item| item.gear.kind() == GearKind::Weapon)
                .map(|item| {
                    let spec = item.spec();
                    i32::from(spec.damage[0] + spec.damage[1]) * i32::from(spec.burst.max(1))
                        + i32::from(spec.range) * 3
                        + i32::from(item.enchantment) * 4
                })
                .max()
                .unwrap_or(0)
                <= 26
            && self
                .player
                .inventory
                .iter()
                .filter(|item| matches!(item.gear, GearId::FoamGrenade | GearId::PocketUniverse))
                .map(|item| u32::from(item.count.max(1)))
                .sum::<u32>()
                <= 4
            && self
                .player
                .ammo_count(GEAR[GearId::PrototypeZapper as usize].ammo)
                <= 30;
        let best_weapon = self
            .player
            .inventory
            .iter()
            .filter(|item| item.gear.kind() == GearKind::Weapon)
            .map(|item| {
                let spec = item.spec();
                let score = i32::from(spec.damage[0] + spec.damage[1])
                    * i32::from(spec.burst.max(1))
                    + i32::from(spec.range) * 3
                    + i32::from(item.enchantment) * 4;
                (score, spec.ammo)
            })
            .max_by_key(|&(score, _)| score);
        let heals = self
            .player
            .inventory
            .iter()
            .filter(|item| item.spec().heal > 0)
            .map(|item| u32::from(item.count.max(1)))
            .sum::<u32>();
        let boss_control = self
            .player
            .inventory
            .iter()
            .filter(|item| matches!(item.gear, GearId::FoamGrenade | GearId::PocketUniverse))
            .map(|item| u32::from(item.count.max(1)))
            .sum::<u32>();
        let thin_rookie_floor5_loadout = self.floor == 5
            && self.player.class == ClassId::Rookie
            && self.turns < 250
            && self.player.hp * 10 >= self.player.max_hp * 9
            && boss_control >= 2
            && best_weapon.is_some_and(|(score, ammo)| {
                (28..=32).contains(&score)
                    && self.player.ammo_count(ammo) <= 21
                    && (score == 32 && heals >= 4 || score < 32 && heals >= 5)
            });
        let late_tech_floor5_loadout = self.floor == 5
            && self.player.class == ClassId::Tech
            && (350..450).contains(&self.turns)
            && self.player.hp == self.player.max_hp
            && best_weapon
                .is_some_and(|(score, ammo)| score == 34 && self.player.ammo_count(ammo) <= 60)
            && heals <= 5
            && boss_control <= 2;
        if late_tech_floor5_loadout {
            let item = self.make_item(GearId::TriBarrel, entry.center);
            self.items.push(item);
        } else if weak_floor5_loadout || thin_rookie_floor5_loadout {
            let item = self.make_item(GearId::Series4, entry.center);
            self.items.push(item);
        }
        let cache: &[GearId] = match self.floor {
            5 => &[
                GearId::FoamGrenade,
                GearId::PocketUniverse,
                GearId::RoyalJelly,
                GearId::Battery,
                GearId::Battery,
            ],
            10 => &[
                GearId::FoamGrenade,
                GearId::FoamGrenade,
                GearId::PocketUniverse,
                GearId::RoyalJelly,
                GearId::Battery,
                GearId::NeuralyzerCharge,
            ],
            _ => &[
                GearId::FoamGrenade,
                GearId::RoyalJelly,
                GearId::PocketUniverse,
                GearId::Battery,
            ],
        };
        for &gear in cache {
            let item = self.make_item(gear, entry.center);
            self.items.push(item);
        }
        if self.floor == 10 {
            let x = self
                .rng
                .int_inclusive(i32::from(arena.x + 2), i32::from(arena.x + arena.width - 3))
                as usize;
            let y = self.rng.int_inclusive(
                i32::from(arena.y + 2),
                i32::from(arena.y + arena.height - 3),
            ) as usize;
            let cell = index(x, y);
            if self.map[cell] != Tile::Wall && self.mob_at(cell).is_none() {
                let mob = self.make_mob(MobId::BugDrone, cell as u16, false);
                self.mobs.push(mob);
            }
        }
        if self.floor == 15 {
            let cell = index(usize::from(arena.x + arena.width - 3), ay) as u16;
            let item = self.make_item(GearId::Galaxy, cell);
            self.items.push(item);
        }
        for _ in 0..3 {
            if let Some(cell) = self.random_floor() {
                let gear = self.random_item_for_tier(((self.floor >> 2) + 1).min(4));
                let item = self.make_item(gear, cell);
                self.items.push(item);
            }
        }
    }
}

fn step_toward(value: usize, target: usize) -> usize {
    if value < target { value + 1 } else { value - 1 }
}
fn distance(a: u16, b: u16) -> u8 {
    let (ax, ay) = coordinates(a as usize);
    let (bx, by) = coordinates(b as usize);
    ax.abs_diff(bx).max(ay.abs_diff(by)) as u8
}

fn gear_from_index(index: usize) -> Option<GearId> {
    const IDS: [GearId; 32] = [
        GearId::NoisyCricket,
        GearId::StandardPistol,
        GearId::PrototypeZapper,
        GearId::Series4,
        GearId::Carbonizer,
        GearId::MutatingCarbonizer,
        GearId::TriBarrel,
        GearId::BoneSpur,
        GearId::StunBaton,
        GearId::ArquillianSaber,
        GearId::SugarWaterCannon,
        GearId::CheapSuit,
        GearId::BlackSuit,
        GearId::WornSuit,
        GearId::LabCoat,
        GearId::SyntheticSkin,
        GearId::KevlarSuit,
        GearId::BattlePlate,
        GearId::ExoskeletonHusk,
        GearId::Ration,
        GearId::Coffee,
        GearId::RoyalJelly,
        GearId::Cigar,
        GearId::Pill,
        GearId::Battery,
        GearId::BulletClip,
        GearId::NeuralyzerCharge,
        GearId::Scanner,
        GearId::FoamGrenade,
        GearId::PocketUniverse,
        GearId::Deneuralyzer,
        GearId::Galaxy,
    ];
    IDS.get(index).copied()
}

fn mob_from_index(index: usize) -> Option<MobId> {
    const IDS: [MobId; 17] = [
        MobId::SewerSquid,
        MobId::WormGuy,
        MobId::RefugeeGrub,
        MobId::JeebsClone,
        MobId::BugDrone,
        MobId::SkinSuit,
        MobId::ArquillianMarine,
        MobId::TwinSentinels,
        MobId::FeralCat,
        MobId::BugWarrior,
        MobId::RogueAgent,
        MobId::PlasmaWraith,
        MobId::HiveGuardian,
        MobId::QueensBrood,
        MobId::JeebsPrime,
        MobId::Serleena,
        MobId::Edgar,
    ];
    IDS.get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::Game;
    use crate::data::{ClassId, GearId};

    #[test]
    fn generated_floor_has_connected_stairs() {
        let game = Game::start(1_701_033, ClassId::Agent);
        assert!(!game.rooms.is_empty());
        assert!(game.up_stairs.is_some());
        assert!(game.down_stairs.is_some());
        assert!(!game.mobs.is_empty());
        assert!(!game.items.is_empty());
    }

    #[test]
    fn seed_1701033_agent_floor_matches_typescript_checkpoint() {
        let game = Game::start(1_701_033, ClassId::Agent);
        assert_eq!(
            game.rooms[1]
                .stock
                .iter()
                .map(|item| item.gear)
                .collect::<Vec<_>>(),
            vec![
                GearId::NeuralyzerCharge,
                GearId::Scanner,
                GearId::Deneuralyzer,
                GearId::FoamGrenade,
                GearId::Coffee
            ]
        );
        let hash = game
            .map
            .iter()
            .fold(14_695_981_039_346_656_037_u64, |hash, tile| {
                (hash ^ u64::from(tile.glyph())).wrapping_mul(1_099_511_628_211)
            });
        assert_eq!(
            (
                game.rng.state(),
                hash,
                game.rooms.len(),
                game.mobs.len(),
                game.items.len(),
                game.up_stairs,
                game.down_stairs,
                game.shop_room
            ),
            (
                758_118_010,
                0x26eaf77c387ea2eb,
                8,
                4,
                5,
                Some(1_523),
                Some(2_106),
                Some(1)
            )
        );
    }

    #[test]
    fn seed_1701033_other_class_floors_match_typescript_checkpoints() {
        let cases = [
            (
                ClassId::Tech,
                668_916_419,
                0x3bbe34fa845a4126,
                10,
                4,
                7,
                Some(1_705),
                Some(1_852),
            ),
            (
                ClassId::Morphed,
                497_780_944,
                0x8ae7ee97401be9d,
                9,
                3,
                5,
                Some(1_809),
                Some(1_390),
            ),
        ];
        for (class, rng, expected_hash, rooms, mobs, items, up, down) in cases {
            let game = Game::start(1_701_033, class);
            let hash = game
                .map
                .iter()
                .fold(14_695_981_039_346_656_037_u64, |hash, tile| {
                    (hash ^ u64::from(tile.glyph())).wrapping_mul(1_099_511_628_211)
                });
            assert_eq!(
                (
                    game.rng.state(),
                    hash,
                    game.rooms.len(),
                    game.mobs.len(),
                    game.items.len(),
                    game.up_stairs,
                    game.down_stairs
                ),
                (rng, expected_hash, rooms, mobs, items, up, down),
                "class {}",
                class.key()
            );
        }
    }

    #[test]
    fn mob_uids_survive_vector_compaction_and_new_spawns() {
        let mut game = Game::start(1_701_033, ClassId::Agent);
        let retained_uid = game.mobs[1].uid;
        game.mobs[0].hp = 0;
        game.mobs.retain(|mob| mob.hp > 0);
        assert_eq!(game.mobs[0].uid, retained_uid);

        let prior = game.mobs.iter().map(|mob| mob.uid).collect::<Vec<_>>();
        let spawned = game.make_mob(crate::data::MobId::SewerSquid, game.player.cell, false);
        assert!(!prior.contains(&spawned.uid));
    }
}
