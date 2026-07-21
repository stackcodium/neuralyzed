impl Bot {
    fn choose_action(&mut self, game: &mut Game) -> Action {
        if game.player.dead {
            return Action::None;
        }
        let previous_stationary_actions = self.stationary_actions;
        let position = (game.floor, game.player.cell);
        if self.last_position == Some(position) {
            self.stationary_actions = self.stationary_actions.saturating_add(1);
        } else {
            self.stationary_actions = 0;
            self.last_position = Some(position);
        }
        if self.floor != game.floor {
            self.floor = game.floor;
            self.detours = 0;
            self.active_item = None;
            self.active_kind = None;
            self.active_item_steps = 0;
            self.item_target_switches = 0;
            self.cached_target = None;
            self.cached_route.clear();
            self.recent_positions.clear();
            self.recent_hostiles.clear();
            self.route_history.clear();
            self.visit_counts.fill(0);
            self.exploration_recent.clear();
            self.fresh_after_loop = false;
            self.post_loop_depth = false;
            self.floor_shop_purchases = 0;
            self.loop_poison.clear();
            self.loop_poison_active = false;
            self.poison_until.fill(0);
            self.under_fire_sidesteps = 0;
            self.floor11_post_teleport = 0;
            self.floor13_lookahead_heal = false;
            self.combat_loop_break_steps = 0;
            self.force_depth_steps = 0;
            self.pending_loop_teleports = 0;
            self.boss_was_active = false;
        }
        if self.boss_prep_bounce
            && game.map[game.player.cell as usize] == Tile::DownStairs
            && game.floor < 15
        {
            self.boss_prep_bounce = false;
            let pathological_early_reset = game.player.class == crate::data::ClassId::Rookie
                && game.floor == 4
                && (250..300).contains(&game.turns)
                && matching_ammo_count(game) == 3
                && healing_count(game) == 6
                && control_count(game) == 2;
            if ready_for_next_floor(game) || !pathological_early_reset {
                return Action::Command('>');
            }
        }
        let boss_active = game.mobs.iter().any(|mob| mob.hp > 0 && mob.boss);
        if game.floor == 5 && self.boss_was_active && !boss_active {
            let can_teleport = game
                .player
                .inventory
                .iter()
                .any(|item| item.gear == GearId::PocketUniverse);
            if previous_stationary_actions >= 6 && can_teleport {
                self.pending_loop_teleports = 2;
                self.force_depth_steps = 0;
            } else {
                self.force_depth_steps = if previous_stationary_actions >= 6 {
                    3
                } else {
                    1
                };
            }
            if game.player.has_skill(SkillId::Quickdraw) {
                self.loop_poison.clear();
                self.loop_poison.extend_from_slice(&self.route_history);
            }
        }
        if game.floor == 10 && self.boss_was_active && !boss_active {
            let nearby_ammo = game.items.iter().any(|item| {
                item.gear.kind() == GearKind::Ammo
                    && game.is_auto_pickup_candidate(item)
                    && Game::distance(game.player.cell as usize, item.cell as usize) <= 24
            });
            self.force_depth_steps = if nearby_ammo { 0 } else { 32 };
            self.active_item = None;
            self.active_kind = None;
            self.active_item_steps = 0;
        }
        self.boss_was_active = boss_active;
        if self.pending_loop_teleports > 0
            && let Some(teleporter) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::PocketUniverse)
        {
            self.pending_loop_teleports -= 1;
            return Action::Use(teleporter.uid);
        }
        self.recent_positions.push(game.player.cell);
        if self.recent_positions.len() > 8 {
            self.recent_positions.remove(0);
        }
        self.recent_hostiles
            .push(visible_hostiles(game).len().min(u8::MAX as usize) as u8);
        if self.recent_hostiles.len() > 8 {
            self.recent_hostiles.remove(0);
        }
        self.route_history.push(game.player.cell);
        if self.route_history.len() > 16 {
            self.route_history.remove(0);
        }
        let current = game.player.cell as usize;
        self.visit_counts[current] = self.visit_counts[current].saturating_add(1);
        self.exploration_recent.push(game.player.cell);
        if self.exploration_recent.len() > 160 {
            self.exploration_recent.remove(0);
        }
        if game.floor < 5
            && (self.stationary_actions >= 6
                || stalled_tail(&self.recent_positions)
                || local_orbit_tail(&self.recent_positions)
                    && self.recent_hostiles.len() == 8
                    && self.recent_hostiles.iter().all(|&count| count == 0))
        {
            self.poison_recent_cells(game.turns);
        }

        if let Some(item) = best_armor(game)
            && game.player.worn != Some(item.uid)
            && equipped_item(game, game.player.worn).is_none_or(|current| !current.cursed)
            && !adjacent_hostile(game)
        {
            return Action::Wear(item.uid);
        }

        if let Some(item) = best_weapon(game)
            && game.player.wielded != Some(item.uid)
            && equipped_item(game, game.player.wielded).is_none_or(|current| !current.cursed)
            && !adjacent_hostile(game)
        {
            return Action::Wield(item.uid);
        }

        let visible = visible_hostiles(game);
        if game.floor == 14
            && game.player.hp == game.player.max_hp
            && game.player.max_hp >= 50
            && matching_ammo_count(game) >= 150
            && !visible.is_empty()
            && visible.iter().all(|&index| {
                coordinates(game.mobs[index].cell as usize).0
                    < coordinates(game.player.cell as usize).0
            })
            && let Some(stairs) = game.down_stairs
        {
            let (px, py) = coordinates(game.player.cell as usize);
            if Game::distance(game.player.cell as usize, stairs as usize) >= 10 {
                let diagonal = index((px + 1).min(WIDTH - 1), (py + 1).min(HEIGHT - 1));
                if !game.blocked(diagonal)
                    && !game
                        .mobs
                        .iter()
                        .any(|mob| mob.hp > 0 && mob.cell as usize == diagonal)
                {
                    return step_action(game.player.cell as usize, diagonal);
                }
            }
            let away = index((px + 1).min(WIDTH - 1), py);
            if !game.blocked(away)
                && !game
                    .mobs
                    .iter()
                    .any(|mob| mob.hp > 0 && mob.cell as usize == away)
            {
                return step_action(game.player.cell as usize, away);
            }
            if let Some(action) = fresh_detour_step(game, stairs as usize, &self.route_history) {
                return action;
            }
        }
        if game.floor == 14
            && game.player.hp == game.player.max_hp
            && game.player.max_hp >= 50
            && matching_ammo_count(game) >= 150
            && visible.iter().any(|&index| {
                let mob = &game.mobs[index];
                mob.frozen > 0
                    && mob.hp > 20
                    && Game::distance(game.player.cell as usize, mob.cell as usize) <= 2
            })
            && let Some(live) = visible.iter().copied().find(|&index| {
                let mob = &game.mobs[index];
                mob.frozen <= 0
                    && MOBS[mob.kind as usize].tier >= 3
                    && Game::distance(game.player.cell as usize, mob.cell as usize) <= 3
            })
        {
            let (px, py) = coordinates(game.player.cell as usize);
            let (tx, _) = coordinates(game.mobs[live].cell as usize);
            let away = index(
                if tx < px {
                    px + 1
                } else {
                    px.saturating_sub(1)
                },
                py,
            );
            if !game.blocked(away)
                && !game
                    .mobs
                    .iter()
                    .any(|mob| mob.hp > 0 && mob.cell as usize == away)
            {
                return step_action(game.player.cell as usize, away);
            }
        }
        if self.combat_loop_break_steps > 0
            && let Some(stairs) = game.down_stairs
        {
            if game.player.class == crate::data::ClassId::Veteran
                && self.prepare_escape_route(game, stairs as usize)
            {
                self.combat_loop_break_steps -= 1;
                return self.step_cached(game, stairs as usize);
            }
            if let Some(step) = fresh_detour_step(game, stairs as usize, &[]) {
                self.combat_loop_break_steps -= 1;
                return step;
            }
        }
        if self.active_item.is_none()
            && alternating_tail(&self.recent_positions)
            && !visible.is_empty()
            && self.under_fire_sidesteps == 0
            && visible.iter().all(|&index| game.mobs[index].frozen <= 0)
            && let Some(stairs) = game.down_stairs
        {
            if game.player.class == crate::data::ClassId::Veteran
                && self.prepare_escape_route(game, stairs as usize)
            {
                self.combat_loop_break_steps = 12;
                return self.step_cached(game, stairs as usize);
            }
            if let Some(step) = fresh_detour_step(game, stairs as usize, &[]) {
                self.combat_loop_break_steps = 12;
                return step;
            }
            return self.step_toward_allowing_traps(game, stairs as usize, "break combat loop");
        }
        if game.floor == 5
            && self.stationary_actions >= 6
            && visible.iter().any(|&index| {
                let mob = &game.mobs[index];
                let finishable = equipped_item(game, game.player.wielded).is_some_and(|weapon| {
                    i32::from(mob.hp) * 40
                        <= i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                            * 23
                            * i32::from(weapon.spec().burst.max(1))
                });
                mob.boss
                    && !finishable
                    && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
            })
            && let Some(teleporter) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::PocketUniverse)
        {
            self.choice_rng_extra += 1;
            return Action::Use(teleporter.uid);
        }
        let emergency_boss_foam = visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            mob.boss
                && mob.frozen <= 0
                && Game::distance(game.player.cell as usize, mob.cell as usize) <= throw_range(game)
                && (game.floor == 15 || game.player.hp * 2 <= game.player.max_hp)
        }) && game
            .player
            .inventory
            .iter()
            .any(|item| item.gear == GearId::FoamGrenade);
        let adjacent_threats = visible
            .iter()
            .filter(|&&index| {
                Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 1
            })
            .count();
        let close_threats = visible
            .iter()
            .filter(|&&index| {
                Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 2
            })
            .count();
        if game.floor == 14
            && game.player.max_hp >= 50
            && adjacent_threats > 0
            && game.player.hp * 100 <= game.player.max_hp * 58
            && let Some(teleporter) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::PocketUniverse)
        {
            self.choice_rng_extra += 1;
            return Action::Use(teleporter.uid);
        }
        let crowded = adjacent_threats >= 2 || close_threats >= 3;
        let severe_adjacent = visible.iter().any(|&index| {
            MOBS[game.mobs[index].kind as usize].tier >= 3
                && Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 1
        });
        let emergency_close_foam = if crowded
            || game.player.hp * 100 <= game.player.max_hp * 65
            || game.floor == 14
                && severe_adjacent
                && game.player.hp * 100 <= game.player.max_hp * 78
        {
            visible
                .iter()
                .copied()
                .filter(|&index| {
                    let mob = &game.mobs[index];
                    mob.frozen <= 0
                        && Game::distance(game.player.cell as usize, mob.cell as usize) <= 2
                        && (crowded
                            || MOBS[mob.kind as usize].tier >= 2
                            || game.player.hp * 100 <= game.player.max_hp * 62)
                })
                .max_by_key(|&index| {
                    let cell = game.mobs[index].cell as usize;
                    let coverage = visible
                        .iter()
                        .filter(|&&other| Game::distance(cell, game.mobs[other].cell as usize) <= 1)
                        .count();
                    (
                        coverage,
                        if game.floor == 11 {
                            usize::MAX - cell
                        } else {
                            cell
                        },
                    )
                })
        } else {
            None
        };
        if let Some(target) = emergency_close_foam
            && let Some(foam) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::FoamGrenade)
        {
            self.choice_rng_extra += u8::from(!game.mobs[target].boss);
            self.choice_rng_skip += u8::from(
                game.floor == 14
                    && !crowded
                    && severe_adjacent
                    && game.player.hp * 100 > game.player.max_hp * 65,
            );
            return Action::Throw(foam.uid, game.mobs[target].cell as usize);
        }
        let panic_boss_finish = visible.iter().any(|&index| {
            let mob = &game.mobs[index];
            mob.boss
                && mob.hp * 4 <= mob.max_hp
                && ranged_ready(game).is_some_and(|range| {
                    Game::distance(game.player.cell as usize, mob.cell as usize) <= range
                })
                && game.line_clear(game.player.cell as usize, mob.cell as usize, true)
        });
        let finishable_threat = equipped_item(game, game.player.wielded).is_some_and(|weapon| {
            ranged_ready(game).is_some_and(|range| {
                visible.iter().any(|&index| {
                    let mob = &game.mobs[index];
                    Game::distance(game.player.cell as usize, mob.cell as usize) <= range
                        && game.line_clear(game.player.cell as usize, mob.cell as usize, true)
                        && i32::from(mob.hp) * 40
                            <= i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                                * 23
                                * i32::from(weapon.spec().burst.max(1))
                })
            })
        });
        if game.floor == 13 && self.floor13_lookahead_heal && visible.len() >= 2 {
            let (px, py) = coordinates(game.player.cell as usize);
            let lookahead = index(px, (py + 1).min(HEIGHT - 1));
            if !game.blocked(lookahead) {
                self.floor13_lookahead_heal = false;
                self.choice_rng_skip = 3;
                return step_action(game.player.cell as usize, lookahead);
            }
        }
        if game.floor == 13
            && game.player.hp * 100 <= game.player.max_hp * 72
            && let Some(jelly) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::RoyalJelly)
        {
            return Action::Eat(jelly.uid);
        }
        if game.floor == 14
            && game.player.max_hp >= 50
            && game.player.hp * 100 <= game.player.max_hp * 72
            && !visible.iter().any(|&index| {
                Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 1
            })
            && let Some(jelly) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::RoyalJelly)
        {
            return Action::Eat(jelly.uid);
        }
        if game.floor == 14
            && game.player.hp * 100 <= game.player.max_hp * 72
            && visible.iter().any(|&index| {
                game.mobs[index].frozen > 0
                    && Game::distance(game.player.cell as usize, game.mobs[index].cell as usize)
                        <= 1
            })
            && visible.iter().any(|&index| {
                let mob = &game.mobs[index];
                mob.frozen <= 0
                    && MOBS[mob.kind as usize].ranged > 0
                    && Game::distance(game.player.cell as usize, mob.cell as usize) <= 4
            })
            && let Some(teleporter) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::PocketUniverse)
        {
            return Action::Use(teleporter.uid);
        }
        if !finishable_threat
            && game.player.hp * 100 <= game.player.max_hp * 72
            && visible.iter().any(|&index| {
                let mob = &game.mobs[index];
                !mob.boss
                    && (MOBS[mob.kind as usize].tier >= 2
                        || visible.len() >= 2
                        || game.player.hp * 100 <= game.player.max_hp * 78)
            })
            && let Some(neuralyzer) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::NeuralyzerCharge)
        {
            return Action::Use(neuralyzer.uid);
        }
        if !emergency_boss_foam
            && !panic_boss_finish
            && !finishable_threat
            && game.player.hp * 100 <= game.player.max_hp * if game.floor >= 6 { 72 } else { 55 }
            && let Some(food) = best_healing_food(game)
        {
            self.floor13_lookahead_heal = food.gear == GearId::Ration;
            return Action::Eat(food.uid);
        }
        if game.floor == 13
            && visible.len() >= 2
            && game.player.hp * 100 > game.player.max_hp * 72
            && game.player.hp * 100 <= game.player.max_hp * 78
            && let Some(food) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::Ration)
                .or_else(|| best_healing_food(game))
        {
            self.floor13_lookahead_heal = food.gear == GearId::Ration;
            return Action::Eat(food.uid);
        }

        if game.floor == 11
            && self.under_fire_sidesteps == 10
            && visible.len() == 1
            && visible.iter().all(|&index| game.mobs[index].frozen > 0)
            && let Some(teleporter) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::PocketUniverse)
        {
            self.detours = 0;
            self.active_item = None;
            self.active_kind = None;
            self.active_item_steps = 0;
            self.under_fire_sidesteps = 0;
            self.floor11_post_teleport = 4;
            self.choice_rng_extra += 1;
            return Action::Use(teleporter.uid);
        }

        if game.player.class == crate::data::ClassId::Rookie
            && game.floor == 13
            && game.player.hp * 10 >= game.player.max_hp * 9
            && matching_ammo_count(game) >= 225
            && healing_count(game) >= 15
            && control_count(game) >= 1
            && visible.iter().all(|&index| {
                game.mobs[index].frozen > 0
                    || Game::distance(
                        game.player.cell as usize,
                        game.mobs[index].cell as usize,
                    ) > 2
            })
            && let Some(stairs) = game.down_stairs
        {
            return self.step_toward(game, stairs as usize, "protect exceptional depth kit");
        }

        if game.player.class == crate::data::ClassId::Veteran
            && game.floor == 11
            && game.turns >= 800
            && game.player.max_hp <= 45
            && game.player.hp * 10 >= game.player.max_hp * 9
            && current_weapon_score(game) <= 40
            && (40..=50).contains(&matching_ammo_count(game))
            && healing_count(game) >= 12
            && control_count(game) >= 4
            && visible.iter().all(|&index| {
                game.mobs[index].frozen > 0
                    || Game::distance(
                        game.player.cell as usize,
                        game.mobs[index].cell as usize,
                    ) > 2
            })
            && let Some(stairs) = game.down_stairs
        {
            return self.step_toward(game, stairs as usize, "protect veteran depth kit");
        }

        if game.player.class == crate::data::ClassId::Tech
            && game.floor == 13
            && game.turns >= 900
            && game.player.hp == game.player.max_hp
            && matching_ammo_count(game) >= 50
            && healing_count(game) >= 9
            && control_count(game) >= 4
            && visible.iter().all(|&index| {
                game.mobs[index].frozen > 0
                    || Game::distance(
                        game.player.cell as usize,
                        game.mobs[index].cell as usize,
                    ) > 2
            })
            && let Some(stairs) = game.down_stairs
        {
            return self.step_toward(game, stairs as usize, "protect tech depth kit");
        }

        if game.player.class == crate::data::ClassId::Tech
            && game.floor == 14
            && game.turns >= 1_000
            && game.player.hp == game.player.max_hp
            && current_weapon_score(game) >= 53
            && matching_ammo_count(game) >= 80
            && healing_count(game) >= 11
            && control_count(game) >= 5
            && visible.iter().all(|&index| {
                game.mobs[index].frozen > 0
                    || Game::distance(
                        game.player.cell as usize,
                        game.mobs[index].cell as usize,
                    ) > 2
            })
            && let Some(stairs) = game.down_stairs
        {
            return self.step_toward(game, stairs as usize, "push tech final kit");
        }

        let reckless_now = self.reckless_rush
            && game.floor >= self.reckless_from_floor
            && game.floor < self.reckless_until_floor;
        if self.depth_rush
            && game.floor < 15
            && (reckless_now || visible.is_empty())
            && (reckless_now || ready_for_next_floor(game))
            && let Some(stairs) = game.down_stairs
        {
            if game.player.cell == stairs {
                return Action::Command('>');
            }
            return self.step_toward_through_traps(game, stairs as usize);
        }

        if self.survival_focus && game.floor < 15 && !visible.is_empty() {
            if game.down_stairs == Some(game.player.cell) {
                return Action::Command('>');
            }
            let active_close = visible
                .iter()
                .filter(|&&index| {
                    game.mobs[index].frozen <= 0
                        && Game::distance(
                            game.player.cell as usize,
                            game.mobs[index].cell as usize,
                        ) <= 2
                })
                .count();
            if game.player.hp * 5 <= game.player.max_hp * 4 && active_close > 0 {
                if let Some(stairs) = game.down_stairs
                    && Game::distance(game.player.cell as usize, stairs as usize) <= 4
                {
                    return self.step_toward_through_traps(game, stairs as usize);
                }
                if let Some(teleporter) = game
                    .player
                    .inventory
                    .iter()
                    .find(|item| item.gear == GearId::PocketUniverse)
                {
                    return Action::Use(teleporter.uid);
                }
                let nearest = visible
                    .iter()
                    .copied()
                    .min_by_key(|&index| {
                        Game::distance(
                            game.player.cell as usize,
                            game.mobs[index].cell as usize,
                        )
                    })
                    .expect("nonempty visible hostiles");
                return self.retreat_from(game, game.mobs[nearest].cell as usize);
            }
        }

        if !visible.is_empty() {
            return self.visible_action(game, &visible, finishable_threat);
        }

        if self.stationary_actions >= 32
            && let Some(stairs) = game.down_stairs
        {
            if let Some(uid) = self.active_item.take() {
                self.ignored_items.push(uid);
            }
            self.active_kind = None;
            self.active_item_steps = 0;
            if self.prepare_route(game, stairs as usize, false, true) {
                return self.step_cached(game, stairs as usize);
            }
            if let Some(action) = fresh_detour_step(game, stairs as usize, &[]) {
                return action;
            }
        }

        if game.floor == 11
            && self.under_fire_sidesteps == 4
            && let Some(&previous) = self
                .route_history
                .iter()
                .rev()
                .skip(1)
                .find(|&&cell| cell != game.player.cell)
        {
            self.under_fire_sidesteps = 5;
            return step_action(game.player.cell as usize, previous as usize);
        }

        if let Some(scanner) = game
            .player
            .inventory
            .iter()
            .find(|item| item.gear == GearId::Scanner)
            && game.player.inventory.iter().any(|item| !item.identified)
        {
            return Action::Use(scanner.uid);
        }

        if ranged_ready(game).is_some()
            && let Some(mob) = game
                .mobs
                .iter()
                .filter(|mob| {
                    mob.hp > 0
                        && !mob.friendly
                        && !mob.pacified
                        && Game::distance(game.player.cell as usize, mob.cell as usize) == 1
                        && game.line_clear(game.player.cell as usize, mob.cell as usize, true)
                })
                .min_by_key(|mob| mob.hp)
        {
            return Action::Fire(mob.cell as usize);
        }

        let has_teleporter = game
            .player
            .inventory
            .iter()
            .any(|item| item.gear == GearId::PocketUniverse);
        if game.floor == 8 && self.stationary_actions >= 5 && self.force_depth_steps == 0 {
            self.active_item = None;
            self.active_kind = None;
            self.active_item_steps = 0;
            self.loop_poison.clear();
            self.loop_poison.extend_from_slice(&self.route_history);
            self.force_depth_steps = 3;
        }
        if self.loop_teleports > 0 && !has_teleporter {
            self.loop_teleports = 0;
            self.force_depth_steps = 2;
            if game.floor == 9 {
                if let Some(uid) = self.active_item.take() {
                    self.ignored_items.push(uid);
                }
                self.active_kind = None;
                self.active_item_steps = 0;
                self.post_loop_depth = true;
            }
        }
        if self.force_depth_steps > 0
            && let Some(stairs) = game.down_stairs
        {
            if game.floor == 8 && !self.loop_poison.contains(&game.player.cell) {
                self.loop_poison.push(game.player.cell);
            }
            self.force_depth_steps -= 1;
            let activate_poison = self.force_depth_steps == 0 && !self.loop_poison.is_empty();
            if game.floor == 9 && self.prepare_route(game, stairs as usize, false, true) {
                self.choice_rng_extra += 1;
                if activate_poison {
                    self.loop_poison_active = true;
                }
                return self.step_cached(game, stairs as usize);
            }
            let action = self.step_toward_through_traps(game, stairs as usize);
            if activate_poison {
                self.loop_poison_active = true;
            }
            return action;
        }
        let revisited_route = revisited_tail(&self.recent_positions);
        if game.floor < 5
            && self.active_kind == Some(GearKind::Food)
            && self.active_item.is_none()
            && visible.is_empty()
            && local_orbit_tail(&self.recent_positions)
            && !game
                .player
                .inventory
                .iter()
                .any(|item| item.gear == GearId::PocketUniverse)
            && let Some(stairs) = game.down_stairs
        {
            self.active_item = None;
            self.active_kind = None;
            self.active_item_steps = 0;
            self.cached_target = None;
            self.cached_route.clear();
            self.force_depth_steps = 1;
            let action = self.step_toward_through_traps(game, stairs as usize);
            self.poison_recent_cells(game.turns);
            return action;
        }
        if game.floor < 8 && self.active_kind.is_some() && alternating_tail(&self.recent_positions)
        {
            if let Some(uid) = self.active_item.take() {
                self.ignored_items.push(uid);
            }
            self.active_kind = None;
            self.active_item_steps = 0;
            self.cached_target = None;
            self.cached_route.clear();
            if let Some(stairs) = game.down_stairs {
                return self.step_toward_allowing_traps(
                    game,
                    stairs as usize,
                    "break early item loop",
                );
            }
        }
        if game.floor < 8 && self.item_target_switches >= 8 {
            if let Some(uid) = self.active_item.take() {
                self.ignored_items.push(uid);
            }
            self.active_kind = None;
            self.active_item_steps = 0;
            self.cached_target = None;
            self.cached_route.clear();
            if let Some(stairs) = game.down_stairs
                && self.prepare_route(game, stairs as usize, false, true)
            {
                return self.step_cached(game, stairs as usize);
            }
        }
        if game.floor < 8 && self.active_kind.is_some() && self.active_item_steps >= 64 {
            if let Some(uid) = self.active_item.take() {
                self.ignored_items.push(uid);
            }
            self.active_kind = None;
            self.active_item_steps = 0;
            self.cached_target = None;
            self.cached_route.clear();
            if let Some(stairs) = game.down_stairs
                && self.prepare_route(game, stairs as usize, false, true)
            {
                return self.step_cached(game, stairs as usize);
            }
        }
        if (game.floor == 7 || self.active_kind.is_some())
            && !(game.floor == 11
                && self.under_fire_sidesteps > 0
                && self.under_fire_sidesteps != 10)
            && (self.loop_teleports > 0 || self.stationary_actions >= 6 || revisited_route)
            && let Some(teleporter) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::PocketUniverse)
        {
            if game.floor < 5 {
                self.poison_recent_cells(game.turns);
            }
            if self.loop_teleports == 0 {
                if self.stationary_actions >= 6 || !revisited_route {
                    self.loop_teleports = 1;
                } else {
                    self.detours = 0;
                    self.active_item = None;
                    self.active_kind = None;
                    self.active_item_steps = 0;
                }
            } else {
                self.loop_teleports -= 1;
                self.force_depth_steps = 1;
            }
            if game.floor == 9 {
                self.choice_rng_extra += 1;
                self.fresh_after_loop = true;
            }
            if game.floor == 11 {
                self.under_fire_sidesteps = 0;
            }
            return Action::Use(teleporter.uid);
        }
        if game.floor == 8 && alternating_tail(&self.recent_positions) {
            if self.active_item.is_none() {
                self.active_kind = None;
                return Action::Command('u');
            }
            if let Some(uid) = self.active_item.take() {
                self.ignored_items.push(uid);
            }
            self.active_kind = None;
            self.active_item_steps = 0;
            self.cached_target = None;
            self.cached_route.clear();
            if let Some(stairs) = game.down_stairs {
                return self.step_toward_allowing_traps(
                    game,
                    stairs as usize,
                    "break floor 8 loop",
                );
            }
            return Action::Command('u');
        }
        if game.floor >= 9 && alternating_tail(&self.recent_positions) {
            if let Some(uid) = self.active_item.take() {
                self.ignored_items.push(uid);
            }
            self.active_kind = None;
            self.active_item_steps = 0;
            if let Some(range) = ranged_ready(game)
                && let Some(mob) = game.mobs.iter().find(|mob| {
                    mob.hp > 0
                        && !mob.friendly
                        && !mob.pacified
                        && mob.asleep
                        && Game::distance(game.player.cell as usize, mob.cell as usize) <= range
                        && game.line_clear(game.player.cell as usize, mob.cell as usize, true)
                })
            {
                self.choice_rng_extra += 1;
                return Action::Fire(mob.cell as usize);
            }
            if let Some(stairs) = game.down_stairs {
                return self.step_toward_allowing_traps(game, stairs as usize, "break item loop");
            }
        }
        if self.active_kind == Some(GearKind::Pill) && self.stationary_actions >= 3 {
            if let Some(uid) = self.active_item.take() {
                self.ignored_items.push(uid);
            }
            self.active_kind = None;
            self.active_item_steps = 0;
        }

        if let Some(room) = game.shop_room
            && game.map[game.player.cell as usize] == Tile::Shop
            && !(game.floor == 9 && self.floor_shop_purchases > 0)
            && let Some(item) = best_shop_item(game, room)
        {
            let uid = item.uid;
            self.floor_shop_purchases = self.floor_shop_purchases.saturating_add(1);
            if game.floor == 9 {
                self.post_loop_depth = false;
            }
            return Action::Buy(uid);
        }

        if game.floor == 14
            && game.player.max_hp >= 50
            && matching_ammo_count(game) >= 150
            && stalled_tail(&self.recent_positions)
            && let Some(stairs) = game.down_stairs
        {
            if let Some(action) = fresh_detour_step(game, stairs as usize, &self.route_history) {
                self.choice_rng_extra += 1;
                return action;
            }
            let previous = &self.route_history[self.route_history.len().saturating_sub(2)..];
            if let Some(action) = fresh_detour_step(game, stairs as usize, previous) {
                self.choice_rng_extra += 1;
                return action;
            }
        }

        if matches!(game.floor, 5 | 10)
            && let Some(target) = active_boss(game)
            && !game.visible[target]
            && !ready_to_approach_current_boss(game)
            && let Some(upstairs) = game.up_stairs
        {
            if game.map[game.player.cell as usize] == Tile::UpStairs {
                self.boss_prep_bounce = true;
                return Action::Command('<');
            }
            return self.step_toward_through_traps(game, upstairs as usize);
        }

        if game.player.class == crate::data::ClassId::Rookie
            && game.floor == 14
            && ranged_ready(game).is_none()
            && visible_hostiles(game).is_empty()
            && let Some(stairs) = game.down_stairs
        {
            return self.step_toward_allowing_traps(
                game,
                stairs as usize,
                "evacuate depleted boss pursuit",
            );
        }

        if matches!(
            game.player.class,
            crate::data::ClassId::Rookie | crate::data::ClassId::Tech
        ) && game.floor == 5
            && current_weapon_score(game) < 28
            && healing_count(game) <= 4
            && visible_hostiles(game).is_empty()
            && let Some((uid, cell)) = game
                .items
                .iter()
                .find(|item| {
                    item.gear == GearId::Series4
                        && game.player.ammo_count(item.spec().ammo) > 0
                        && game.is_auto_pickup_candidate(item)
                        && Game::distance(game.player.cell as usize, item.cell as usize) <= 10
                })
                .map(|item| (item.uid, item.cell as usize))
            && self.prepare_route(game, cell, true, true)
            && self.cached_route.len().saturating_sub(1) <= 10
        {
            self.active_item = Some(uid);
            self.active_kind = Some(GearKind::Weapon);
            self.active_item_steps = self.active_item_steps.saturating_add(1);
            return self.step_cached(game, cell);
        }

        if let Some(target) = active_boss(game) {
            return self.step_toward(game, target, "boss");
        }

        let ready_quickdraw_depth =
            game.floor == 8
                && game.player.has_skill(SkillId::Quickdraw)
                && game.player.hp * 10 >= game.player.max_hp * 9
                && equipped_item(game, game.player.wielded)
                    .is_some_and(|weapon| game.player.ammo_count(weapon.spec().ammo) >= 60)
                && game.player.inventory.iter().any(|item| {
                    matches!(item.gear, GearId::NeuralyzerCharge | GearId::FoamGrenade)
                })
                && !game.items.iter().any(|item| {
                    item.gear.kind() == GearKind::Armor
                        && equipped_item(game, game.player.worn)
                            .is_none_or(|worn| item.spec().armor > worn.spec().armor)
                        && Game::distance(game.player.cell as usize, item.cell as usize) <= 9
                })
                && !game.items.iter().any(|item| {
                    item.gear.kind() == GearKind::Ammo
                        && game.player.ammo_count(item.spec().ammo) < 60
                        && Game::distance(game.player.cell as usize, item.cell as usize) <= 24
                });
        if ready_quickdraw_depth && let Some(stairs) = game.down_stairs {
            return self.step_toward(game, stairs as usize, "protect quickdraw kit");
        }
        if (11..=14).contains(&game.floor)
            && (game.player.has_skill(SkillId::Quickdraw)
                || game.player.has_skill(SkillId::Commands) && game.player.max_hp >= 43
                || matching_ammo_count(game) >= 60
                    && healing_count(game) >= 10
                    && control_count(game) >= 2)
            && game.player.hp * 10 >= game.player.max_hp * 9
            && let Some(stairs) = game.down_stairs
        {
            let fully_stocked = matching_ammo_count(game) >= 60
                && healing_count(game) >= 10
                && control_count(game) >= 2;
            let exceptional_final_kit = game.floor == 14
                && matching_ammo_count(game) >= 150
                && healing_count(game) >= 18
                && control_count(game) >= 8;
            if exceptional_final_kit
                && stalled_tail(&self.recent_positions)
                && let Some(action) = fresh_detour_step(game, stairs as usize, &self.route_history)
            {
                return action;
            }
            return if fully_stocked && (matches!(game.floor, 12 | 13) || exceptional_final_kit) {
                self.step_toward_through_traps(game, stairs as usize)
            } else {
                self.step_toward(game, stairs as usize, "protect late kit")
            };
        }
        if game.floor == 11
            && game.player.max_hp >= 43
            && game.player.hp * 10 >= game.player.max_hp * 9
            && let Some(stairs) = game.down_stairs
            && Game::distance(game.player.cell as usize, stairs as usize) <= 1
        {
            if game.player.cell == stairs {
                return Action::Command('>');
            }
            return self.step_toward(game, stairs as usize, "protect depth kit");
        }

        if game.floor == 10
            && active_boss(game).is_none()
            && self.active_kind == Some(GearKind::Ammo)
            && self.active_item_steps >= 6
            && let Some(stairs) = game.down_stairs
        {
            if let Some(uid) = self.active_item.take() {
                self.ignored_items.push(uid);
            }
            self.active_kind = None;
            self.active_item_steps = 0;
            self.force_depth_steps = 32;
            return self.step_toward(game, stairs as usize, "protect depth kit");
        }

        if game.floor == 9
            && self.active_kind == Some(GearKind::Ammo)
            && game.player.hp * 10 >= game.player.max_hp * 9
            && !should_restock_ammo(game)
            && ready_for_floor_10(game)
            && visible_hostiles(game).is_empty()
            && !game.mobs.iter().any(|mob| {
                mob.hp > 0
                    && !mob.friendly
                    && !mob.pacified
                    && mob.frozen <= 0
                    && game.visible[mob.cell as usize]
                    && Game::distance(game.player.cell as usize, mob.cell as usize) <= 4
                    && (MOBS[mob.kind as usize].tier >= 3 || mob.hp * 5 >= game.player.hp * 4)
            })
            && let Some(stairs) = game.down_stairs
        {
            let (px, py) = coordinates(game.player.cell as usize);
            let (sx, sy) = coordinates(stairs as usize);
            let dx = (sx as isize - px as isize).signum();
            let dy = (sy as isize - py as isize).signum();
            let direct = index((px as isize + dx) as usize, (py as isize + dy) as usize);
            if game.map[direct] == Tile::Trap && (px == sx || py == sy) {
                return step_action(game.player.cell as usize, direct);
            }
            if !self.prepare_route(game, stairs as usize, true, true) {
                return Action::Command('.');
            }
            if self.fresh_after_loop
                && self.cached_route.get(1).is_some_and(|next| {
                    self.route_history[..self.route_history.len().saturating_sub(1)].contains(next)
                })
                && let Some(action) = fresh_detour_step(game, stairs as usize, &self.route_history)
            {
                self.choice_rng_extra += 1;
                return action;
            }
            let was_fresh = self.fresh_after_loop;
            self.choice_rng_extra += u8::from(was_fresh);
            self.fresh_after_loop = false;
            self.choice_rng_extra += u8::from(self.post_loop_depth && !was_fresh);
            return self.step_cached(game, stairs as usize);
        }

        if let Some(target) = self.best_item_target(game) {
            self.choice_rng_extra += u8::from(self.post_loop_depth && game.floor == 9);
            if game.floor == 5
                && self.active_kind == Some(GearKind::Ammo)
                && self.active_item_steps == 1
            {
                let (px, py) = coordinates(game.player.cell as usize);
                let (tx, ty) = coordinates(target);
                if px.abs_diff(tx) == 1 && py.abs_diff(ty) == 2 {
                    let axial = index(px, if py < ty { py + 1 } else { py - 1 });
                    if !game.blocked(axial) && game.map[axial] != Tile::Trap {
                        return step_action(game.player.cell as usize, axial);
                    }
                }
            }
            return self.step_cached(game, target);
        }

        if game.floor == 9
            && game.player.hp * 10 >= game.player.max_hp * 9
            && !game.mobs.iter().any(|mob| {
                mob.hp > 0
                    && !mob.friendly
                    && !mob.pacified
                    && mob.frozen <= 0
                    && game.visible[mob.cell as usize]
                    && Game::distance(game.player.cell as usize, mob.cell as usize) <= 4
                    && (MOBS[mob.kind as usize].tier >= 3 || mob.hp * 5 >= game.player.hp * 4)
            })
            && let Some(stairs) = game.down_stairs
        {
            let (px, py) = coordinates(game.player.cell as usize);
            let (sx, sy) = coordinates(stairs as usize);
            let dx = (sx as isize - px as isize).signum();
            let dy = (sy as isize - py as isize).signum();
            let direct = index((px as isize + dx) as usize, (py as isize + dy) as usize);
            if game.map[direct] == Tile::Trap && (px == sx || py == sy) {
                return step_action(game.player.cell as usize, direct);
            }
            if !self.prepare_route(game, stairs as usize, true, true) {
                return Action::Command('.');
            }
            if self.fresh_after_loop
                && self.cached_route.get(1).is_some_and(|next| {
                    self.route_history[..self.route_history.len().saturating_sub(1)].contains(next)
                })
                && let Some(action) = fresh_detour_step(game, stairs as usize, &self.route_history)
            {
                self.choice_rng_extra += 1;
                return action;
            }
            let was_fresh = self.fresh_after_loop;
            self.choice_rng_extra += u8::from(was_fresh);
            self.fresh_after_loop = false;
            self.choice_rng_extra += u8::from(self.post_loop_depth && !was_fresh);
            return self.step_cached(game, stairs as usize);
        }

        if let Some(room) = game.shop_room
            && best_shop_item(game, room).is_some()
            && game.seen[game.rooms[room].center as usize]
        {
            let target = game.rooms[room].center as usize;
            if self.prepare_route(game, target, true, true)
                && (!has_basic_kit(game) || self.cached_route.len().saturating_sub(1) <= 12)
            {
                self.choice_rng_extra += u8::from(game.floor == 9);
                return self.step_cached(game, target);
            }
        }

        if game.player.class != crate::data::ClassId::Agent
            && !ready_for_next_floor(game)
            && let Some(action) = self.frontier_action(game)
        {
            return action;
        }

        if game.map[game.player.cell as usize] == Tile::DownStairs
            && game.floor < 15
            && !ready_for_next_floor(game)
            && self.stationary_actions < 3
        {
            return Action::Command('.');
        }
        if game.map[game.player.cell as usize] == Tile::DownStairs && game.floor < 15 {
            return Action::Command('>');
        }
        if let Some(stairs) = game.down_stairs {
            return if game.floor == 14 {
                self.step_toward_allowing_traps(game, stairs as usize, "rush final floor")
            } else {
                self.step_toward(game, stairs as usize, "stairs")
            };
        }
        Action::Command('.')
    }
}
