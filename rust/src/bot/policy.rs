fn visible_hostiles(game: &Game) -> Vec<usize> {
    game.mobs
        .iter()
        .enumerate()
        .filter_map(|(index, mob)| {
            (mob.hp > 0
                && !mob.friendly
                && !mob.pacified
                && game.visible[mob.cell as usize]
                && (!mob.asleep
                    || Game::distance(game.player.cell as usize, mob.cell as usize) <= 2))
                .then_some(index)
        })
        .collect()
}

fn alternating_tail(positions: &[u16]) -> bool {
    if positions.len() < 6 {
        return false;
    }
    let tail = &positions[positions.len() - 6..];
    tail[0] == tail[2]
        && tail[2] == tail[4]
        && tail[1] == tail[3]
        && tail[3] == tail[5]
        && tail[0] != tail[1]
}

fn stalled_tail(positions: &[u16]) -> bool {
    if positions.len() < 8 {
        return false;
    }
    let tail = &positions[positions.len() - 8..];
    let mut unique = [u16::MAX; 4];
    let mut count = 0;
    for &cell in tail {
        if unique[..count].contains(&cell) {
            continue;
        }
        if count == 3 {
            return false;
        }
        unique[count] = cell;
        count += 1;
    }
    true
}

fn local_orbit_tail(positions: &[u16]) -> bool {
    if positions.len() < 8 {
        return false;
    }
    let tail = &positions[positions.len() - 8..];
    let mut unique = [u16::MAX; 6];
    let mut count = 0;
    let mut min_x = usize::MAX;
    let mut max_x = 0;
    let mut min_y = usize::MAX;
    let mut max_y = 0;
    for &cell in tail {
        let (x, y) = coordinates(cell as usize);
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
        if unique[..count].contains(&cell) {
            continue;
        }
        if count == 5 {
            return false;
        }
        unique[count] = cell;
        count += 1;
    }
    tail.len() - count >= 3 && max_x - min_x < 5 && max_y - min_y < 5
}

fn revisited_tail(positions: &[u16]) -> bool {
    positions.len() >= 8 && positions[..2].contains(positions.last().expect("nonempty positions"))
}

fn adjacent_hostile(game: &Game) -> bool {
    game.mobs.iter().any(|mob| {
        mob.hp > 0
            && !mob.friendly
            && !mob.pacified
            && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
    })
}

fn active_boss(game: &Game) -> Option<usize> {
    game.mobs
        .iter()
        .find(|mob| mob.hp > 0 && mob.boss)
        .map(|mob| mob.cell as usize)
}

fn best_armor(game: &Game) -> Option<&Item> {
    let current = equipped_item(game, game.player.worn).map_or(0, |item| item.spec().armor);
    game.player
        .inventory
        .iter()
        .filter(|item| {
            item.gear.kind() == GearKind::Armor
                && !item.cursed
                && Some(item.uid) != game.player.worn
                && item.spec().armor > current
        })
        .max_by_key(|item| (item.spec().armor, -(item.spec().weight_tenths as i32)))
}

fn best_weapon(game: &Game) -> Option<&Item> {
    game.player
        .inventory
        .iter()
        .filter(|item| item.gear.kind() == GearKind::Weapon && !item.cursed)
        .filter(|item| {
            item.spec().flags & gear_flags::MELEE != 0
                || game.player.ammo_count(item.spec().ammo) > 0
        })
        .max_by_key(|item| weapon_score(item))
}

fn weapon_score(item: &Item) -> i32 {
    let spec = item.spec();
    i32::from(spec.damage[0] + spec.damage[1]) * i32::from(spec.burst.max(1))
        + i32::from(spec.range) * 3
        + i32::from(item.enchantment) * 4
        + if spec.flags & gear_flags::POLYMORPH != 0 {
            8
        } else {
            0
        }
}

fn current_weapon_score(game: &Game) -> i32 {
    equipped_item(game, game.player.wielded).map_or(0, weapon_score)
}

fn best_healing_food(game: &Game) -> Option<&Item> {
    game.player
        .inventory
        .iter()
        .filter(|item| item.gear.kind() == GearKind::Food && item.spec().heal > 0)
        .max_by_key(|item| item.spec().heal)
}

fn best_shop_item(game: &Game, room: usize) -> Option<&Item> {
    let carry_room = game
        .player
        .carry_capacity_tenths()
        .saturating_sub(game.player.carry_weight_tenths());
    game.rooms[room]
        .stock
        .iter()
        .filter(|item| {
            item.price > 0
                && item.price <= game.player.credits
                && u32::from(item.weight_tenths) <= carry_room + 20
        })
        .filter_map(|item| {
            let score = shop_value(game, item) - i32::from(item.price);
            (score > 10).then_some((item, score))
        })
        .max_by_key(|(_, score)| *score)
        .map(|(item, _)| item)
}

fn shop_value(game: &Game, item: &Item) -> i32 {
    let spec = item.spec();
    let ammo_count: u16 = game
        .player
        .inventory
        .iter()
        .filter(|held| held.gear.kind() == GearKind::Ammo)
        .map(|held| held.count)
        .sum();
    let healing = game
        .player
        .inventory
        .iter()
        .filter(|held| held.gear.kind() == GearKind::Food && held.spec().heal > 0)
        .count();
    let current_armor = equipped_item(game, game.player.worn).map_or(0, |held| held.spec().armor);
    let mut value = 0_i32;
    match item.gear.kind() {
        GearKind::Food => {
            value += if spec.heal > 0 {
                80 + i32::from(spec.heal) * 8
            } else {
                30
            };
            if game.player.nutrition < 1800 {
                value += ((1800 - i32::from(game.player.nutrition)) / 12).min(90);
            }
            if healing < 2 && spec.heal > 0 {
                value += 70;
            }
            if spec.haste > 0 {
                value += 35;
            }
        }
        GearKind::Ammo => {
            let matching_weapon =
                game.player.inventory.iter().any(|held| {
                    held.gear.kind() == GearKind::Weapon && held.spec().ammo == spec.ammo
                });
            if matching_weapon {
                value += if ammo_count < 12 {
                    140
                } else if ammo_count < 24 {
                    90
                } else {
                    35
                };
            }
            value += i32::from(item.count) * 3;
        }
        GearKind::Armor if spec.armor > current_armor => {
            value += 70 + i32::from(spec.armor - current_armor) * 65;
        }
        GearKind::Weapon => {
            let improved = weapon_score(item) - current_weapon_score(game);
            if improved > 0 {
                value += 60 + improved * 16;
            }
        }
        _ => {}
    }
    value += match item.gear {
        GearId::FoamGrenade => {
            if game
                .player
                .inventory
                .iter()
                .any(|held| held.gear == item.gear)
            {
                60
            } else {
                180
            }
        }
        GearId::NeuralyzerCharge => {
            if game
                .player
                .inventory
                .iter()
                .any(|held| held.gear == item.gear)
            {
                55
            } else {
                160
            }
        }
        GearId::PocketUniverse => {
            if game
                .player
                .inventory
                .iter()
                .any(|held| held.gear == item.gear)
            {
                35
            } else {
                120
            }
        }
        GearId::Scanner => {
            if game
                .player
                .inventory
                .iter()
                .any(|held| !held.identified || held.gear.kind() == GearKind::Pill)
            {
                55
            } else {
                10
            }
        }
        GearId::Deneuralyzer => {
            if game.player.status.iter().any(|&turns| turns > 0) {
                80
            } else {
                15
            }
        }
        _ => 0,
    };
    if game.floor <= 4
        && (matches!(item.gear, GearId::FoamGrenade | GearId::NeuralyzerCharge)
            || matches!(item.gear.kind(), GearKind::Food | GearKind::Ammo))
    {
        value += 30;
    }
    value
}

fn has_basic_kit(game: &Game) -> bool {
    let food = game.player.inventory.iter().any(|item| {
        item.gear.kind() == GearKind::Food && (item.spec().nutrition >= 300 || item.spec().heal > 0)
    }) || game.player.nutrition > 1050;
    let ranged = game.player.inventory.iter().any(|weapon| {
        weapon.gear.kind() == GearKind::Weapon
            && weapon.spec().range > 0
            && weapon.spec().flags & gear_flags::MELEE == 0
            && game.player.ammo_count(weapon.spec().ammo) > 0
    });
    let melee = game.player.inventory.iter().any(|weapon| {
        weapon.gear.kind() == GearKind::Weapon && weapon.spec().flags & gear_flags::MELEE != 0
    });
    let control = game
        .player
        .inventory
        .iter()
        .any(|item| matches!(item.gear, GearId::NeuralyzerCharge | GearId::FoamGrenade));
    food && (ranged || melee || control)
}

fn ready_for_next_floor(game: &Game) -> bool {
    let next_floor = game.floor + 1;
    let hp_threshold = if next_floor >= 6 { 72 } else { 62 };
    if game.player.hp * 100 < game.player.max_hp * hp_threshold {
        return false;
    }
    let has_ranged = game.player.inventory.iter().any(|item| {
        item.gear.kind() == GearKind::Weapon
            && item.spec().range > 0
            && item.spec().flags & gear_flags::MELEE == 0
    });
    let matching = resource_matching_ammo_count(game);
    let total_ammo = game
        .player
        .inventory
        .iter()
        .filter(|item| item.gear.kind() == GearKind::Ammo)
        .map(|item| u32::from(item.count))
        .sum::<u32>();
    let boss_ammo = match next_floor {
        5 => 12,
        10 => 18,
        15 => 50,
        _ => 4,
    };
    let heals = healing_count(game);
    let boss_control = boss_control_count(game);
    let final_ammo_covered = next_floor == 15 && matching >= 45 && heals >= 14 && boss_control >= 2;
    if !final_ammo_covered
        && (has_ranged && matching < boss_ammo || next_floor == 15 && total_ammo < boss_ammo)
    {
        return false;
    }
    let boss_heals = match next_floor {
        5 => 2,
        10 => 4,
        15 => 8,
        _ => 1,
    };
    let backup_ready = game.player.has_skill(SkillId::Backup) && game.player.backup_cooldown <= 0;
    if matches!(next_floor, 5 | 10 | 15) && heals < boss_heals {
        return false;
    }
    if matches!(next_floor, 5 | 10 | 15)
        && boss_control < if next_floor == 15 { 2 } else { 1 }
        && !backup_ready
    {
        return false;
    }
    let armor = equipped_item(game, game.player.worn).map_or(0, |item| item.spec().armor);
    if next_floor == 12 && heals < 3
        || next_floor >= 13 && heals < 6
        || next_floor >= 12 && armor < 3 && heals < 10
        || next_floor >= 14 && control_count(game) < 1
    {
        return false;
    }
    let best_burst_twice = game
        .player
        .inventory
        .iter()
        .filter(|item| {
            item.gear.kind() == GearKind::Weapon
                && item.spec().range > 0
                && item.spec().flags & gear_flags::MELEE == 0
        })
        .map(|item| {
            u32::from(item.spec().damage[0] + item.spec().damage[1])
                * u32::from(item.spec().burst.max(1))
                + u32::from(item.enchantment.max(0) as u16) * 2
        })
        .max()
        .unwrap_or(0);
    if next_floor >= 14 && best_burst_twice < 14
        || next_floor == 10 && best_burst_twice < 12 && heals < 14
        || next_floor == 10 && armor < 3 && heals < 12
        || next_floor == 15 && best_burst_twice < 16
        || next_floor >= 8 && heals == 0 && control_count(game) == 0
    {
        return false;
    }
    if has_basic_kit(game) {
        return true;
    }
    let food = game
        .player
        .inventory
        .iter()
        .filter(|item| item.gear.kind() == GearKind::Food)
        .map(|item| u32::from(item.count.max(1)))
        .sum::<u32>();
    game.floor >= 6
        && game.player.hp * 10 >= game.player.max_hp * 9
        && food >= 5
        && armor >= 4
        && game
            .player
            .inventory
            .iter()
            .any(|item| item.gear.kind() == GearKind::Weapon)
}

fn ready_to_approach_current_boss(game: &Game) -> bool {
    if game.floor == 15 {
        return true;
    }
    let has_ranged = game.player.inventory.iter().any(|item| {
        item.gear.kind() == GearKind::Weapon
            && item.spec().range > 0
            && item.spec().flags & gear_flags::MELEE == 0
    });
    let heals = healing_count(game);
    let matching = resource_matching_ammo_count(game);
    let control_ready = boss_control_count(game) > 0
        || game.player.has_skill(SkillId::Backup) && game.player.backup_cooldown <= 0;
    if game.floor == 5 {
        return if control_ready {
            game.player.hp * 10 >= game.player.max_hp * 9
                && heals >= 2
                && (!has_ranged || matching >= 12)
        } else {
            game.player.hp * 100 >= game.player.max_hp * 82
                && heals >= 1
                && (!has_ranged || matching >= 8)
        };
    }
    if game.floor != 10 {
        return true;
    }
    let armor = equipped_item(game, game.player.worn).map_or(0, |item| item.spec().armor);
    if control_ready {
        let ammo_floor = if game.player.class == crate::data::ClassId::Rookie {
            55
        } else {
            45
        };
        let healing_floor = if game.player.class == crate::data::ClassId::Rookie {
            12
        } else {
            10
        };
        game.player.hp * 10 >= game.player.max_hp * 9
            && (!has_ranged || matching >= ammo_floor)
            && (armor >= 3 || heals >= healing_floor)
            && (best_ranged_burst_twice(game) >= 12 || heals >= 14)
    } else {
        game.player.hp * 100 >= game.player.max_hp * 98
            && armor >= 4
            && (heals >= 5 && matching >= 55 || heals >= 8 && total_ammo_count(game) >= 90)
    }
}

fn best_ranged_burst_twice(game: &Game) -> u32 {
    game.player
        .inventory
        .iter()
        .filter(|item| {
            item.gear.kind() == GearKind::Weapon
                && item.spec().range > 0
                && item.spec().flags & gear_flags::MELEE == 0
        })
        .map(|item| {
            u32::from(item.spec().damage[0] + item.spec().damage[1])
                * u32::from(item.spec().burst.max(1))
                + u32::from(item.enchantment.max(0) as u8) * 2
        })
        .max()
        .unwrap_or(0)
}

fn total_ammo_count(game: &Game) -> u32 {
    game.player
        .inventory
        .iter()
        .filter(|item| item.gear.kind() == GearKind::Ammo)
        .map(|item| u32::from(item.count))
        .sum()
}

fn has_rng_shop_candidate(game: &Game, room: usize) -> bool {
    best_shop_item(game, room).is_some()
}

fn worthwhile_detour(game: &Game, item: &Item) -> bool {
    match item.gear.kind() {
        GearKind::Quest => true,
        GearKind::Ammo => {
            game.player.ammo_count(item.spec().ammo) < 60
                || Game::distance(game.player.cell as usize, item.cell as usize) <= 1
        }
        GearKind::Food => {
            item.spec().nutrition >= 100 || item.spec().heal > 0 || item.spec().haste > 0
        }
        GearKind::Pill => game.floor <= 4,
        GearKind::Thrown => {
            game.player
                .inventory
                .iter()
                .filter(|held| held.gear == GearId::FoamGrenade)
                .map(|held| held.count.max(1))
                .sum::<u16>()
                < 4
        }
        GearKind::Tool => {
            matches!(item.gear, GearId::PocketUniverse | GearId::NeuralyzerCharge)
                || item.gear == GearId::Scanner && game.floor <= 2
        }
        GearKind::Armor => equipped_item(game, game.player.worn)
            .is_none_or(|held| item.spec().armor > held.spec().armor),
        GearKind::Weapon => {
            contextual_weapon_score(game, item)
                > equipped_item(game, game.player.wielded)
                    .map_or(0, |held| contextual_weapon_score(game, held))
        }
    }
}

fn contextual_weapon_score(game: &Game, item: &Item) -> i32 {
    let spec = item.spec();
    let ranged = spec.range > 0 && spec.flags & gear_flags::MELEE == 0;
    let ammo_factor = if ranged {
        game.player.ammo_count(spec.ammo).min(6) as i32
    } else {
        6
    };
    let damage =
        i32::from(spec.damage[0] + spec.damage[1]) * i32::from(spec.burst.max(1)) * ammo_factor * 5;
    let range = if ranged {
        i32::from(spec.range) * 22 * ammo_factor
    } else {
        0
    };
    let melee = if spec.flags & gear_flags::MELEE != 0 {
        8 * 60
    } else {
        0
    };
    let final_boss = if game.floor >= 14 && ranged && ammo_factor > 0 {
        (18 + game.player.ammo_count(spec.ammo).min(12) as i32) * 60
    } else {
        0
    };
    let kick_penalty = if spec.flags & gear_flags::KICK != 0 {
        2 * 60
    } else {
        0
    };
    damage + range + melee + final_boss + i32::from(item.enchantment) * 120 - kick_penalty
}

fn food_route_worthwhile(game: &Game, route_steps: usize) -> bool {
    let reserve = if game.floor >= 12 {
        9
    } else if game.floor >= 8 {
        7
    } else if game.floor >= 5 {
        5
    } else {
        3
    };
    game.floor == 8
        || route_steps <= 2
        || healing_count(game) < reserve && route_steps <= 5
        || !has_basic_kit(game) && route_steps <= 15
        || !ready_for_next_floor(game) && route_steps <= 13
}

fn ammo_route_worthwhile(game: &Game, route_steps: usize) -> bool {
    let limit = if game.floor >= 12 {
        27
    } else if game.floor >= 8 {
        23
    } else {
        17
    };
    route_steps <= limit
}

fn should_restock_ammo(game: &Game) -> bool {
    let total_target = if game.floor >= 14 {
        90
    } else if game.floor >= 9 {
        70
    } else if game.floor >= 5 {
        42
    } else {
        24
    };
    let matching_target = if game.floor >= 14 {
        75
    } else if game.floor >= 9 {
        55
    } else if game.floor >= 5 {
        30
    } else {
        16
    };
    let total = game
        .player
        .inventory
        .iter()
        .filter(|item| item.gear.kind() == GearKind::Ammo)
        .map(|item| u32::from(item.count))
        .sum::<u32>();
    total < total_target || matching_ammo_count(game) < matching_target
}

fn ready_for_floor_10(game: &Game) -> bool {
    let armor = equipped_item(game, game.player.worn).map_or(0, |item| item.spec().armor);
    let ranged_burst_twice = game
        .player
        .inventory
        .iter()
        .filter(|item| item.gear.kind() == GearKind::Weapon && item.spec().range > 0)
        .map(|item| {
            u32::from(item.spec().damage[0] + item.spec().damage[1])
                * u32::from(item.spec().burst.max(1))
        })
        .max()
        .unwrap_or(0);
    game.player.hp * 100 >= game.player.max_hp * 72
        && matching_ammo_count(game) >= 18
        && healing_count(game) >= 4
        && boss_control_count(game) >= 1
        && (ranged_burst_twice >= 12 || healing_count(game) >= 14)
        && (armor >= 3 || healing_count(game) >= 12)
        && has_basic_kit(game)
}

fn ranged_ready(game: &Game) -> Option<usize> {
    let uid = game.player.wielded?;
    let weapon = game.player.inventory.iter().find(|item| item.uid == uid)?;
    let spec = weapon.spec();
    (spec.range > 0 && spec.flags & gear_flags::MELEE == 0 && game.player.ammo_count(spec.ammo) > 0)
        .then_some(usize::from(spec.range))
}

fn throw_range(game: &Game) -> usize {
    4 + game.stat(0) as usize / 4
}

fn typescript_choice_rng_draws(game: &Game, action: &Action) -> u8 {
    let visible = visible_hostiles(game);
    let mut draws = u8::from(!visible.is_empty());
    let has_healing = game
        .player
        .inventory
        .iter()
        .any(|item| item.gear.kind() == GearKind::Food && item.spec().heal > 0);
    draws += u8::from(has_healing);

    let adjacent = visible.iter().any(|&index| {
        Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 1
    });
    let heal_line = if game.floor >= 6 { 72 } else { 55 };
    let returns_with_early_heal = !adjacent
        && game.player.hp * 100 <= (game.player.max_hp * heal_line).max(800)
        && has_healing
        && matches!(action, Action::Eat(_));
    let returns_with_coffee = matches!(action, Action::Eat(uid) if equipped_item(game, Some(*uid)).is_some_and(|item| item.gear == GearId::Coffee));
    if returns_with_early_heal || returns_with_coffee {
        return draws;
    }

    let has_hunger_food = game
        .player
        .inventory
        .iter()
        .any(|item| item.gear.kind() == GearKind::Food && item.spec().nutrition > 0);
    draws += u8::from(has_hunger_food);
    if matches!(action, Action::Use(uid) if equipped_item(game, Some(*uid)).is_some_and(|item| item.gear == GearId::Scanner))
    {
        return draws;
    }

    let current_armor = equipped_item(game, game.player.worn).map_or(0, |item| item.spec().armor);
    let better_armor = game.player.inventory.iter().any(|item| {
        item.gear.kind() == GearKind::Armor
            && !item.cursed
            && Some(item.uid) != game.player.worn
            && item.spec().armor > current_armor
    });
    draws += u8::from(better_armor);
    if matches!(action, Action::Wear(_)) {
        return draws;
    }

    let wielded = equipped_item(game, game.player.wielded);
    if wielded.is_none_or(|item| !item.cursed) {
        let ranged_alternatives = game.player.inventory.iter().any(|item| {
            item.gear.kind() == GearKind::Weapon
                && item.spec().range > 0
                && item.spec().flags & gear_flags::MELEE == 0
                && !item.cursed
                && Some(item.uid) != game.player.wielded
                && game.player.ammo_count(item.spec().ammo) > 0
        });
        draws += u8::from(ranged_alternatives);
        let current = wielded.map_or(0, weapon_score);
        let upgrade = game.player.inventory.iter().any(|item| {
            item.gear.kind() == GearKind::Weapon
                && !item.cursed
                && Some(item.uid) != game.player.wielded
                && weapon_score(item) > current
        });
        let early_ranged_swap = game.floor >= 8
            && ranged_alternatives
            && wielded.is_none_or(|item| item.spec().range == 0);
        if !early_ranged_swap {
            draws += u8::from(upgrade);
        }
    }
    if matches!(action, Action::Wield(_)) {
        return draws;
    }

    let reaches_combat_tools = !visible.is_empty()
        && (game.floor == 15
            || visible.iter().any(|&index| game.mobs[index].frozen <= 0)
            || visible.iter().any(|&index| {
                Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 1
            }))
        && !(game.floor == 14
            && matches!(action, Action::Command(_))
            && !visible.iter().any(|&index| {
                Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 1
            }));
    if reaches_combat_tools {
        draws += u8::from(
            game.player
                .inventory
                .iter()
                .any(|item| item.gear == GearId::FoamGrenade),
        );
        draws += u8::from(
            game.player
                .inventory
                .iter()
                .any(|item| item.gear == GearId::PocketUniverse),
        );
    } else if matches!(game.floor, 1 | 2 | 6 | 7 | 8)
        && game
            .shop_room
            .is_some_and(|room| has_rng_shop_candidate(game, room))
    {
        draws += 1;
    }
    if game.floor == 9 && matches!(action, Action::Buy(_)) {
        draws += 1;
    }
    if game.floor == 9
        && matches!(action, Action::Fire(_))
        && game.shop_room.is_none()
        && !visible.is_empty()
        && visible.iter().all(|&index| game.mobs[index].frozen > 0)
        && visible.iter().all(|&index| {
            Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) > 1
        })
    {
        draws += 1;
    }
    if visible.is_empty() && matches!(action, Action::Fire(_)) {
        draws += 1;
    }
    if visible.is_empty()
        && matches!(action, Action::Use(uid) if equipped_item(game, Some(*uid)).is_some_and(|item| item.gear == GearId::PocketUniverse))
    {
        draws += 1;
    }
    if game.floor == 10
        && matches!(action, Action::Fire(_))
        && game.player.status[GRABBED] == 0
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            mob.boss
                && mob.frozen <= 0
                && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
        })
    {
        draws += 1;
    }
    if matches!(action, Action::Fire(_))
        && game.player.hp * 100 <= game.player.max_hp * 65
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            !mob.boss
                && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
                && equipped_item(game, game.player.wielded).is_some_and(|weapon| {
                    i32::from(mob.hp) * 40
                        <= i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                            * 23
                            * i32::from(weapon.spec().burst.max(1))
                })
        })
    {
        draws += 1;
    }
    if matches!(action, Action::Throw(_, _))
        && visible.iter().any(|&index| game.mobs[index].boss)
        && (game.player.hp * 2 <= game.player.max_hp
            || game.floor == 5 && game.player.hp * 5 <= game.player.max_hp * 3)
    {
        draws += 1;
    }
    if matches!(action, Action::Fire(_))
        && (game.player.hp * 2 <= game.player.max_hp
            || game.floor == 5 && game.player.hp * 5 <= game.player.max_hp * 3)
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            mob.boss
                && mob.frozen > 0
                && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
        })
    {
        draws += 1;
    }
    if visible.len() >= 2
        && matches!(action, Action::Fire(_) | Action::Command(_))
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            !mob.boss
                && mob.frozen > 0
                && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
        })
    {
        draws += 1;
    }
    if game.floor == 10
        && matches!(action, Action::Command(_))
        && visible.len() == 1
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            mob.frozen > 0 && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
        })
    {
        draws = draws.saturating_sub(2);
    }
    if game.floor == 10
        && matches!(action, Action::Fire(_))
        && active_boss(game).is_some()
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            !mob.boss
                && mob.frozen > 0
                && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
        })
    {
        draws = draws.saturating_sub(1);
    }
    if game.floor == 10
        && matches!(action, Action::Fire(cell) if game.mobs.iter().any(|mob| mob.hp > 0 && mob.boss && mob.cell as usize == *cell))
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            !mob.boss
                && mob.frozen <= 0
                && Game::distance(game.player.cell as usize, mob.cell as usize) == 2
        })
        && matches!(action, Action::Fire(cell) if game.line_clear(game.player.cell as usize, *cell, true))
    {
        draws = draws.saturating_sub(2);
    }
    if game.floor == 10
        && matches!(action, Action::Fire(_))
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            mob.boss
                && mob.frozen > 0
                && Game::distance(game.player.cell as usize, mob.cell as usize) > 1
                && matches!(action, Action::Fire(cell) if *cell == mob.cell as usize)
        })
    {
        draws += 2;
    }
    if game.floor == 12
        && game.player.has_skill(SkillId::Quickdraw)
        && matches!(action, Action::Command('l'))
        && visible.iter().any(|&index| {
            game.mobs[index].frozen > 0
                && Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 1
        })
    {
        draws += 1;
    }
    if game.floor == 14
        && game.player.has_skill(SkillId::Quickdraw)
        && game.player.max_hp >= 40
        && matches!(action, Action::Fire(_))
        && visible.len() >= 2
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            !mob.boss
                && mob.frozen > 0
                && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
        })
    {
        draws = draws.saturating_sub(1);
    }
    if game.floor == 14
        && game.player.has_skill(SkillId::Quickdraw)
        && game.player.max_hp >= 40
        && matches!(action, Action::Fire(_))
        && visible.len() == 1
        && visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            !mob.boss
                && mob.frozen > 0
                && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
                && equipped_item(game, game.player.wielded).is_some_and(|weapon| {
                    i32::from(mob.hp) * 40
                        > i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                            * 23
                            * i32::from(weapon.spec().burst.max(1))
                })
        })
    {
        draws += 1;
    }
    if game.floor == 14
        && game.player.has_skill(SkillId::Quickdraw)
        && game.player.max_hp >= 40
        && matches!(action, Action::Throw(_, _))
        && visible.len() == 1
    {
        draws += 1;
    }
    draws
}
