use std::fmt::Write;

use mib_rust::{
    HEIGHT, WIDTH,
    bot::{
        Action, Bot, ensemble_candidate_count, evaluate_ensemble_candidate_from_state,
        plan_ensemble_candidate_from_state, plan_ensemble_from_state,
    },
    data::{ClassId, GEAR, MOBS},
    world::{Game, PlayerDamageSource, Tile},
};
use wasm_bindgen::prelude::*;

const PROTOCOL_VERSION: u8 = 1;
const TURN_CAP: u32 = 3_600;

#[wasm_bindgen]
pub struct RustEpisode {
    seed: u64,
    class: ClassId,
    game: Game,
    bot: Bot,
    previous: Option<Game>,
    action: Action,
    cursor: usize,
    plan: Vec<(Game, Action)>,
    plan_cursor: usize,
    selected_policy: String,
}

#[wasm_bindgen]
impl RustEpisode {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32, class_key: &str) -> Result<RustEpisode, JsError> {
        let key = class_key.chars().next().unwrap_or('a');
        let class = ClassId::from_key(key).ok_or_else(|| JsError::new("unknown MIB class"))?;
        let seed = u64::from(seed);
        Ok(Self {
            seed,
            class,
            game: Game::start(seed, class),
            bot: Bot::default(),
            previous: None,
            action: Action::None,
            cursor: 0,
            plan: Vec::new(),
            plan_cursor: 0,
            selected_policy: "unplanned".to_owned(),
        })
    }

    pub fn protocol_version(&self) -> u8 {
        PROTOCOL_VERSION
    }

    pub fn frame_count(&self) -> usize {
        TURN_CAP as usize
    }

    pub fn frame_index(&self) -> usize {
        self.cursor
    }

    pub fn reset(&mut self) -> String {
        self.game = Game::start(self.seed, self.class);
        self.bot = Bot::default();
        self.previous = None;
        self.action = Action::None;
        self.cursor = 0;
        self.plan.clear();
        self.plan_cursor = 0;
        self.selected_policy = "unplanned".to_owned();
        self.snapshot_json()
    }

    pub fn snapshot_json(&self) -> String {
        let (game, previous, action) = self.frame_context();
        snapshot_json(
            game,
            previous,
            action,
            self.seed,
            self.class,
            self.cursor,
            TURN_CAP as usize,
        )
    }

    pub fn next_snapshot_json(&mut self) -> String {
        if self.cursor < TURN_CAP as usize && !self.game.player.won && !self.game.player.dead {
            if self.plan_cursor >= self.plan.len() {
                let remaining = TURN_CAP.saturating_sub(self.cursor as u32);
                self.replan(remaining);
            }
            self.previous = Some(self.game.clone());
            if let Some((game, action)) = self.plan.get(self.plan_cursor) {
                self.game = game.clone();
                self.action = action.clone();
                self.plan_cursor += 1;
            } else {
                self.action = self.bot.choose(&mut self.game);
                self.bot.apply(&mut self.game, self.action.clone());
                self.selected_policy = "baseline-fallback".to_owned();
            }
            self.cursor += 1;
        }
        self.snapshot_json()
    }

    pub fn seek_snapshot_json(&mut self, frame: usize) -> String {
        self.reset();
        while self.cursor < frame.min(TURN_CAP as usize)
            && !self.game.player.won
            && !self.game.player.dead
        {
            self.next_snapshot_json();
        }
        self.snapshot_json()
    }

    pub fn apply_action_signature_json(&mut self, signature: &str) -> Result<String, JsError> {
        if self.game.player.won || self.game.player.dead {
            return Ok(self.snapshot_json());
        }
        let before_turn = self.game.turns;
        self.previous = Some(self.game.clone());
        self.plan.clear();
        self.plan_cursor = 0;
        self.selected_policy = "human".to_owned();
        let action = signature_action(signature, &self.game);
        self.game
            .apply_action_signature(signature)
            .map_err(|error| JsError::new(&error))?;
        self.action = action;
        if self.game.turns != before_turn {
            self.cursor += 1;
        }
        Ok(self.snapshot_json())
    }

    pub fn selected_policy(&self) -> String {
        self.selected_policy.clone()
    }

    pub fn needs_plan(&self) -> bool {
        self.plan_cursor >= self.plan.len() && !self.game.player.won && !self.game.player.dead
    }

    pub fn planning_candidate_count(&self) -> usize {
        ensemble_candidate_count(self.class)
    }

    pub fn evaluate_planning_candidate_json(&self, index: usize) -> Result<String, JsError> {
        let remaining = TURN_CAP.saturating_sub(self.cursor as u32);
        self.evaluate_planning_candidate_with_cap_json(index, remaining)
    }

    pub fn benchmark_planning_candidate_json(
        &self,
        index: usize,
        turn_cap: u32,
    ) -> Result<String, JsError> {
        self.evaluate_planning_candidate_with_cap_json(index, turn_cap.clamp(1, TURN_CAP))
    }

    fn evaluate_planning_candidate_with_cap_json(
        &self,
        index: usize,
        turn_cap: u32,
    ) -> Result<String, JsError> {
        let Some((game, policy)) = evaluate_ensemble_candidate_from_state(
            &self.game, &self.bot, self.class, turn_cap, index,
        ) else {
            return Err(JsError::new("planning candidate index is out of range"));
        };
        let primary = if game.player.won {
            u32::MAX - game.turns
        } else {
            game.score()
        };
        Ok(format!(
            "{{\"index\":{index},\"policy\":\"{policy}\",\"won\":{},\"deepest\":{},\"primary\":{primary},\"score\":{},\"turns\":{}}}",
            game.player.won,
            game.player.deepest,
            game.score(),
            game.turns,
        ))
    }

    pub fn install_planning_candidate(&mut self, index: usize) -> Result<(), JsError> {
        let remaining = TURN_CAP.saturating_sub(self.cursor as u32);
        let Some((plan, policy)) =
            plan_ensemble_candidate_from_state(&self.game, &self.bot, self.class, remaining, index)
        else {
            return Err(JsError::new("planning candidate index is out of range"));
        };
        self.plan = plan;
        self.plan_cursor = 0;
        self.selected_policy = policy;
        Ok(())
    }

    pub fn plan_strategy(&mut self, strategy: &str) -> Result<(), JsError> {
        if strategy == "strongest" {
            let remaining = TURN_CAP.saturating_sub(self.cursor as u32);
            self.replan(remaining);
            return Ok(());
        }
        if strategy == "baseline" {
            return self.install_planning_candidate(0);
        }
        if strategy != "balanced" {
            return Err(JsError::new("unknown planning strategy"));
        }
        let remaining = TURN_CAP.saturating_sub(self.cursor as u32);
        let candidate_count = ensemble_candidate_count(self.class).min(7);
        let mut best: Option<(usize, (bool, u8, u32, u32))> = None;
        for index in 0..candidate_count {
            let Some((game, _)) = evaluate_ensemble_candidate_from_state(
                &self.game, &self.bot, self.class, remaining, index,
            ) else {
                continue;
            };
            let primary = if game.player.won {
                u32::MAX - game.turns
            } else {
                game.score()
            };
            let rank = (game.player.won, game.player.deepest, primary, game.score());
            if best.as_ref().is_none_or(|(_, current)| rank > *current) {
                best = Some((index, rank));
            }
        }
        self.install_planning_candidate(best.map_or(0, |(index, _)| index))
    }
}

/// Test-only episode. Bot choice and human UI application share one Game, while
/// the production RustEpisode type and hot path remain unchanged.
#[wasm_bindgen]
pub struct RustE2eEpisode {
    episode: RustEpisode,
    recommended: Option<String>,
}

#[wasm_bindgen]
impl RustE2eEpisode {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32, class_key: &str) -> Result<RustE2eEpisode, JsError> {
        Ok(Self {
            episode: RustEpisode::new(seed, class_key)?,
            recommended: None,
        })
    }

    pub fn snapshot_json(&self) -> String {
        self.episode.snapshot_json()
    }
    pub fn reset(&mut self) -> String {
        self.recommended = None;
        self.episode.reset()
    }
    pub fn seek_snapshot_json(&mut self, frame: usize) -> String {
        self.recommended = None;
        self.episode.seek_snapshot_json(frame)
    }
    pub fn next_snapshot_json(&mut self) -> String {
        self.recommended = None;
        self.episode.next_snapshot_json()
    }
    pub fn selected_policy(&self) -> String {
        self.episode.selected_policy()
    }
    pub fn needs_plan(&self) -> bool {
        self.episode.needs_plan()
    }
    pub fn planning_candidate_count(&self) -> usize {
        self.episode.planning_candidate_count()
    }
    pub fn evaluate_planning_candidate_json(&self, index: usize) -> Result<String, JsError> {
        self.episode.evaluate_planning_candidate_json(index)
    }

    pub fn benchmark_planning_candidate_json(
        &self,
        index: usize,
        turn_cap: u32,
    ) -> Result<String, JsError> {
        self.episode
            .benchmark_planning_candidate_json(index, turn_cap)
    }
    pub fn install_planning_candidate(&mut self, index: usize) -> Result<(), JsError> {
        self.episode.install_planning_candidate(index)
    }
    pub fn plan_strategy(&mut self, strategy: &str) -> Result<(), JsError> {
        self.episode.plan_strategy(strategy)
    }

    pub fn recommend_action_signature(&mut self) -> String {
        if self.episode.game.player.won || self.episode.game.player.dead {
            return "none".to_owned();
        }
        if self.recommended.is_none() {
            let action = self.episode.bot.choose(&mut self.episode.game);
            self.recommended = Some(action.identity(&self.episode.game));
        }
        self.recommended.clone().expect("recommendation")
    }

    pub fn apply_action_signature_json(&mut self, signature: &str) -> Result<String, JsError> {
        let expected = self
            .recommended
            .take()
            .ok_or_else(|| JsError::new("UI action arrived without a bot instruction"))?;
        if signature != expected {
            return Err(JsError::new(&format!(
                "UI action {signature} did not match bot instruction {expected}"
            )));
        }
        self.episode.apply_action_signature_json(signature)
    }
}

impl RustEpisode {
    fn replan(&mut self, turn_cap: u32) {
        (self.plan, self.selected_policy) =
            plan_ensemble_from_state(&self.game, &self.bot, self.class, turn_cap);
        self.plan_cursor = 0;
    }

    fn frame_context(&self) -> (&Game, Option<&Game>, Option<&Action>) {
        (
            &self.game,
            self.previous.as_ref(),
            (self.cursor > 0).then_some(&self.action),
        )
    }
}

fn snapshot_json(
    game: &Game,
    previous: Option<&Game>,
    action: Option<&Action>,
    seed: u64,
    class: ClassId,
    frame: usize,
    frame_count: usize,
) -> String {
    let event_previous = previous;
    let previous = previous.filter(|old| old.floor == game.floor);
    let mut out = String::with_capacity(12_000);
    write!(
        out,
        "{{\"version\":{PROTOCOL_VERSION},\"seed\":{seed},\"class\":\"{}\",\"width\":{WIDTH},\"height\":{HEIGHT},\"frame\":{frame},\"frameCount\":{frame_count},\"floor\":{},\"turns\":{},\"score\":{},\"won\":{},\"dead\":{},",
        class.key(),
        game.floor,
        game.turns,
        game.score(),
        game.player.won,
        game.player.dead,
    )
    .expect("write JSON header");

    let raw_player_from = previous.map_or(game.player.cell, |old| old.player.cell);
    let player_teleported =
        raw_player_from != game.player.cell && !adjacent(raw_player_from, game.player.cell);
    let player_from = if player_teleported {
        game.player.cell
    } else {
        raw_player_from
    };
    let player_dealt_damage = previous.is_some_and(|old| {
        old.mobs.iter().any(|old_mob| {
            old_mob.hp > 0
                && game
                    .mobs
                    .iter()
                    .find(|mob| mob.uid == old_mob.uid)
                    .is_none_or(|mob| old_mob.hp > mob.hp)
        })
    });
    let player_state = if game.player.won {
        0
    } else if game.player.dead {
        5
    } else if previous.is_some_and(|old| old.player.hp > game.player.hp) {
        4
    } else if matches!(action, Some(Action::Fire(_) | Action::Throw(_, _))) {
        3
    } else if player_dealt_damage && player_from == game.player.cell {
        2
    } else if player_from != game.player.cell {
        1
    } else {
        0
    };
    let player_was_hurt = previous.is_some_and(|old| old.player.hp > game.player.hp);
    let damaged_mob_target = previous.and_then(|old_game| {
        old_game
            .mobs
            .iter()
            .filter_map(|old_mob| {
                let current = game.mobs.iter().find(|mob| mob.uid == old_mob.uid);
                (old_mob.hp > 0 && current.is_none_or(|mob| old_mob.hp > mob.hp))
                    .then_some(current.map_or(old_mob.cell, |mob| mob.cell))
            })
            .min_by_key(|cell| Game::distance(*cell as usize, game.player.cell as usize))
    });
    let attacking_mob_target = player_was_hurt
        .then(|| {
            game.mobs
                .iter()
                .filter(|mob| mob.hp > 0 && !mob.friendly)
                .min_by_key(|mob| Game::distance(mob.cell as usize, game.player.cell as usize))
                .map(|mob| mob.cell)
        })
        .flatten();
    let engaged_mob_target = game
        .mobs
        .iter()
        .filter(|mob| {
            mob.hp > 0
                && !mob.friendly
                && !mob.asleep
                && !mob.pacified
                && mob.hunting
                && game.visible[mob.cell as usize]
        })
        .min_by_key(|mob| Game::distance(mob.cell as usize, game.player.cell as usize))
        .map(|mob| mob.cell);
    let player_target = action_target(action)
        .or(damaged_mob_target)
        .or(attacking_mob_target)
        .or(engaged_mob_target);
    // Ranged weapons such as the Noisy Cricket can recoil the player in the
    // opposite direction during the same action. The attack pose must follow
    // its explicit target, not that secondary displacement.
    let player_direction = if player_state == 2 || player_state == 3 {
        actor_direction(game.player.cell, game.player.cell, player_target)
    } else {
        actor_direction(player_from, game.player.cell, player_target)
    };
    let weapon = game
        .player
        .wielded
        .and_then(|uid| game.player.inventory.iter().find(|item| item.uid == uid));
    let weapon_spec = weapon.map(|item| item.spec());
    let weapon_name = weapon_spec.map_or("unarmed", |spec| spec.name);
    let weapon_gear = weapon.map_or(-1, |item| item.gear as i16);
    let damage = weapon_spec.map_or([1, 2], |spec| spec.damage);
    let range = weapon_spec.map_or(1, |spec| spec.range);
    let ammo = weapon_spec.map_or(0, |spec| game.player.ammo_count(spec.ammo));
    let worn = game
        .player
        .worn
        .and_then(|uid| game.player.inventory.iter().find(|item| item.uid == uid));
    let armor = worn.map_or(0, |item| item.spec().armor);
    let armor_gear = worn.map_or(-1, |item| item.gear as i16);
    write!(
        out,
        "\"player\":{{\"cell\":{},\"fromCell\":{player_from},\"teleported\":{player_teleported},\"state\":{player_state},\"direction\":{player_direction},\"agent\":{},\"hp\":{},\"maxHp\":{},\"level\":{},\"xp\":{},\"xpNext\":{},\"credits\":{},\"nutrition\":{},\"kills\":{},\"weapon\":\"{weapon_name}\",\"weaponGear\":{weapon_gear},\"damageMin\":{},\"damageMax\":{},\"range\":{range},\"ammo\":{ammo},\"armor\":{armor},\"armorGear\":{armor_gear}}},",
        game.player.cell, game.player.agent_letter, game.player.hp, game.player.max_hp, game.player.level, game.player.xp, game.player.xp_next, game.player.credits, game.player.nutrition, game.player.kills, damage[0], damage[1],
    )
    .expect("write player");

    out.push_str("\"action\":\"");
    // Consumables and thrown items may no longer exist after application, so
    // resolve their human-readable identity against the pre-action state.
    let identity_game = previous.unwrap_or(game);
    push_json_string(
        &mut out,
        &action.map_or_else(
            || "mission ready".to_owned(),
            |action| action.identity(identity_game),
        ),
    );
    out.push_str("\",");
    write_event_logs(&mut out, game, event_previous, action);
    out.push_str("\"inventory\":[");
    for (index, item) in game.player.inventory.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        let kind = match item.gear.kind() {
            mib_rust::data::GearKind::Weapon => "weapon",
            mib_rust::data::GearKind::Armor => "armor",
            mib_rust::data::GearKind::Food => "food",
            mib_rust::data::GearKind::Pill => "pill",
            mib_rust::data::GearKind::Ammo => "ammo",
            mib_rust::data::GearKind::Tool => "tool",
            mib_rust::data::GearKind::Thrown => "thrown",
            mib_rust::data::GearKind::Quest => "quest",
        };
        write!(out, "{{\"name\":\"").expect("inventory start");
        push_json_string(&mut out, item.spec().name);
        write!(
            out,
            "\",\"kind\":\"{kind}\",\"count\":{},\"gear\":{},\"wielded\":{},\"worn\":{}}}",
            item.count.max(1),
            item.gear as u8,
            game.player.wielded == Some(item.uid),
            game.player.worn == Some(item.uid)
        )
        .expect("inventory item");
    }
    out.push_str("],");
    out.push_str("\"shop\":[");
    if let Some(room) = game.shop_room {
        for (index, item) in game.rooms[room].stock.iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            write!(out, "{{\"name\":\"").expect("shop item");
            push_json_string(&mut out, item.spec().name);
            write!(
                out,
                "\",\"gear\":{},\"price\":{}}}",
                item.gear as u8, item.price
            )
            .expect("shop item");
        }
    }
    out.push_str("],");

    out.push_str("\"map\":\"");
    for tile in game.map {
        out.push(char::from(tile.glyph()));
    }
    out.push_str("\",\"seen\":\"");
    for &seen in &game.seen {
        out.push(if seen { '1' } else { '0' });
    }
    out.push_str("\",\"visible\":\"");
    for &visible in &game.visible {
        out.push(if visible { '1' } else { '0' });
    }
    out.push_str("\",\"items\":[");
    for (index, item) in game.items.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        write!(
            out,
            "{{\"cell\":{},\"gear\":{},\"name\":\"",
            item.cell, item.gear as u8
        )
        .expect("write item");
        push_json_string(&mut out, GEAR[item.gear as usize].name);
        out.push_str("\"}");
    }
    out.push_str("],\"mobs\":[");
    let mut first = true;
    let attacker_uid = player_was_hurt
        .then(|| {
            game.mobs
                .iter()
                .filter(|mob| mob.hp > 0 && !mob.friendly)
                .min_by_key(|mob| Game::distance(mob.cell as usize, game.player.cell as usize))
                .map(|mob| mob.uid)
        })
        .flatten();
    let mut render_mobs = game
        .mobs
        .iter()
        .map(|mob| {
            let old = previous.and_then(|game| game.mobs.iter().find(|old| old.uid == mob.uid));
            (mob, old, false)
        })
        .collect::<Vec<_>>();
    if let Some(old_game) = previous {
        for old in old_game
            .mobs
            .iter()
            .filter(|old| old.hp > 0 && !game.mobs.iter().any(|mob| mob.uid == old.uid))
        {
            render_mobs.push((old, Some(old), true));
        }
    }
    for (mob, old, removed_dead) in render_mobs {
        let appeared = previous.is_some() && old.is_none();
        let newly_dead = removed_dead || mob.hp <= 0 && old.is_some_and(|old| old.hp > 0);
        if mob.hp <= 0 && !newly_dead {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        let raw_from_cell = old.map_or(mob.cell, |old| old.cell);
        let from_cell = if adjacent(raw_from_cell, mob.cell) {
            raw_from_cell
        } else {
            mob.cell
        };
        let state = if newly_dead {
            7
        } else if mob.frozen > 0 {
            4
        } else if old.is_some_and(|old| old.hp > mob.hp) {
            3
        } else if mob.pacified {
            6
        } else if mob.asleep {
            5
        } else if attacker_uid == Some(mob.uid) {
            2
        } else if from_cell != mob.cell {
            1
        } else {
            0
        };
        let target = if state == 2 || state == 3 {
            Some(game.player.cell)
        } else if mob.hunting && !mob.friendly && !mob.asleep && !mob.pacified {
            mob.target_cell.or(Some(game.player.cell))
        } else {
            None
        };
        let render_hp = if newly_dead { 0 } else { mob.hp };
        let direction = actor_direction(from_cell, mob.cell, target);
        write!(
            out,
            "{{\"uid\":{},\"cell\":{},\"fromCell\":{from_cell},\"state\":{state},\"direction\":{direction},\"appeared\":{appeared},\"kind\":{},\"name\":\"",
            mob.uid, mob.cell, mob.kind as u8
        )
        .expect("write mob");
        push_json_string(&mut out, MOBS[mob.kind as usize].name);
        write!(
            out,
            "\",\"hp\":{},\"maxHp\":{},\"boss\":{},\"friendly\":{},\"spotted\":{},\"asleep\":{},\"pacified\":{},\"frozen\":{}}}",
            render_hp,
            mob.max_hp,
            mob.boss,
            mob.friendly,
            mob.spotted,
            mob.asleep,
            mob.pacified,
            mob.frozen,
        )
        .expect("finish mob");
    }
    out.push_str("]}");
    out
}

#[derive(Clone)]
struct EventLine {
    text: String,
    class: Option<&'static str>,
}

fn write_event_logs(
    out: &mut String,
    game: &Game,
    previous: Option<&Game>,
    action: Option<&Action>,
) {
    let Some(old) = previous else {
        out.push_str("\"logs\":[],");
        return;
    };
    let mut lines = Vec::<EventLine>::new();
    let mut mob_was_hit = false;

    if old.floor != game.floor {
        lines.push(EventLine {
            text: format!("F{}: New operational sector entered.", game.floor),
            class: Some("warn"),
        });
    } else {
        if old.player.cell != game.player.cell && !adjacent(old.player.cell, game.player.cell) {
            lines.push(EventLine {
                text: "Teleported.".to_owned(),
                class: Some("good"),
            });
        }
        for old_mob in &old.mobs {
            let next = game.mobs.iter().find(|mob| mob.uid == old_mob.uid);
            if old_mob.hp > 0 && next.is_none_or(|mob| mob.hp <= 0) {
                lines.push(EventLine {
                    text: format!("{} dispatched.", MOBS[old_mob.kind as usize].name),
                    class: Some("good"),
                });
                mob_was_hit = true;
            } else if let Some(next) = next
                && old_mob.hp > next.hp
            {
                lines.push(EventLine {
                    text: format!(
                        "Hit {}: {} damage.",
                        MOBS[next.kind as usize].name,
                        old_mob.hp - next.hp
                    ),
                    class: None,
                });
                mob_was_hit = true;
            }
        }
        for mob in &game.mobs {
            if mob.spotted
                && old
                    .mobs
                    .iter()
                    .find(|old_mob| old_mob.uid == mob.uid)
                    .is_none_or(|old_mob| !old_mob.spotted)
            {
                lines.push(EventLine {
                    text: format!("Contact: {}.", MOBS[mob.kind as usize].name),
                    class: mob.boss.then_some("warn"),
                });
            }
        }
    }

    if old.player.hp > game.player.hp {
        let damage = old.player.hp - game.player.hp;
        if !game.player_damage_events.is_empty() {
            for event in &game.player_damage_events {
                let text = match event.source {
                    PlayerDamageSource::Mob(kind) => format!(
                        "{} hits: {} damage.",
                        MOBS[kind as usize].name, event.amount
                    ),
                    PlayerDamageSource::Trap => {
                        format!("Trap triggered: {} damage.", event.amount)
                    }
                    PlayerDamageSource::Starvation => {
                        format!("Starvation: {} damage.", event.amount)
                    }
                    PlayerDamageSource::Swallowed => {
                        format!("Digestive acids: {} damage.", event.amount)
                    }
                    PlayerDamageSource::Poison => format!("Poison: {} damage.", event.amount),
                };
                lines.push(EventLine {
                    text,
                    class: Some("flash"),
                });
            }
        } else {
            // A triggered trap is replaced by floor before the post-turn snapshot.
            // Detect it from the previous map before attributing generic HP loss to
            // a nearby hostile; otherwise an unrelated, unseen mob can appear to
            // attack the player from across the level.
            let triggered_damage_trap = old.floor == game.floor
                && old.map[game.player.cell as usize] == Tile::Trap
                && game.map[game.player.cell as usize] != Tile::Trap;
            let attacker = (!triggered_damage_trap)
                .then(|| {
                    game.mobs
                        .iter()
                        .filter(|mob| mob.hp > 0 && !mob.friendly)
                        .min_by_key(|mob| {
                            Game::distance(mob.cell as usize, game.player.cell as usize)
                        })
                })
                .flatten();
            lines.push(EventLine {
                text: if triggered_damage_trap {
                    format!("Trap triggered: {damage} damage.")
                } else {
                    attacker.map_or_else(
                        || format!("Agent takes {damage} damage."),
                        |mob| format!("{} hits: {damage} damage.", MOBS[mob.kind as usize].name),
                    )
                },
                class: Some("flash"),
            });
        }
    } else if old.player.hp < game.player.hp {
        lines.push(EventLine {
            text: format!("Recovered {} HP.", game.player.hp - old.player.hp),
            class: Some("good"),
        });
    }

    let old_counts = inventory_counts(old);
    let new_counts = inventory_counts(game);
    let mut arrival_supplies = false;
    for (gear, (&before, &after)) in old_counts.iter().zip(&new_counts).enumerate() {
        if after > before && !matches!(action, Some(Action::Buy(_))) {
            let amount = after - before;
            let item = if amount > 1 {
                format!("{} x{amount}", GEAR[gear].name)
            } else {
                GEAR[gear].name.to_owned()
            };
            if old.floor != game.floor {
                arrival_supplies = true;
            } else {
                lines.push(EventLine {
                    text: format!("Picked up {item}."),
                    class: None,
                });
            }
        }
    }
    if arrival_supplies {
        lines.push(EventLine {
            text: "MIB supplies received.".to_owned(),
            class: Some("good"),
        });
    }

    if game.player.level > old.player.level {
        lines.push(EventLine {
            text: format!("Level {}. +1 SP.", game.player.level),
            class: Some("good"),
        });
    }
    if game.player.credits > old.player.credits {
        lines.push(EventLine {
            text: format!("+{} credits.", game.player.credits - old.player.credits),
            class: Some("good"),
        });
    }

    const STATUS_NAMES: [&str; 7] = [
        "Haste",
        "Blindness",
        "Telepathy",
        "Hallucination",
        "Confusion",
        "Grabbed",
        "Swallowed",
    ];
    for (index, name) in STATUS_NAMES.iter().enumerate() {
        if old.player.status[index] <= 0 && game.player.status[index] > 0 {
            lines.push(EventLine {
                text: format!("{name} active."),
                class: Some(if index == 0 || index == 2 {
                    "good"
                } else {
                    "flash"
                }),
            });
        } else if old.player.status[index] > 0 && game.player.status[index] <= 0 {
            lines.push(EventLine {
                text: format!("{name} wears off."),
                class: None,
            });
        }
    }

    if let Some(action) = action {
        let identity = action.identity(old);
        let mut fields = identity.split(':');
        match fields.next().unwrap_or("") {
            "fire" if !mob_was_hit => lines.push(EventLine {
                text: "Shot missed or was blocked.".to_owned(),
                class: Some("warn"),
            }),
            "throw" if !mob_was_hit => lines.push(EventLine {
                text: format!("Threw {}.", fields.next().unwrap_or("device")),
                class: None,
            }),
            "eat" => lines.push(EventLine {
                text: format!("Consumed {}.", fields.next().unwrap_or("item")),
                class: None,
            }),
            "use" => lines.push(EventLine {
                text: format!("Activated {}.", fields.next().unwrap_or("tool")),
                class: Some("good"),
            }),
            "buy" => lines.push(EventLine {
                text: format!("Purchased {}.", fields.next().unwrap_or("item")),
                class: Some("good"),
            }),
            "wield" => lines.push(EventLine {
                text: format!("Wielding {}.", fields.next().unwrap_or("weapon")),
                class: None,
            }),
            "wear" => lines.push(EventLine {
                text: format!("Wearing {}.", fields.next().unwrap_or("armor")),
                class: None,
            }),
            _ => {}
        }
    }

    if game.player.won && !old.player.won {
        lines.push(EventLine {
            text: "Assignment complete.".to_owned(),
            class: Some("good"),
        });
    } else if game.player.dead && !old.player.dead {
        lines.push(EventLine {
            text: "Assignment failed. Agent down.".to_owned(),
            class: Some("flash"),
        });
    }

    out.push_str("\"logs\":[");
    for (index, line) in lines.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        out.push_str("{\"text\":\"");
        push_json_string(out, &line.text);
        out.push('"');
        if let Some(class) = line.class {
            write!(out, ",\"cls\":\"{class}\"").expect("event class");
        }
        out.push('}');
    }
    out.push_str("],");
}

fn inventory_counts(game: &Game) -> Vec<u32> {
    let mut counts = vec![0; GEAR.len()];
    for item in &game.player.inventory {
        counts[item.gear as usize] += u32::from(item.count.max(1));
    }
    counts
}

fn action_target(action: Option<&Action>) -> Option<u16> {
    match action {
        Some(Action::Fire(cell) | Action::Throw(_, cell)) => Some(*cell as u16),
        _ => None,
    }
}

fn signature_action(signature: &str, game: &Game) -> Action {
    let mut fields = signature.split(':');
    match fields.next() {
        Some("command") => fields
            .next()
            .and_then(|value| value.chars().next())
            .map_or(Action::None, Action::Command),
        Some("fire") => fields
            .next()
            .and_then(parse_signature_cell)
            .map_or(Action::None, Action::Fire),
        Some("throw") => {
            let name = fields.next().unwrap_or("");
            let cell = fields.next().and_then(parse_signature_cell);
            game.player
                .inventory
                .iter()
                .find(|item| item.spec().name == name)
                .and_then(|item| cell.map(|cell| Action::Throw(item.uid, cell)))
                .unwrap_or(Action::None)
        }
        Some(kind @ ("eat" | "use" | "wield" | "wear")) => {
            let name = fields.next().unwrap_or("");
            game.player
                .inventory
                .iter()
                .find(|item| item.spec().name == name)
                .map_or(Action::None, |item| match kind {
                    "eat" => Action::Eat(item.uid),
                    "use" => Action::Use(item.uid),
                    "wield" => Action::Wield(item.uid),
                    "wear" => Action::Wear(item.uid),
                    _ => Action::None,
                })
        }
        Some("buy") => {
            let name = fields.next().unwrap_or("");
            game.shop_room
                .and_then(|room| {
                    game.rooms[room]
                        .stock
                        .iter()
                        .find(|item| item.spec().name == name)
                })
                .map_or(Action::None, |item| Action::Buy(item.uid))
        }
        _ => Action::None,
    }
}

fn parse_signature_cell(value: &str) -> Option<usize> {
    let (x, y) = value.split_once(',')?;
    let x = x.parse::<usize>().ok()?;
    let y = y.parse::<usize>().ok()?;
    (x < WIDTH && y < HEIGHT).then_some(y * WIDTH + x)
}

fn adjacent(from: u16, to: u16) -> bool {
    let (fx, fy) = mib_rust::coordinates(from as usize);
    let (tx, ty) = mib_rust::coordinates(to as usize);
    from != to && fx.abs_diff(tx) <= 1 && fy.abs_diff(ty) <= 1
}

fn actor_direction(from: u16, to: u16, target: Option<u16>) -> u8 {
    let origin = if from != to { from } else { to };
    let destination = if from != to { to } else { target.unwrap_or(to) };
    let (ox, oy) = mib_rust::coordinates(origin as usize);
    let (dx, dy) = mib_rust::coordinates(destination as usize);
    let x = dx as i16 - ox as i16;
    let y = dy as i16 - oy as i16;
    if x.abs() >= y.abs() {
        if x >= 0 { 1 } else { 3 }
    } else if y >= 0 {
        2
    } else {
        0
    }
}

fn push_json_string(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => write!(out, "\\u{:04x}", ch as u32).expect("escape"),
            ch => out.push(ch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_protocol_contains_render_state() {
        let game = Game::start(1_704_334, ClassId::Agent);
        let json = snapshot_json(&game, None, None, 1_704_334, ClassId::Agent, 0, 314);
        assert!(json.starts_with("{\"version\":1"));
        assert!(json.contains("\"class\":\"a\""));
        assert!(json.contains("\"map\":\""));
        assert!(json.contains("\"mobs\":["));
        assert!(json.contains("\"logs\":[]"));
        assert_eq!(json.matches("\"width\":64").count(), 1);
    }

    #[test]
    fn snapshot_protocol_reports_all_detectable_turn_events() {
        let old = Game::start(1_704_334, ClassId::Agent);
        let mut game = old.clone();
        game.player.hp -= 3;
        game.player.credits += 7;
        let mob = game.mobs.first_mut().expect("seed has a mob");
        mob.hp -= 2;
        let action = Action::Command('.');
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&action),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );
        assert!(json.contains("\"logs\":[{"));
        assert!(json.contains("Hit "));
        assert!(json.contains("3 damage."));
        assert!(json.contains("+7 credits."));
        assert!(json.contains("\"cls\":\"flash\""));
    }

    #[test]
    fn snapshot_protocol_attributes_triggered_trap_damage_without_inventing_attacker() {
        let mut old = Game::start(1_704_334, ClassId::Agent);
        old.map[old.player.cell as usize] = Tile::Trap;
        let mut game = old.clone();
        game.map[game.player.cell as usize] = Tile::Floor;
        game.player.hp -= 4;
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&Action::Command('.')),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );

        assert!(json.contains("Trap triggered: 4 damage."));
        assert!(!json.contains(" hits: 4 damage."));
    }

    #[test]
    fn snapshot_protocol_uses_recorded_non_mob_damage_source() {
        let old = Game::start(1_704_334, ClassId::Agent);
        let mut game = old.clone();
        game.player.hp -= 3;
        game.player_damage_events
            .push(mib_rust::world::PlayerDamageEvent {
                source: PlayerDamageSource::Swallowed,
                amount: 3,
            });
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&Action::Command('.')),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );

        assert!(json.contains("Digestive acids: 3 damage."));
        assert!(!json.contains(" hits: 3 damage."));
    }

    #[test]
    fn snapshot_protocol_logs_non_adjacent_player_teleport() {
        let old = Game::start(1_704_334, ClassId::Agent);
        let mut game = old.clone();
        game.player.cell = if old.player.cell < 64 {
            old.player.cell + 128
        } else {
            old.player.cell - 128
        };
        let action = Action::Command('.');
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&action),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );

        assert!(json.contains("\"teleported\":true"));
        assert!(json.contains("\"text\":\"Teleported.\",\"cls\":\"good\""));
    }

    #[test]
    fn snapshot_protocol_summarizes_verified_floor_supplies() {
        let mut old = Game::start(1_704_334, ClassId::Agent);
        old.player.inventory.clear();
        old.player.update_burden();
        let mut game = old.clone();
        game.generate_floor(8);
        game.player.cell = game.up_stairs.expect("floor has arrival stairs");
        let mut floor_supplies = vec![0_u32; GEAR.len()];
        for item in game
            .items
            .iter()
            .filter(|item| item.cell == game.player.cell)
        {
            floor_supplies[item.gear as usize] += u32::from(item.count.max(1));
        }
        assert!(game.auto_pick_up());
        let actual_inventory = inventory_counts(&game);
        assert_eq!(actual_inventory, floor_supplies);

        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&Action::Command('>')),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );

        assert_eq!(json.matches("MIB supplies received.").count(), 1);
        assert!(!json.contains("\"text\":\"Picked up "));
    }

    #[test]
    fn snapshot_protocol_keeps_winning_player_idle() {
        let old = Game::start(1_704_334, ClassId::Agent);
        let mut game = old.clone();
        game.player.won = true;
        game.player.dead = true;
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&Action::Fire(game.player.cell as usize)),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );
        let player_json = json.split("\"action\"").next().expect("player JSON");

        assert!(json.contains("\"won\":true,\"dead\":true"));
        assert!(player_json.contains("\"state\":0"));
    }

    #[test]
    fn snapshot_protocol_keeps_losing_player_dead() {
        let old = Game::start(1_704_334, ClassId::Agent);
        let mut game = old.clone();
        game.player.dead = true;
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&Action::Command('.')),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );
        let player_json = json.split("\"action\"").next().expect("player JSON");

        assert!(player_json.contains("\"state\":5"));
    }

    #[test]
    fn snapshot_protocol_uses_reachable_mob_status_poses() {
        let mut old = Game::start(1_704_334, ClassId::Agent);
        old.mobs.truncate(1);
        let mut game = old.clone();
        game.mobs[0].pacified = true;
        game.mobs[0].asleep = true;
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&Action::Command('.')),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );
        let mob_json = json.split("\"mobs\":[").nth(1).expect("mob JSON");
        assert!(mob_json.starts_with("{\"uid\":"));
        assert!(mob_json.contains("\"state\":6"), "{mob_json}");

        game.mobs[0].pacified = false;
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&Action::Command('.')),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );
        let mob_json = json.split("\"mobs\":[").nth(1).expect("mob JSON");
        assert!(mob_json.contains("\"state\":5"), "{mob_json}");

        game.mobs[0].pacified = true;
        game.mobs[0].frozen = 2;
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&Action::Command('.')),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );
        let mob_json = json.split("\"mobs\":[").nth(1).expect("mob JSON");
        assert!(mob_json.contains("\"state\":4"), "{mob_json}");
    }

    #[test]
    fn snapshot_protocol_keeps_removed_enemy_in_dead_pose_for_transition() {
        let mut old = Game::start(1_704_334, ClassId::Agent);
        old.mobs.truncate(1);
        let mut game = old.clone();
        game.mobs.clear();
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&Action::Fire(old.mobs[0].cell as usize)),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );
        let mob_json = json.split("\"mobs\":[").nth(1).expect("mob JSON");

        assert!(mob_json.contains("\"state\":7"), "{mob_json}");
        assert!(mob_json.contains("\"hp\":0"), "{mob_json}");
    }

    #[test]
    fn snapshot_protocol_faces_shot_target_instead_of_opposite_recoil() {
        let old = Game::start(1_704_334, ClassId::Agent);
        let mut game = old.clone();
        let player_cell = old.player.cell;
        let (player_x, _) = mib_rust::coordinates(player_cell as usize);
        assert!(player_x > 0 && player_x + 3 < WIDTH);
        game.player.cell = player_cell - 1;
        let target = player_cell + 3;
        let expected = actor_direction(game.player.cell, game.player.cell, Some(target));
        let recoil = actor_direction(player_cell, game.player.cell, Some(target));
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&Action::Fire(target as usize)),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );
        let player_json = json.split("\"action\"").next().expect("player JSON");

        assert_ne!(expected, recoil);
        assert!(player_json.contains(&format!("\"state\":3,\"direction\":{expected}")));
    }

    #[test]
    fn snapshot_protocol_keeps_player_facing_enemy_killed_by_melee() {
        let mut old = Game::start(1_704_334, ClassId::Agent);
        old.mobs.truncate(2);
        let player_cell = old.player.cell;
        let (player_x, _) = mib_rust::coordinates(player_cell as usize);
        assert!(player_x > 0 && player_x + 1 < WIDTH);
        let killed_cell = player_cell + 1;
        let other_cell = player_cell - 1;
        old.mobs[0].cell = killed_cell;
        old.mobs[0].asleep = false;
        old.mobs[1].cell = other_cell;
        old.mobs[1].asleep = false;
        old.mobs[1].hunting = true;
        let mut game = old.clone();
        game.mobs.remove(0);
        game.visible[killed_cell as usize] = true;
        game.visible[other_cell as usize] = true;
        let expected_direction = actor_direction(player_cell, player_cell, Some(killed_cell));
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&Action::Command('l')),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );
        let player_json = json.split("\"action\"").next().expect("player JSON");

        assert!(player_json.contains(&format!("\"state\":2,\"direction\":{expected_direction}")));
    }

    #[test]
    fn snapshot_protocol_faces_player_and_mob_toward_each_other_during_melee() {
        let mut old = Game::start(1_704_334, ClassId::Agent);
        let player_cell = old.player.cell;
        let (player_x, _) = mib_rust::coordinates(player_cell as usize);
        let target_cell = if player_x + 1 < WIDTH {
            player_cell + 1
        } else {
            player_cell - 1
        };
        old.mobs[0].cell = target_cell;
        let mut game = old.clone();
        game.mobs[0].hp -= 1;
        let expected_player_direction =
            actor_direction(player_cell, player_cell, Some(target_cell));
        let expected_mob_direction = actor_direction(target_cell, target_cell, Some(player_cell));
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&Action::Command('.')),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );
        let player_json = json.split("\"action\"").next().expect("player JSON");
        let mob_json = json.split("\"mobs\":[").nth(1).expect("mob JSON");

        assert!(player_json.contains(&format!(
            "\"state\":2,\"direction\":{expected_player_direction}"
        )));
        assert!(mob_json.contains(&format!(
            "\"state\":3,\"direction\":{expected_mob_direction}"
        )));
        assert_ne!(expected_player_direction, expected_mob_direction);
    }

    #[test]
    fn snapshot_protocol_keeps_engaged_stationary_actors_facing_combat() {
        let mut game = Game::start(1_704_334, ClassId::Agent);
        let player_cell = game.player.cell;
        let (player_x, _) = mib_rust::coordinates(player_cell as usize);
        let target_cell = if player_x + 1 < WIDTH {
            player_cell + 1
        } else {
            player_cell - 1
        };
        game.mobs[0].cell = target_cell;
        game.mobs[0].hunting = true;
        game.mobs[0].target_cell = Some(player_cell);
        game.mobs[0].asleep = false;
        game.mobs[0].pacified = false;
        game.mobs[0].frozen = 0;
        game.visible[target_cell as usize] = true;
        let old = game.clone();
        let expected_player_direction =
            actor_direction(player_cell, player_cell, Some(target_cell));
        let expected_mob_direction = actor_direction(target_cell, target_cell, Some(player_cell));
        let json = snapshot_json(
            &game,
            Some(&old),
            Some(&Action::Command('.')),
            1_704_334,
            ClassId::Agent,
            1,
            314,
        );
        let player_json = json.split("\"action\"").next().expect("player JSON");
        let mob_json = json.split("\"mobs\":[").nth(1).expect("mob JSON");

        assert!(player_json.contains(&format!(
            "\"state\":0,\"direction\":{expected_player_direction}"
        )));
        assert!(mob_json.contains(&format!(
            "\"state\":0,\"direction\":{expected_mob_direction}"
        )));
    }

    #[test]
    fn human_action_invalidates_and_rebuilds_ensemble_plan() {
        let mut episode = RustEpisode::new(1_704_334, "a").expect("episode");
        episode.replan(2);
        assert!(!episode.plan.is_empty());
        assert_ne!(episode.selected_policy, "human");

        let before = episode.game.turns;
        episode
            .apply_action_signature_json("command:.")
            .expect("human wait action");
        assert!(episode.plan.is_empty());
        assert_eq!(episode.selected_policy, "human");
        assert!(episode.game.turns > before);

        let human_state = episode.game.clone();
        episode.replan(2);
        assert!(!episode.plan.is_empty());
        assert_ne!(episode.selected_policy, "human");
        assert!(episode.plan[0].0.turns > human_state.turns);
    }
}
