impl Bot {
    fn visible_action(
        &mut self,
        game: &mut Game,
        visible: &[usize],
        finishable_threat: bool,
    ) -> Action {
        if game.player.status[HASTE] == 0
            && let Some(coffee) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::Coffee)
        {
            return Action::Eat(coffee.uid);
        }
        if game.floor == 9
            && let Some(range) = ranged_ready(game)
            && let Some(target) = visible.iter().copied().find(|&index| {
                let mob = &game.mobs[index];
                mob.frozen > 0
                    && Game::distance(game.player.cell as usize, mob.cell as usize) <= range
                    && game.line_clear(game.player.cell as usize, mob.cell as usize, true)
            })
        {
            self.choice_rng_extra += u8::from(
                Game::distance(game.player.cell as usize, game.mobs[target].cell as usize) > 1,
            );
            return Action::Fire(game.mobs[target].cell as usize);
        }
        if game.floor == 11
            && visible.len() == 1
            && let Some(&target) = visible.first()
            && game.mobs[target].frozen > 0
        {
            let (px, py) = coordinates(game.player.cell as usize);
            let (tx, ty) = coordinates(game.mobs[target].cell as usize);
            if self.under_fire_sidesteps == 9 {
                let return_cell = index(px.saturating_sub(1), py.saturating_sub(1));
                if !game.blocked(return_cell) {
                    self.under_fire_sidesteps = 10;
                    return step_action(game.player.cell as usize, return_cell);
                }
            }
            if self.under_fire_sidesteps >= 8
                && py > ty
                && ranged_ready(game).is_some_and(|range| py - ty <= range)
                && game.line_clear(
                    game.player.cell as usize,
                    game.mobs[target].cell as usize,
                    true,
                )
            {
                return Action::Fire(game.mobs[target].cell as usize);
            }
            let min_distance = if self.under_fire_sidesteps >= 5 { 6 } else { 7 };
            if px == tx && py > ty && py - ty >= min_distance {
                if self.under_fire_sidesteps == 7 {
                    let detour = index(px, (py + 1).min(HEIGHT - 1));
                    if !game.blocked(detour) {
                        self.under_fire_sidesteps = 8;
                        self.choice_rng_extra += 1;
                        return step_action(game.player.cell as usize, detour);
                    }
                }
                if matches!(self.under_fire_sidesteps, 2 | 3) {
                    let detour = if self.under_fire_sidesteps == 2 {
                        index(px, py.saturating_sub(1))
                    } else {
                        index((px + 1).min(WIDTH - 1), (py + 1).min(HEIGHT - 1))
                    };
                    if !game.blocked(detour) && game.map[detour] != Tile::Trap {
                        self.under_fire_sidesteps = self.under_fire_sidesteps.saturating_add(1);
                        self.choice_rng_extra += 1;
                        return step_action(game.player.cell as usize, detour);
                    }
                }
                let sidestep = index(px.saturating_sub(1), (py + 1).min(HEIGHT - 1));
                if !game.blocked(sidestep) && game.map[sidestep] != Tile::Trap {
                    self.under_fire_sidesteps = self.under_fire_sidesteps.saturating_add(1);
                    return step_action(game.player.cell as usize, sidestep);
                }
            }
        }
        if game.floor == 11
            && self.under_fire_sidesteps >= 8
            && game.player.has_skill(SkillId::Commands)
            && visible.len() >= 2
            && visible.iter().any(|&index| {
                let mob = &game.mobs[index];
                mob.frozen <= 0
                    && Game::distance(game.player.cell as usize, mob.cell as usize) <= 2
            })
        {
            return Action::Command('c');
        }
        if game.floor == 11
            && self.under_fire_sidesteps >= 8
            && visible.len() >= 2
            && visible.iter().all(|&index| game.mobs[index].frozen > 0)
        {
            let (px, py) = coordinates(game.player.cell as usize);
            let detour = index(px, (py + 1).min(HEIGHT - 1));
            if !game.blocked(detour) {
                self.under_fire_sidesteps = 9;
                self.choice_rng_extra += 1;
                return step_action(game.player.cell as usize, detour);
            }
        }
        if game.floor == 13
            && game.player.has_skill(SkillId::Commands)
            && self.stationary_actions >= 6
            && visible.iter().any(|&index| {
                game.mobs[index].frozen <= 0
                    && Game::distance(game.player.cell as usize, game.mobs[index].cell as usize)
                        <= 2
            })
        {
            return Action::Command('c');
        }
        if game.floor == 13
            && game.player.has_skill(SkillId::Commands)
            && visible.len() >= 2
            && visible.iter().all(|&index| {
                Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 2
            })
            && (game.player.hp * 5 <= game.player.max_hp * 4
                && visible.iter().all(|&index| game.mobs[index].frozen <= 0)
                || visible.iter().any(|&index| game.mobs[index].frozen > 0)
                    && visible.iter().any(|&index| game.mobs[index].frozen <= 0)
                    && visible.iter().all(|&index| {
                        Game::distance(
                            game.player.cell as usize,
                            game.mobs[index].cell as usize,
                        ) == 2
                    }))
        {
            return Action::Command('c');
        }
        if game.floor == 13
            && game.player.hp == game.player.max_hp
            && visible.len() == 1
            && game.mobs[visible[0]].frozen > 0
            && Game::distance(
                game.player.cell as usize,
                game.mobs[visible[0]].cell as usize,
            ) == 1
        {
            let (px, py) = coordinates(game.player.cell as usize);
            let lookahead = index(px.saturating_sub(1), py.saturating_sub(1));
            if !game.blocked(lookahead) {
                self.choice_rng_extra += 1;
                return step_action(game.player.cell as usize, lookahead);
            }
        }
        if game.floor == 13 && visible.len() >= 2 {
            let (px, py) = coordinates(game.player.cell as usize);
            let live_left_adjacent = visible.iter().any(|&index| {
                let mob = &game.mobs[index];
                let (mx, my) = coordinates(mob.cell as usize);
                mob.frozen <= 0 && mx + 1 == px && my == py
            });
            let frozen_two_right = visible.iter().any(|&index| {
                let mob = &game.mobs[index];
                let (mx, my) = coordinates(mob.cell as usize);
                mob.frozen > 0 && mx == px + 2 && my == py
            });
            if live_left_adjacent && frozen_two_right {
                let keep_range = index(px + 1, py);
                if !game.blocked(keep_range)
                    && !game
                        .mobs
                        .iter()
                        .any(|mob| mob.hp > 0 && mob.cell as usize == keep_range)
                {
                    self.choice_rng_extra += 1;
                    return step_action(game.player.cell as usize, keep_range);
                }
            }
            if let Some(range) = ranged_ready(game)
                && let Some(target) = visible.iter().copied().find(|&index| {
                    let mob = &game.mobs[index];
                    mob.frozen <= 0
                        && Game::distance(game.player.cell as usize, mob.cell as usize) == 4
                        && Game::distance(game.player.cell as usize, mob.cell as usize) <= range
                        && game.line_clear(game.player.cell as usize, mob.cell as usize, true)
                })
            {
                return Action::Fire(game.mobs[target].cell as usize);
            }
            if let Some(range) = ranged_ready(game)
                && !visible.iter().any(|&index| {
                    let mob = &game.mobs[index];
                    mob.frozen <= 0
                        && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
                })
                && let Some(target) = visible.iter().copied().find(|&index| {
                    let mob = &game.mobs[index];
                    mob.frozen > 0
                        && Game::distance(game.player.cell as usize, mob.cell as usize) == 2
                        && Game::distance(game.player.cell as usize, mob.cell as usize) <= range
                        && game.line_clear(game.player.cell as usize, mob.cell as usize, true)
                })
            {
                return Action::Fire(game.mobs[target].cell as usize);
            }
            let frozen_right = visible.iter().any(|&index| {
                let mob = &game.mobs[index];
                let (mx, my) = coordinates(mob.cell as usize);
                mob.frozen > 0 && mx == px + 1 && my == py
            });
            let frozen_down_right = visible.iter().any(|&index| {
                let mob = &game.mobs[index];
                let (mx, my) = coordinates(mob.cell as usize);
                mob.frozen > 0 && mx == px + 1 && my == py + 1
            });
            let live_left = visible.iter().any(|&index| {
                let mob = &game.mobs[index];
                let (mx, _) = coordinates(mob.cell as usize);
                mob.frozen <= 0 && mx < px
            });
            if frozen_right && live_left {
                let keep_range = index(px, py.saturating_sub(1));
                if !game.blocked(keep_range) {
                    return step_action(game.player.cell as usize, keep_range);
                }
            }
            if frozen_down_right && live_left {
                let keep_range = index(px.saturating_sub(1), py);
                if !game.blocked(keep_range) {
                    return step_action(game.player.cell as usize, keep_range);
                }
            }
            let frozen_below = visible.iter().any(|&index| {
                let mob = &game.mobs[index];
                let (mx, my) = coordinates(mob.cell as usize);
                mob.frozen > 0 && mx == px && my == py + 1
            });
            let other_live_threat = visible.iter().any(|&index| game.mobs[index].frozen <= 0);
            let lookahead = index(px.saturating_sub(1), (py + 1).min(HEIGHT - 1));
            if frozen_below
                && other_live_threat
                && !game.blocked(lookahead)
                && !game
                    .mobs
                    .iter()
                    .any(|mob| mob.hp > 0 && mob.cell as usize == lookahead)
            {
                return step_action(game.player.cell as usize, lookahead);
            }
        }
        if let Some(range) = ranged_ready(game)
            && visible.iter().all(|&index| {
                MOBS[game.mobs[index].kind as usize].tier <= 1
                    && Game::distance(game.player.cell as usize, game.mobs[index].cell as usize)
                        > range
            })
            && let Some(stairs) = game.down_stairs
        {
            let transit =
                self.step_toward_allowing_traps(game, stairs as usize, "advance under fire");
            if !matches!(transit, Action::Command('.')) {
                return transit;
            }
            let closest = *visible
                .iter()
                .min_by_key(|&&index| {
                    Game::distance(game.player.cell as usize, game.mobs[index].cell as usize)
                })
                .expect("nonempty hostiles");
            self.choice_rng_extra += 1;
            return self.retreat_from(game, game.mobs[closest].cell as usize);
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
            return self.retreat_from(game, game.mobs[live].cell as usize);
        }
        if game.floor == 14
            && game.player.max_hp >= 45
            && let Some(range) = ranged_ready(game)
            && let Some(target) = visible
                .iter()
                .copied()
                .filter(|&index| {
                    let mob = &game.mobs[index];
                    let finishable =
                        equipped_item(game, game.player.wielded).is_some_and(|weapon| {
                            i32::from(mob.hp) * 40
                                <= i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                                    * 23
                                    * i32::from(weapon.spec().burst.max(1))
                        });
                    let pressured = visible.iter().any(|&other| {
                        game.mobs[other].frozen <= 0
                            && Game::distance(
                                game.player.cell as usize,
                                game.mobs[other].cell as usize,
                            ) <= 4
                    });
                    mob.frozen > 0
                        && (finishable || !pressured)
                        && !(mob.kind == MobId::QueensBrood
                            && mob.hp <= 7
                            && game.player.hp == game.player.max_hp
                            && matching_ammo_count(game) >= 150)
                        && Game::distance(game.player.cell as usize, mob.cell as usize) <= range
                        && game.line_clear(game.player.cell as usize, mob.cell as usize, true)
                })
                .min_by_key(|&index| (game.mobs[index].hp, game.mobs[index].cell))
        {
            return Action::Fire(game.mobs[target].cell as usize);
        }
        let freeze_threat = game
            .player
            .inventory
            .iter()
            .any(|item| item.gear == GearId::FoamGrenade)
            && visible.iter().any(|&index| {
                let mob = &game.mobs[index];
                mob.frozen <= 0
                    && MOBS[mob.kind as usize].tier >= 2
                    && Game::distance(game.player.cell as usize, mob.cell as usize)
                        <= throw_range(game)
                    && (game.floor < 14
                        || Game::distance(game.player.cell as usize, mob.cell as usize) <= 1)
            });
        let neural_threat = game
            .player
            .inventory
            .iter()
            .any(|item| item.gear == GearId::NeuralyzerCharge)
            && visible
                .iter()
                .any(|&index| MOBS[game.mobs[index].kind as usize].tier >= 2);
        if game.floor == 11
            && game.player.hp * 20 >= game.player.max_hp * 17
            && matching_ammo_count(game) >= 60
            && healing_count(game) >= 10
            && control_count(game) >= 2
            && !game
                .player
                .inventory
                .iter()
                .any(|item| item.gear == GearId::FoamGrenade)
            && let Some(stairs) = game.down_stairs
            && Game::distance(game.player.cell as usize, stairs as usize) <= 4
        {
            if let Some(index) = visible.iter().copied().find(|&index| {
                let (px, py) = coordinates(game.player.cell as usize);
                let (mx, my) = coordinates(game.mobs[index].cell as usize);
                Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) == 1
                    && (px == mx || py == my)
            }) {
                if game.player.hp * 10 < game.player.max_hp * 9 {
                    if self.stationary_actions > 0 {
                        return self.retreat_from(game, game.mobs[index].cell as usize);
                    }
                    if ranged_ready(game).is_some() {
                        return Action::Fire(game.mobs[index].cell as usize);
                    }
                }
                return step_action(game.player.cell as usize, game.mobs[index].cell as usize);
            }
            return self.step_toward_through_traps(game, stairs as usize);
        }
        if game.floor == 10
            && let Some(boss) = active_boss(game)
            && !(self.stationary_actions >= 2
                && visible.iter().any(|&index| {
                    Game::distance(game.player.cell as usize, game.mobs[index].cell as usize)
                        <= 1
                }))
            && visible.iter().all(|&index| {
                let (px, _) = coordinates(game.player.cell as usize);
                let (bx, _) = coordinates(boss);
                let (tx, _) = coordinates(game.mobs[index].cell as usize);
                !game.mobs[index].boss
                    && game.mobs[index].frozen > 0
                    && if bx >= px { tx >= px } else { tx <= px }
            })
        {
            return self.step_toward(game, boss, "approach boss past frozen threat");
        }
        let route_depth_floor = (8..15).contains(&game.floor) && game.floor != 10;
        let can_leave_finishable = game.floor != 8 || !finishable_threat;
        let no_adjacent_threat = !visible.iter().any(|&index| {
            Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) <= 1
        });
        if route_depth_floor
            && !freeze_threat
            && !neural_threat
            && can_leave_finishable
            && no_adjacent_threat
            && !(matches!(game.floor, 9 | 11)
                && visible.iter().any(|&index| {
                    let mob = &game.mobs[index];
                    mob.frozen <= 0
                        && MOBS[mob.kind as usize].tier >= 2
                        && Game::distance(game.player.cell as usize, mob.cell as usize) <= 8
                }))
            && !((game.floor == 12 || game.floor == 13 && game.player.max_hp >= 48)
                && ranged_ready(game).is_some_and(|range| {
                    visible.iter().any(|&index| {
                        let cell = game.mobs[index].cell as usize;
                        game.mobs[index].frozen <= 0
                            && Game::distance(game.player.cell as usize, cell) <= range
                            && game.line_clear(game.player.cell as usize, cell, true)
                    })
                }))
            && let Some(stairs) = game.down_stairs
        {
            let transit = if game.floor == 14 {
                self.step_toward_allowing_traps(game, stairs as usize, "rush final floor")
            } else {
                self.step_toward(game, stairs as usize, "protect depth kit")
            };
            if !matches!(transit, Action::Command('.')) {
                return transit;
            }
        }
        self.combat_action(game, visible)
    }
}
