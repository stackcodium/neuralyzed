#[derive(Clone)]
struct LookaheadCandidate {
    action: Action,
    is_baseline: bool,
}

struct LookaheadStart {
    score: i64,
    turn: u32,
    floor: u8,
    deepest: u8,
    hp: i16,
    xp: u32,
    kills: u16,
    inventory: usize,
    seen: usize,
    hostile_hp: i32,
}

impl LookaheadStart {
    fn capture(game: &Game) -> Self {
        Self {
            score: i64::from(game.score()),
            turn: game.turns,
            floor: game.floor,
            deepest: game.player.deepest,
            hp: game.player.hp,
            xp: game.player.xp,
            kills: game.player.kills,
            inventory: game.player.inventory.len(),
            seen: game.seen.iter().filter(|&&seen| seen).count(),
            hostile_hp: hostile_hp(game),
        }
    }
}

fn should_predict(game: &Game) -> bool {
    let visible = lookahead_visible_hostiles(game);
    if visible.is_empty() {
        return false;
    }
    let closest = visible
        .iter()
        .map(|&index| Game::distance(game.player.cell as usize, game.mobs[index].cell as usize))
        .min()
        .unwrap_or(usize::MAX);
    let hp = i32::from(game.player.hp);
    let max_hp = i32::from(game.player.max_hp.max(1));
    closest <= 1
        || hp * 100 <= max_hp * 62 && closest <= 5
        || game.floor >= 10 && hp * 100 <= max_hp * 78 && closest <= 4
}

fn should_trust_baseline(game: &Game, action: &Action) -> bool {
    let visible = lookahead_visible_hostiles(game);
    if game.floor <= 4
        && matches!(action, Action::Command('h' | 'j' | 'n' | 'u' | 'k'))
        && matching_ammo_count(game) == 0
        && game.player.inventory.iter().any(|item| {
            item.gear.kind() == GearKind::Weapon
                && Some(item.uid) != game.player.wielded
                && item.spec().range > 0
                && game.player.ammo_count(item.spec().ammo) > 0
        })
        && visible.iter().any(|&index| {
            let (px, py) = coordinates(game.player.cell as usize);
            let (mx, my) = coordinates(game.mobs[index].cell as usize);
            Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 1
                || mx == px && my + 2 == py
        })
    {
        return true;
    }
    if game.floor == 14
        && game.player.max_hp >= 50
        && game.player.hp * 100 <= game.player.max_hp * 72
        && matches!(action, Action::Eat(uid) if equipped_item(game, Some(*uid)).is_some_and(|item| item.gear == GearId::RoyalJelly))
        && visible.iter().all(|&index| {
            Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) > 1
        })
    {
        return true;
    }
    let Some(closest) = visible
        .iter()
        .map(|&index| Game::distance(game.player.cell as usize, game.mobs[index].cell as usize))
        .min()
    else {
        return false;
    };
    let hp = i32::from(game.player.hp);
    let max_hp = i32::from(game.player.max_hp.max(1));
    let close_combat = closest <= 1 || hp * 100 <= max_hp * 55 && closest <= 2;
    if matches!(action, Action::Command(_))
        && closest <= 1
        && hp * 10 >= max_hp * 9
        && ranged_ready(game).is_some()
        && visible.iter().all(|&index| game.mobs[index].frozen > 0)
    {
        return true;
    }
    if game.floor == 10
        && matches!(action, Action::Command(_))
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            mob.boss && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
        })
        && visible
            .iter()
            .any(|&index| !game.mobs[index].boss && game.mobs[index].frozen > 0)
    {
        return true;
    }
    if game.floor == 11
        && matches!(action, Action::Command(_))
        && !game
            .player
            .inventory
            .iter()
            .any(|item| item.gear == GearId::FoamGrenade)
        && visible.iter().any(|&index| {
            Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) == 1
        })
        && game
            .down_stairs
            .is_some_and(|stairs| Game::distance(game.player.cell as usize, stairs as usize) <= 4)
    {
        return true;
    }
    if game.floor == 14
        && matches!(action, Action::Command(_))
        && game.player.hp == game.player.max_hp
        && game.player.max_hp >= 50
        && matching_ammo_count(game) >= 150
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            mob.frozen > 0
                && mob.hp > 20
                && Game::distance(game.player.cell as usize, mob.cell as usize) <= 2
        })
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            mob.frozen <= 0
                && MOBS[mob.kind as usize].tier >= 3
                && Game::distance(game.player.cell as usize, mob.cell as usize) <= 3
        })
    {
        return true;
    }
    close_combat
        && (matches!(
            action,
            Action::Throw(_, _) | Action::Use(_) | Action::Eat(_)
        ) || matches!(action, Action::Fire(_)) && closest <= 2)
}

fn needs_post_lookahead_rng_draw(game: &Game, action: &Action) -> bool {
    game.floor == 11
        && matches!(action, Action::Command(_))
        && !game
            .player
            .inventory
            .iter()
            .any(|item| item.gear == GearId::FoamGrenade)
        && game.mobs.iter().any(|mob| {
            mob.hp > 0
                && !mob.friendly
                && !mob.pacified
                && game.visible[mob.cell as usize]
                && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
        })
        && game
            .down_stairs
            .is_some_and(|stairs| Game::distance(game.player.cell as usize, stairs as usize) <= 4)
}

fn enumerate_lookahead_candidates(game: &Game, bot: &Bot) -> Vec<LookaheadCandidate> {
    let mut candidates = Vec::with_capacity(24);
    let mut baseline_game = game.clone();
    let mut baseline_bot = bot.clone();
    candidates.push(LookaheadCandidate {
        action: baseline_bot.choose(&mut baseline_game),
        is_baseline: true,
    });

    let mut shootable = lookahead_visible_hostiles(game);
    if let Some(range) = ranged_ready(game) {
        shootable.retain(|&index| {
            let cell = game.mobs[index].cell as usize;
            Game::distance(game.player.cell as usize, cell) <= range
                && game.line_clear(game.player.cell as usize, cell, true)
        });
        shootable.sort_by(|&a, &b| {
            tactical_threat_score(game, b).total_cmp(&tactical_threat_score(game, a))
        });
        for index in shootable.into_iter().take(4) {
            candidates.push(LookaheadCandidate {
                action: Action::Fire(game.mobs[index].cell as usize),
                is_baseline: false,
            });
        }
    }

    let visible = lookahead_visible_hostiles(game);
    let close_threat = visible.iter().any(|&index| {
        Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 2
    });
    let foam_target = visible
        .iter()
        .copied()
        .filter(|&index| {
            let mob = &game.mobs[index];
            mob.frozen <= 0
                && Game::distance(game.player.cell as usize, mob.cell as usize) <= throw_range(game)
                && game.line_clear(game.player.cell as usize, mob.cell as usize, true)
        })
        .max_by(|&a, &b| tactical_threat_score(game, a).total_cmp(&tactical_threat_score(game, b)));
    let hp_low = i32::from(game.player.hp) * 100 < i32::from(game.player.max_hp.max(1)) * 85;
    // A pocket universe is an escape/loop-break tool, not a generic combat
    // move. Offering it for every nearby enemy made wide lookahead spend one
    // at full health against a lone ordinary target surprisingly often.
    let teleport_warranted = teleport_candidate_warranted(game, bot, &visible);
    let mut tactical_items: Vec<_> = game
        .player
        .inventory
        .iter()
        .filter(|item| {
            item.gear.kind() == GearKind::Food
                && (item.spec().heal > 0 || game.player.has_skill(SkillId::Fieldsurgeon))
                && hp_low
                || close_threat
                    && (matches!(item.gear, GearId::FoamGrenade | GearId::NeuralyzerCharge)
                        || item.gear == GearId::PocketUniverse && teleport_warranted)
        })
        .collect();
    tactical_items.sort_by_key(|item| std::cmp::Reverse(tactical_item_score(game, item)));
    for item in tactical_items.into_iter().take(6) {
        let action = match item.gear {
            GearId::FoamGrenade => {
                foam_target.map(|target| Action::Throw(item.uid, game.mobs[target].cell as usize))
            }
            GearId::PocketUniverse | GearId::NeuralyzerCharge => Some(Action::Use(item.uid)),
            _ if item.gear.kind() == GearKind::Food => Some(Action::Eat(item.uid)),
            _ => None,
        };
        if let Some(action) = action {
            candidates.push(LookaheadCandidate {
                action,
                is_baseline: false,
            });
        }
    }

    let current_ready =
        equipped_item(game, game.player.wielded).map_or(0.0, |item| weapon_ready_score(game, item));
    let mut weapons: Vec<_> = game
        .player
        .inventory
        .iter()
        .filter(|item| {
            item.gear.kind() == GearKind::Weapon
                && !item.cursed
                && Some(item.uid) != game.player.wielded
                && weapon_ready_score(game, item) > current_ready + 1.0
        })
        .collect();
    weapons.sort_by(|a, b| weapon_ready_score(game, b).total_cmp(&weapon_ready_score(game, a)));
    for weapon in weapons.into_iter().take(3) {
        candidates.push(LookaheadCandidate {
            action: Action::Wield(weapon.uid),
            is_baseline: false,
        });
    }

    for key in ['h', 'l', 'k', 'j', 'y', 'u', 'b', 'n', '.'] {
        if key != '.' && !can_lookahead_command(game, key) {
            continue;
        }
        candidates.push(LookaheadCandidate {
            action: Action::Command(key),
            is_baseline: false,
        });
    }
    if game.map[game.player.cell as usize] == Tile::DownStairs && game.floor < 15 {
        candidates.push(LookaheadCandidate {
            action: Action::Command('>'),
            is_baseline: false,
        });
    }
    if game.map[game.player.cell as usize] == Tile::UpStairs && game.floor > 1 {
        candidates.push(LookaheadCandidate {
            action: Action::Command('<'),
            is_baseline: false,
        });
    }
    dedupe_lookahead_candidates(candidates, game)
}

fn teleport_candidate_warranted(game: &Game, bot: &Bot, visible: &[usize]) -> bool {
    let hp_percent = i32::from(game.player.hp) * 100 / i32::from(game.player.max_hp.max(1));
    let adjacent = visible.iter().filter(|&&index| {
        game.mobs[index].frozen <= 0
            && Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 1
    }).count();
    let close = visible.iter().filter(|&&index| {
        game.mobs[index].frozen <= 0
            && Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 2
    }).count();
    let adjacent_boss = visible.iter().any(|&index| {
        let mob = &game.mobs[index];
        mob.boss && mob.frozen <= 0
            && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
    });
    let floor_ten_reset = game.floor == 10
        && adjacent_boss
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            !mob.boss && mob.frozen > 0
                && Game::distance(game.player.cell as usize, mob.cell as usize) >= 3
        });

    bot.stationary_actions >= 6
        || hp_percent <= 70 && close > 0
        || adjacent >= 2
        || close >= 3
        || adjacent_boss && (hp_percent <= 85 || game.floor == 15)
        || floor_ten_reset
}

fn select_lookahead_candidates(
    candidates: Vec<LookaheadCandidate>,
    game: &Game,
    bot: &Bot,
    max_branches: usize,
) -> Vec<LookaheadCandidate> {
    let baseline = candidates
        .iter()
        .find(|candidate| candidate.is_baseline)
        .cloned();
    let mut scored: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| {
            !candidate.is_baseline && !reckless_close_candidate(game, &candidate.action)
        })
        .map(|candidate| {
            let mut trial_game = game.clone();
            let mut trial_bot = bot.clone();
            let start = LookaheadStart::capture(&trial_game);
            trial_bot.apply(&mut trial_game, candidate.action.clone());
            let score = score_lookahead(&start, &trial_game);
            (candidate, score)
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut selected = Vec::with_capacity(max_branches);
    if let Some(baseline) = baseline {
        selected.push(baseline);
    }
    let remaining = max_branches.saturating_sub(selected.len());
    selected.extend(
        scored
            .into_iter()
            .take(remaining)
            .map(|(candidate, _)| candidate),
    );
    selected
}

fn dedupe_lookahead_candidates(
    candidates: Vec<LookaheadCandidate>,
    game: &Game,
) -> Vec<LookaheadCandidate> {
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.action.identity(game)))
        .collect()
}

fn reckless_close_candidate(game: &Game, action: &Action) -> bool {
    let Action::Command(key) = action else {
        return false;
    };
    let threats = lookahead_visible_hostiles(game);
    if threats.is_empty() {
        return false;
    }
    let closest = threats
        .iter()
        .map(|&index| Game::distance(game.player.cell as usize, game.mobs[index].cell as usize))
        .min()
        .unwrap_or(99);
    if *key == '.' && closest <= 2 {
        return true;
    }
    if i32::from(game.player.hp) * 100 > i32::from(game.player.max_hp.max(1)) * 42 || closest > 1 {
        return false;
    }
    let Some(next) = command_cell(game.player.cell as usize, *key) else {
        return false;
    };
    if let Some(target) = game.mob_at(next)
        && !game.mobs[target].friendly
        && !game.mobs[target].pacified
    {
        return ranged_ready(game).is_some()
            && tactical_threat_score(game, target) >= 45.0
            && game.mobs[target].hp > 8;
    }
    let current_adjacent = threats
        .iter()
        .filter(|&&index| {
            Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 1
        })
        .count();
    let next_adjacent = threats
        .iter()
        .filter(|&&index| Game::distance(next, game.mobs[index].cell as usize) <= 1)
        .count();
    let current_close = threats
        .iter()
        .filter(|&&index| {
            Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 2
        })
        .count();
    let next_close = threats
        .iter()
        .filter(|&&index| Game::distance(next, game.mobs[index].cell as usize) <= 2)
        .count();
    !(next_adjacent < current_adjacent
        || next_adjacent <= current_adjacent && next_close < current_close)
}

fn score_lookahead(start: &LookaheadStart, game: &Game) -> f64 {
    let visible = lookahead_visible_hostiles(game);
    let nearest = visible
        .iter()
        .map(|&index| Game::distance(game.player.cell as usize, game.mobs[index].cell as usize))
        .min()
        .unwrap_or(9) as f64;
    let adjacent_danger = visible
        .iter()
        .filter(|&&index| {
            Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 1
                && tactical_threat_score(game, index) >= 45.0
        })
        .count() as f64;
    let close_danger = visible
        .iter()
        .filter(|&&index| {
            Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 2
                && tactical_threat_score(game, index) >= 45.0
        })
        .count() as f64;
    let matching_ammo = matching_ammo_count(game) as f64;
    let control = control_count(game) as f64;
    let healing = healing_count(game) as f64;
    let hp_pct = f64::from(game.player.hp) / f64::from(game.player.max_hp.max(1));
    let no_loaded_plan = game.floor >= 10
        && game.player.inventory.iter().any(|item| {
            item.gear.kind() == GearKind::Weapon
                && item.spec().range > 0
                && item.spec().flags & gear_flags::MELEE == 0
        })
        && matching_ammo <= 0.0
        && control <= 0.0;
    (if game.player.won { 180_000.0 } else { 0.0 })
        + (if game.player.dead && !game.player.won {
            -120_000.0
        } else {
            0.0
        })
        + (i64::from(game.score()) - start.score) as f64
        + f64::from(game.player.deepest as i16 - start.deepest as i16) * 1400.0
        + f64::from(game.floor as i16 - start.floor as i16) * 240.0
        + f64::from(game.player.hp - start.hp) * 16.0
        + hp_pct * 260.0
        + healing.min(8.0) * 18.0
        + control.min(6.0) * 75.0
        + matching_ammo.min(80.0) * if game.floor >= 10 { 3.0 } else { 1.2 }
        + nearest * 20.0
        + if hp_pct < 0.35 { -2200.0 } else { 0.0 }
        + if hp_pct < 0.55 && control <= 0.0 {
            -850.0
        } else {
            0.0
        }
        + if no_loaded_plan { -1800.0 } else { 0.0 }
        - adjacent_danger * 2200.0
        - close_danger * 650.0
        + (i64::from(game.player.xp) - i64::from(start.xp)) as f64 * 5.0
        + (i32::from(game.player.kills) - i32::from(start.kills)) as f64 * 24.0
        + (game.player.inventory.len() as isize - start.inventory as isize) as f64 * 10.0
        + f64::from(start.hostile_hp - hostile_hp(game)) * 3.0
        + (game.seen.iter().filter(|&&seen| seen).count() as isize - start.seen as isize) as f64
            * 0.6
        - f64::from(game.turns.saturating_sub(start.turn)) * 0.35
}

fn lookahead_override_margin(game: &Game) -> f64 {
    let visible = lookahead_visible_hostiles(game);
    let closest = visible
        .iter()
        .map(|&index| Game::distance(game.player.cell as usize, game.mobs[index].cell as usize))
        .min()
        .unwrap_or(99);
    let hp = i32::from(game.player.hp);
    let max_hp = i32::from(game.player.max_hp.max(1));
    if closest <= 1 && hp * 100 <= max_hp * 40 {
        1800.0
    } else if closest <= 1 && hp * 100 <= max_hp * 60 {
        650.0
    } else if closest <= 2 && hp * 100 <= max_hp * 45 {
        450.0
    } else {
        35.0
    }
}

fn lookahead_visible_hostiles(game: &Game) -> Vec<usize> {
    game.mobs
        .iter()
        .enumerate()
        .filter_map(|(index, mob)| {
            (mob.hp > 0 && !mob.friendly && !mob.pacified && game.visible[mob.cell as usize])
                .then_some(index)
        })
        .collect()
}

fn tactical_threat_score(game: &Game, index: usize) -> f64 {
    let mob = &game.mobs[index];
    let spec = MOBS[mob.kind as usize];
    let prefix = if mob.prefix > 0 || mob.modifier.is_some() {
        18.0
    } else {
        0.0
    };
    let special = if spec.flags & mob_flags_for_lookahead() != 0 || spec.ranged > 0 || mob.boss {
        18.0
    } else {
        0.0
    };
    (if mob.boss { 60.0 } else { 0.0 })
        + f64::from(spec.tier) * 16.0
        + prefix
        + special
        + (f64::from(mob.hp) / f64::from(game.player.hp.max(1)) * 18.0).min(35.0)
}

fn mob_is_dangerous(game: &Game, index: usize) -> bool {
    let mob = &game.mobs[index];
    let spec = MOBS[mob.kind as usize];
    let dangerous_prefix = mob.prefix >= 2;
    let dangerous_modifier = matches!(
        mob.modifier,
        Some(
            crate::data::MobMod::Venomous
                | crate::data::MobMod::AcidBlooded
                | crate::data::MobMod::SporeLaden
        )
    );
    let special = spec.name.contains("jeebs")
        || spec.name.contains("sentinel")
        || spec.name.contains("bug")
        || spec.flags & crate::data::mob_flags::REGEN != 0
        || spec.ranged > 0
        || mob.boss;
    let close = Game::distance(game.player.cell as usize, mob.cell as usize) <= 3;
    dangerous_prefix && close
        || dangerous_modifier
        || special
        || mob.hp * 100 >= game.player.hp * 65
        || spec.tier >= 2
}

fn should_spend_foam_on_threat(game: &Game, target: usize, visible: &[usize]) -> bool {
    let mob = &game.mobs[target];
    let distance = Game::distance(game.player.cell as usize, mob.cell as usize);
    let close_active = visible
        .iter()
        .filter(|&&index| {
            game.mobs[index].frozen <= 0
                && Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 2
        })
        .count();
    let boss_floor = if game
        .mobs
        .iter()
        .any(|candidate| candidate.hp > 0 && candidate.boss)
        && matches!(game.floor, 5 | 10 | 15)
    {
        game.floor
    } else if matches!(game.floor + 1, 5 | 10 | 15) {
        game.floor + 1
    } else {
        0
    };
    if !mob.boss && boss_floor > 0 {
        let required = if boss_floor == 15 { 2 } else { 1 };
        if boss_control_count(game) <= required {
            let immediate = distance <= 1
                || visible.iter().any(|&index| {
                    game.mobs[index].frozen <= 0
                        && MOBS[game.mobs[index].kind as usize].tier >= 3
                        && Game::distance(game.player.cell as usize, game.mobs[index].cell as usize)
                            <= 1
                });
            let crowded = close_active >= 2 || visible.len() >= 3;
            if !(game.player.hp * 100 <= game.player.max_hp * 55
                || immediate && game.player.hp * 100 <= game.player.max_hp * 80
                || crowded && game.player.hp * 100 <= game.player.max_hp * 75)
            {
                return false;
            }
        }
    }
    mob.boss
        || MOBS[mob.kind as usize].tier >= 4
        || visible.iter().any(|&index| {
            game.mobs[index].frozen <= 0
                && (game.mobs[index].boss || MOBS[game.mobs[index].kind as usize].tier >= 4)
                && Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 2
        })
        || close_active >= 2
        || visible.len() >= 3
        || game.player.hp * 100 <= game.player.max_hp * 62
        || mob_is_dangerous(game, target)
        || visible.len() >= 2
}

const fn mob_flags_for_lookahead() -> u16 {
    crate::data::mob_flags::BUG | crate::data::mob_flags::REGEN | crate::data::mob_flags::SPLITS
}

fn tactical_item_score(game: &Game, item: &Item) -> i32 {
    match item.gear {
        GearId::PocketUniverse => 120,
        GearId::FoamGrenade => 100,
        GearId::NeuralyzerCharge => 80,
        _ => {
            i32::from(item.spec().heal)
                * if game.player.has_skill(SkillId::Fieldsurgeon) {
                    2
                } else {
                    1
                }
        }
    }
}

fn weapon_ready_score(game: &Game, item: &Item) -> f64 {
    let spec = item.spec();
    let average = f64::from(spec.damage[0] + spec.damage[1]) / 2.0;
    if spec.range > 0 && spec.flags & gear_flags::MELEE == 0 {
        let ammo = game.player.ammo_count(spec.ammo);
        if ammo == 0 {
            return 0.0;
        }
        average * f64::from(spec.burst.max(1))
            + f64::from(spec.range) * 1.8
            + f64::from(ammo.min(30))
    } else {
        average + f64::from(item.enchantment) + 6.0
    }
}

fn matching_ammo_count(game: &Game) -> u32 {
    equipped_item(game, game.player.wielded)
        .map_or(0, |weapon| game.player.ammo_count(weapon.spec().ammo))
}

fn resource_matching_ammo_count(game: &Game) -> u32 {
    let best_ready = game
        .player
        .inventory
        .iter()
        .filter(|weapon| {
            weapon.gear.kind() == GearKind::Weapon
                && weapon.spec().range > 0
                && weapon.spec().flags & gear_flags::MELEE == 0
        })
        .map(|weapon| game.player.ammo_count(weapon.spec().ammo))
        .max()
        .unwrap_or(0);
    matching_ammo_count(game).max(best_ready)
}

fn control_count(game: &Game) -> u32 {
    let inventory = game
        .player
        .inventory
        .iter()
        .filter(|item| {
            matches!(
                item.gear,
                GearId::FoamGrenade | GearId::NeuralyzerCharge | GearId::PocketUniverse
            )
        })
        .map(|item| u32::from(item.count.max(1)))
        .sum::<u32>();
    inventory
        + u32::from(game.player.has_skill(SkillId::Backup) && game.player.backup_cooldown <= 0)
}

fn boss_control_count(game: &Game) -> u32 {
    game.player
        .inventory
        .iter()
        .filter(|item| matches!(item.gear, GearId::FoamGrenade | GearId::PocketUniverse))
        .map(|item| u32::from(item.count.max(1)))
        .sum::<u32>()
        + u32::from(game.player.has_skill(SkillId::Backup) && game.player.backup_cooldown <= 0)
}

fn healing_count(game: &Game) -> u32 {
    game.player
        .inventory
        .iter()
        .filter(|item| {
            item.gear.kind() == GearKind::Food
                && (item.spec().heal > 0 || game.player.has_skill(SkillId::Fieldsurgeon))
        })
        .map(|item| u32::from(item.count.max(1)))
        .sum()
}

fn hostile_hp(game: &Game) -> i32 {
    game.mobs
        .iter()
        .filter(|mob| mob.hp > 0 && !mob.friendly && !mob.pacified)
        .map(|mob| i32::from(mob.hp))
        .sum()
}

fn can_lookahead_command(game: &Game, key: char) -> bool {
    let Some(cell) = command_cell(game.player.cell as usize, key) else {
        return false;
    };
    !game.blocked(cell)
        && game
            .mob_at(cell)
            .is_none_or(|index| !game.mobs[index].friendly)
}

fn command_cell(cell: usize, key: char) -> Option<usize> {
    let (x, y) = coordinates(cell);
    let (dx, dy) = match key {
        'h' => (-1, 0),
        'l' => (1, 0),
        'k' => (0, -1),
        'j' => (0, 1),
        'y' => (-1, -1),
        'u' => (1, -1),
        'b' => (-1, 1),
        'n' => (1, 1),
        _ => return None,
    };
    let nx = x as isize + dx;
    let ny = y as isize + dy;
    (nx >= 0 && ny >= 0 && nx < WIDTH as isize && ny < HEIGHT as isize)
        .then(|| index(nx as usize, ny as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ClassId;

    #[test]
    fn lone_adjacent_enemy_does_not_warrant_full_health_teleport_candidate() {
        let mut game = Game::start(1_706_023, ClassId::Agent);
        game.player.hp = game.player.max_hp;
        game.mobs[0].hp = game.mobs[0].max_hp.max(1);
        game.mobs[0].boss = false;
        game.mobs[0].frozen = 0;
        game.mobs[0].cell = game.player.cell + 1;

        assert!(!teleport_candidate_warranted(
            &game,
            &Bot::default(),
            &[0]
        ));
    }

    #[test]
    fn low_health_or_verified_loop_warrants_teleport_candidate() {
        let mut game = Game::start(1_706_023, ClassId::Agent);
        game.mobs[0].hp = game.mobs[0].max_hp.max(1);
        game.mobs[0].boss = false;
        game.mobs[0].frozen = 0;
        game.mobs[0].cell = game.player.cell + 1;
        game.player.hp = game.player.max_hp * 7 / 10;
        assert!(teleport_candidate_warranted(
            &game,
            &Bot::default(),
            &[0]
        ));

        game.player.hp = game.player.max_hp;
        let mut looped_bot = Bot::default();
        looped_bot.stationary_actions = 6;
        assert!(teleport_candidate_warranted(&game, &looped_bot, &[0]));
    }
}
