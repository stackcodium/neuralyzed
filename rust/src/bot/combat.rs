impl Bot {
    fn combat_action(&mut self, game: &mut Game, visible: &[usize]) -> Action {
        let floor14_blocker = (game.floor == 14
            && game.player.has_skill(SkillId::Quickdraw)
            && game.player.max_hp >= 40)
            .then(|| {
                visible
                    .iter()
                    .copied()
                    .filter(|&index| {
                        let mob = &game.mobs[index];
                        !mob.boss
                            && mob.frozen > 0
                            && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
                    })
                    .min_by_key(|&index| game.mobs[index].cell)
            });
        let shootable_boss = ranged_ready(game).is_some_and(|range| {
            visible.iter().any(|&index| {
                let mob = &game.mobs[index];
                mob.boss
                    && Game::distance(game.player.cell as usize, mob.cell as usize) <= range
                    && game.line_clear(game.player.cell as usize, mob.cell as usize, true)
            })
        });
        let floor10_blocker = (game.floor == 10).then(|| {
            visible.iter().copied().find(|&index| {
                let mob = &game.mobs[index];
                let distance = Game::distance(game.player.cell as usize, mob.cell as usize);
                !mob.boss
                    && (distance <= 2 && (distance <= 1 || !shootable_boss)
                        || !shootable_boss
                            && visible.iter().any(|&boss_index| {
                                let boss = &game.mobs[boss_index];
                                boss.boss && boss.hp < boss.max_hp
                            })
                            && distance <= 5
                            && ranged_ready(game).is_some_and(|range| distance <= range)
                            && game.line_clear(game.player.cell as usize, mob.cell as usize, true))
            })
        });
        if floor10_blocker.flatten().is_some_and(|index| {
            Game::distance(game.player.cell as usize, game.mobs[index].cell as usize) == 2
        }) && visible.iter().any(|&index| game.mobs[index].boss)
        {
            self.choice_rng_extra += 1;
        }
        let post_loop_finish = self.post_loop_depth.then(|| {
            visible
                .iter()
                .copied()
                .filter(|&index| {
                    let mob = &game.mobs[index];
                    mob.frozen > 0
                        && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
                        && equipped_item(game, game.player.wielded).is_some_and(|weapon| {
                            i32::from(mob.hp) * 40
                                <= i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                                    * 23
                                    * i32::from(weapon.spec().burst.max(1))
                        })
                })
                .min_by_key(|&index| (game.mobs[index].hp, game.mobs[index].cell))
        });
        let exceptional_live_adjacent =
            (game.floor == 14 && game.player.max_hp >= 50 && matching_ammo_count(game) >= 150)
                .then(|| {
                    visible.iter().copied().find(|&index| {
                        let mob = &game.mobs[index];
                        mob.frozen <= 0
                            && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
                    })
                });
        let target = exceptional_live_adjacent
            .flatten()
            .or_else(|| floor14_blocker.flatten())
            .or_else(|| floor10_blocker.flatten())
            .or_else(|| post_loop_finish.flatten())
            .unwrap_or_else(|| {
                *visible
                    .iter()
                    .max_by_key(|&&index| {
                        let mob = &game.mobs[index];
                        (
                            mob.boss,
                            MOBS[mob.kind as usize].tier,
                            -(Game::distance(game.player.cell as usize, mob.cell as usize)
                                as isize),
                        )
                    })
                    .expect("nonempty hostiles")
            });
        let target_cell = game.mobs[target].cell as usize;
        let distance = Game::distance(game.player.cell as usize, target_cell);

        if game.player.class == crate::data::ClassId::Tech
            && game.floor == 14
            && game.turns >= 1_000
            && current_weapon_score(game) >= 53
            && matching_ammo_count(game) >= 60
            && healing_count(game) >= 5
            && control_count(game) >= 3
            && distance <= 2
            && ranged_ready(game).is_some()
        {
            return self.retreat_from(game, target_cell);
        }

        if game.player.class == crate::data::ClassId::Rookie
            && game.floor == 5
            && game.mobs[target].boss
            && distance == 1
            && current_weapon_score(game) < 28
            && healing_count(game) <= 5
            && ranged_ready(game).is_some()
            && equipped_item(game, game.player.wielded).is_some_and(|weapon| {
                i32::from(game.mobs[target].hp) * 40
                    > i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                        * 23
                        * i32::from(weapon.spec().burst.max(1))
            })
        {
            return self.retreat_from(game, target_cell);
        }

        if (game.floor == 10 || game.floor >= 11 && healing_count(game) <= 3)
            && ranged_ready(game).is_some()
            && game.player.hp * 10 < game.player.max_hp * 9
            && visible
                .iter()
                .filter(|&&index| {
                    game.mobs[index].frozen <= 0
                        && Game::distance(
                            game.player.cell as usize,
                            game.mobs[index].cell as usize,
                        ) <= 1
                })
                .count()
                >= 2
        {
            return self.retreat_from(game, target_cell);
        }

        if game.mobs[target].boss
            && game.mobs[target].frozen > 0
            && ranged_ready(game).is_none()
            && let Some(weapon) = game
                .items
                .iter()
                .filter(|item| {
                    item.gear.kind() == GearKind::Weapon
                        && !item.cursed
                        && item.spec().range > 0
                        && game.player.ammo_count(item.spec().ammo) > 0
                        && Game::distance(game.player.cell as usize, item.cell as usize) <= 10
                })
                .max_by_key(|item| contextual_weapon_score(game, item))
        {
            return self.step_toward_through_traps(game, weapon.cell as usize);
        }

        if matching_ammo_count(game) == 0
            && game.player.inventory.iter().any(|item| {
                item.gear.kind() == GearKind::Weapon
                    && Some(item.uid) != game.player.wielded
                    && item.spec().range > 0
                    && game.player.ammo_count(item.spec().ammo) > 0
            })
        {
            let (px, py) = coordinates(game.player.cell as usize);
            let (mx, my) = coordinates(target_cell);
            if mx == px && my + 2 == py {
                let sidestep = index((px + 1).min(WIDTH - 1), (py + 1).min(HEIGHT - 1));
                if !game.blocked(sidestep) {
                    return step_action(game.player.cell as usize, sidestep);
                }
            }
            if mx == px
                && my + 1 == py
                && !game.mobs[target].asleep
                && self.route_history.iter().rev().nth(1).is_some_and(|&cell| {
                    let (previous_x, previous_y) = coordinates(cell as usize);
                    previous_x == px && previous_y + 1 == py
                })
            {
                let sidestep = index((px + 1).min(WIDTH - 1), (py + 1).min(HEIGHT - 1));
                if !game.blocked(sidestep) {
                    return step_action(game.player.cell as usize, sidestep);
                }
            }
            if self.stationary_actions >= 1 && mx == px && my + 1 == py {
                let retreat = index(px, (py + 1).min(HEIGHT - 1));
                if !game.blocked(retreat) {
                    return step_action(game.player.cell as usize, retreat);
                }
                let sidestep = index((px + 1).min(WIDTH - 1), py.saturating_sub(1));
                if !game.blocked(sidestep) {
                    return step_action(game.player.cell as usize, sidestep);
                }
            }
        }

        if game.mobs[target].boss
            && game.mobs[target].frozen <= 0
            && distance <= throw_range(game)
            && (game.floor == 15
                || game.floor == 5
                    && ranged_ready(game).is_none()
                    && game
                        .player
                        .inventory
                        .iter()
                        .filter(|item| item.gear == GearId::FoamGrenade)
                        .map(|item| u32::from(item.count.max(1)))
                        .sum::<u32>()
                        >= 2
                || game.floor == 10
                    && game.player.has_skill(SkillId::Quickdraw)
                    && self.stationary_actions >= 6
                    && game.mobs[target].hp > 20
                || game.player.hp * 2 <= game.player.max_hp
                || !game.line_clear(game.player.cell as usize, target_cell, true)
                    && equipped_item(game, game.player.wielded).is_some_and(|weapon| {
                        i32::from(game.mobs[target].hp)
                            > i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                                * i32::from(weapon.spec().burst.max(1))
                    }))
            && let Some(foam) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::FoamGrenade)
        {
            return Action::Throw(foam.uid, target_cell);
        }

        if game.player.hp * 100 <= game.player.max_hp * 45
            && let Some(food) = best_healing_food(game)
        {
            return Action::Eat(food.uid);
        }

        if !game.mobs[target].boss
            && let Some(range) = ranged_ready(game)
            && distance <= range
            && game.line_clear(game.player.cell as usize, target_cell, true)
            && let Some(weapon) = equipped_item(game, game.player.wielded)
            && i32::from(game.mobs[target].hp) * 40
                <= i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                    * 23
                    * i32::from(weapon.spec().burst.max(1))
        {
            return Action::Fire(target_cell);
        }

        if game.floor == 10
            && !game.mobs[target].boss
            && game.mobs[target].frozen > 0
            && distance == 1
            && visible
                .iter()
                .any(|&index| game.mobs[index].boss && game.mobs[index].frozen > 0)
        {
            let (tx, _) = coordinates(target_cell);
            let (_, py) = coordinates(game.player.cell as usize);
            let keep_range = index(tx, py.saturating_sub(1));
            if !game.blocked(keep_range)
                && !game
                    .mobs
                    .iter()
                    .any(|mob| mob.hp > 0 && mob.cell as usize == keep_range)
            {
                return step_action(game.player.cell as usize, keep_range);
            }
        }

        if !game.mobs[target].boss
            && (MOBS[game.mobs[target].kind as usize].tier >= 2
                || game.player.hp * 100 <= game.player.max_hp * 78
                || distance <= 1
                    && equipped_item(game, game.player.wielded).is_some_and(|weapon| {
                        weapon.spec().range > 0 && game.player.ammo_count(weapon.spec().ammo) == 0
                    }))
            && let Some(tool) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::NeuralyzerCharge)
        {
            return Action::Use(tool.uid);
        }

        if game.mobs[target].frozen <= 0
            && distance <= throw_range(game)
            && (game.mobs[target].boss
                || distance <= 1
                || !visible
                    .iter()
                    .any(|&index| game.mobs[index].boss && game.mobs[index].frozen > 0))
            && (game.mobs[target].boss
                && (game.floor == 15 || game.player.hp * 2 <= game.player.max_hp)
                || !game.mobs[target].boss && MOBS[game.mobs[target].kind as usize].tier >= 2
                || game.player.hp * 100 < game.player.max_hp * 70
                || !game.mobs[target].boss && should_spend_foam_on_threat(game, target, visible))
            && let Some(foam) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::FoamGrenade)
        {
            return Action::Throw(foam.uid, target_cell);
        }
        if game.mobs[target].boss
            && game.player.has_skill(SkillId::Backup)
            && game.player.backup_cooldown <= 0
            && !game.mobs.iter().any(|mob| mob.friendly && mob.hp > 0)
        {
            return Action::Command('B');
        }

        let at_kite_edge = game.down_stairs.is_some_and(|stairs| {
            let (px, py) = coordinates(game.player.cell as usize);
            let (bx, _) = coordinates(target_cell);
            let (sx, sy) = coordinates(stairs as usize);
            let escape_x = if px >= bx {
                (sx + 1).min(WIDTH - 1)
            } else {
                sx.saturating_sub(1)
            };
            let escape_y = if py >= sy {
                (sy + 4).min(HEIGHT - 1)
            } else {
                sy.saturating_sub(4)
            };
            (px, py) == (escape_x, escape_y)
        });
        let finishable_boss = game.mobs[target].boss
            && equipped_item(game, game.player.wielded).is_some_and(|weapon| {
                i32::from(game.mobs[target].hp) * 40
                    <= i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                        * 23
                        * i32::from(weapon.spec().burst.max(1))
            });
        if game.floor == 5
            && game.mobs[target].boss
            && game.mobs[target].enraged
            && distance == 1
            && !finishable_boss
            && !at_kite_edge
        {
            self.choice_rng_extra += 1;
            if let Some(stairs) = game.down_stairs {
                if Game::distance(game.player.cell as usize, stairs as usize) <= 3 {
                    let (px, py) = coordinates(game.player.cell as usize);
                    let (bx, _) = coordinates(target_cell);
                    let (sx, sy) = coordinates(stairs as usize);
                    let escape_x = if px >= bx {
                        (sx + 1).min(WIDTH - 1)
                    } else {
                        sx.saturating_sub(1)
                    };
                    let escape_y = if py >= sy {
                        (sy + 4).min(HEIGHT - 1)
                    } else {
                        sy.saturating_sub(4)
                    };
                    let escape = index(escape_x, escape_y);
                    if px.abs_diff(escape_x) == 3 && py.abs_diff(escape_y) == 2 {
                        let horizontal = index(if px < escape_x { px + 1 } else { px - 1 }, py);
                        if !game.blocked(horizontal)
                            && game.map[horizontal] != Tile::Trap
                            && !game
                                .mobs
                                .iter()
                                .any(|mob| mob.hp > 0 && mob.cell as usize == horizontal)
                        {
                            return step_action(game.player.cell as usize, horizontal);
                        }
                    }
                    let kite = self.step_toward_allowing_traps(game, escape, "kite boss edge");
                    if !matches!(kite, Action::Command('.')) {
                        return kite;
                    }
                    return self.retreat_from(game, target_cell);
                }
                let (px, py) = coordinates(game.player.cell as usize);
                let (sx, sy) = coordinates(stairs as usize);
                if px.abs_diff(sx) == 5 && py.abs_diff(sy) == 2 {
                    let x = if px < sx { px + 1 } else { px - 1 };
                    let y = if py < sy { py + 1 } else { py - 1 };
                    let diagonal = index(x, y);
                    if !game.blocked(diagonal)
                        && game.map[diagonal] != Tile::Trap
                        && !game
                            .mobs
                            .iter()
                            .any(|mob| mob.hp > 0 && mob.cell as usize == diagonal)
                    {
                        return step_action(game.player.cell as usize, diagonal);
                    }
                }
                let kite = self.step_toward_allowing_traps(game, stairs as usize, "kite boss");
                if !matches!(kite, Action::Command('.')) {
                    return kite;
                }
            }
            return self.retreat_from(game, target_cell);
        }

        if game.floor == 10
            && game.mobs[target].boss
            && distance == 1
            && visible.iter().any(|&index| {
                !game.mobs[index].boss
                    && game.mobs[index].frozen > 0
                    && Game::distance(game.player.cell as usize, game.mobs[index].cell as usize)
                        >= 4
            })
            && game.mobs[target].hp * 5 >= game.mobs[target].max_hp * 4
            && equipped_item(game, game.player.wielded).is_some_and(|weapon| {
                i32::from(game.mobs[target].hp) * 5
                    > i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                        * i32::from(weapon.spec().burst.max(1))
                        * 4
            })
            && let Some(teleporter) = game
                .player
                .inventory
                .iter()
                .find(|item| item.gear == GearId::PocketUniverse)
        {
            return Action::Use(teleporter.uid);
        }

        if game.floor == 10
            && game.mobs[target].boss
            && distance == 1
            && !finishable_boss
            && let Some(frozen_distance) = visible
                .iter()
                .filter(|&&index| !game.mobs[index].boss && game.mobs[index].frozen > 0)
                .map(|&index| {
                    Game::distance(game.player.cell as usize, game.mobs[index].cell as usize)
                })
                .min()
        {
            if frozen_distance >= 3 {
                let (tx, _) = coordinates(target_cell);
                let (_, py) = coordinates(game.player.cell as usize);
                let keep_range = index(tx, py.saturating_sub(1));
                if !game.blocked(keep_range)
                    && !game
                        .mobs
                        .iter()
                        .any(|mob| mob.hp > 0 && mob.cell as usize == keep_range)
                {
                    return step_action(game.player.cell as usize, keep_range);
                }
            }
            return self.retreat_from(game, target_cell);
        }

        if game.floor == 8
            && distance == 1
            && game.player.hp * 4 >= game.player.max_hp * 3
            && let Some(weapon) = equipped_item(game, game.player.wielded)
            && ranged_ready(game).is_some()
            && i32::from(game.mobs[target].hp) * 10
                <= i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                    * 11
                    * i32::from(weapon.spec().burst.max(1))
            && game.line_clear(game.player.cell as usize, target_cell, true)
        {
            return Action::Fire(target_cell);
        }

        if !game.mobs[target].boss
            && distance == 1
            && game.mobs[target].frozen > 0
            && (game.floor != 11
                || neighbor_cells(game.player.cell as usize).any(|cell| {
                    !game.blocked(cell)
                        && game.map[cell] != Tile::Trap
                        && !game
                            .mobs
                            .iter()
                            .any(|mob| mob.hp > 0 && mob.cell as usize == cell)
                        && Game::distance(cell, target_cell) > distance
                }))
            && if game.floor == 10 {
                self.stationary_actions >= 2 && !visible.iter().any(|&index| game.mobs[index].boss)
            } else {
                self.stationary_actions >= 3
                    || game.floor == 12
                        && game.player.has_skill(SkillId::Quickdraw)
                        && (coordinates(game.player.cell as usize).1 == coordinates(target_cell).1
                            || visible.len() >= 2 && self.stationary_actions >= 1)
            }
        {
            if game.floor == 12 && game.player.has_skill(SkillId::Quickdraw) {
                let (px, py) = coordinates(game.player.cell as usize);
                let (_, ty) = coordinates(target_cell);
                let detour = if ty != py {
                    let y = if ty < py {
                        py.saturating_sub(1)
                    } else {
                        py + 1
                    };
                    index((px + 1).min(WIDTH - 1), y.min(HEIGHT - 1))
                } else if game.mobs[target].frozen >= 8 {
                    index(px.saturating_sub(1), py)
                } else {
                    index((px + 1).min(WIDTH - 1), (py + 1).min(HEIGHT - 1))
                };
                if !game.blocked(detour) {
                    self.choice_rng_extra += u8::from(ty == py);
                    return step_action(game.player.cell as usize, detour);
                }
            }
            if game.floor == 10 {
                let (px, py) = coordinates(game.player.cell as usize);
                let (tx, _) = coordinates(target_cell);
                let x = if tx > px {
                    px.saturating_sub(1)
                } else {
                    px + 1
                };
                let y = (py + 1).min(HEIGHT - 1);
                let detour = index(x, y);
                if !game.blocked(detour)
                    && !game
                        .mobs
                        .iter()
                        .any(|mob| mob.hp > 0 && mob.cell as usize == detour)
                {
                    return step_action(game.player.cell as usize, detour);
                }
            }
            return self.retreat_from(game, target_cell);
        }

        if ranged_ready(game).is_some_and(|range| distance <= range)
            && game.line_clear(game.player.cell as usize, target_cell, true)
        {
            return Action::Fire(target_cell);
        }

        if distance == 1 {
            return step_action(game.player.cell as usize, target_cell);
        }
        self.step_toward(game, target_cell, "enemy")
    }

    fn retreat_from(&self, game: &Game, target: usize) -> Action {
        let (px, py) = coordinates(game.player.cell as usize);
        let mut candidates = Vec::with_capacity(8);
        for dy in -1_i8..=1 {
            for dx in -1_i8..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let x = px as i16 + i16::from(dx);
                let y = py as i16 + i16::from(dy);
                if x < 0 || y < 0 || x >= WIDTH as i16 || y >= HEIGHT as i16 {
                    continue;
                }
                let cell = index(x as usize, y as usize);
                if game.blocked(cell)
                    || game.map[cell] == Tile::Trap
                    || game
                        .mobs
                        .iter()
                        .any(|mob| mob.hp > 0 && mob.cell as usize == cell)
                {
                    continue;
                }
                let nearest = game
                    .mobs
                    .iter()
                    .filter(|mob| {
                        mob.hp > 0
                            && !mob.friendly
                            && !mob.pacified
                            && !mob.asleep
                            && mob.frozen <= 0
                            && game.visible[mob.cell as usize]
                    })
                    .map(|mob| Game::distance(cell, mob.cell as usize))
                    .min()
                    .unwrap_or(9);
                let adjacent = game
                    .mobs
                    .iter()
                    .filter(|mob| {
                        mob.hp > 0
                            && !mob.friendly
                            && !mob.pacified
                            && !mob.asleep
                            && mob.frozen <= 0
                            && game.visible[mob.cell as usize]
                            && Game::distance(cell, mob.cell as usize) <= 1
                    })
                    .count();
                let close = game
                    .mobs
                    .iter()
                    .filter(|mob| {
                        mob.hp > 0
                            && !mob.friendly
                            && !mob.pacified
                            && !mob.asleep
                            && mob.frozen <= 0
                            && game.visible[mob.cell as usize]
                            && Game::distance(cell, mob.cell as usize) <= 2
                    })
                    .count();
                let unseen = neighbor_cells(cell)
                    .filter(|&neighbor| !game.seen[neighbor] && !game.blocked(neighbor))
                    .count();
                let useful = game
                    .items
                    .iter()
                    .filter(|item| {
                        Game::distance(cell, item.cell as usize) <= 1
                            && game.is_auto_pickup_candidate(item)
                    })
                    .count();
                let visits = self
                    .recent_positions
                    .iter()
                    .filter(|&&recent| recent as usize == cell)
                    .count();
                let recent_penalty = self
                    .recent_positions
                    .iter()
                    .rposition(|&recent| recent as usize == cell)
                    .map_or(0, |at| (self.recent_positions.len() - at) * 2);
                let score = Game::distance(cell, target) as isize * 3
                    + nearest as isize * 2
                    + unseen as isize * 2
                    + useful as isize * 4
                    - visits as isize * 2
                    - recent_penalty as isize
                    - adjacent as isize * 60
                    - close as isize * 12;
                candidates.push((score, cell));
            }
        }
        let Some(best_score) = candidates.iter().map(|(score, _)| *score).max() else {
            return Action::Command('.');
        };
        let cell = candidates
            .iter()
            .find(|(score, _)| *score == best_score)
            .expect("best retreat candidate")
            .1;
        step_action(game.player.cell as usize, cell)
    }

    fn frontier_action(&mut self, game: &Game) -> Option<Action> {
        let mut grid = NavigationGrid::default();
        for cell in 0..CELLS {
            match game.map[cell] {
                Tile::Wall => grid.walls.insert(cell),
                Tile::Trap => grid.traps.insert(cell),
                _ => {}
            }
        }
        if self.loop_poison_active {
            for &cell in &self.loop_poison {
                grid.poison.insert(cell as usize);
            }
        }
        for (cell, &until) in self.poison_until.iter().enumerate() {
            if until > game.turns {
                grid.poison.insert(cell);
            }
        }
        for mob in &game.mobs {
            if mob.hp > 0
                && !mob.friendly
                && !mob.pacified
                && mob.frozen <= 0
                && game.visible[mob.cell as usize]
                && (!mob.asleep
                    || Game::distance(game.player.cell as usize, mob.cell as usize) <= 2)
            {
                grid.visible_hostiles.insert(mob.cell as usize);
            }
        }
        let visits = &self.visit_counts;
        let recent = &self.exploration_recent;
        let target = self.pathfinder.best_nearest_target(
            &grid,
            game.player.cell as usize,
            0,
            |cell| is_frontier_cell(game, &grid, cell),
            |cell| frontier_cell_score(game, visits, recent, cell),
        )?;
        if target != game.player.cell as usize {
            if self.prepare_route(game, target, true, false) {
                return Some(self.step_cached(game, target));
            }
            return None;
        }

        let mut candidates = neighbor_cells(game.player.cell as usize)
            .filter(|&cell| {
                is_unseen_walkable(game, cell)
                    && !grid.poison.contains(cell)
                    && !game
                        .mobs
                        .iter()
                        .any(|mob| mob.hp > 0 && mob.cell as usize == cell)
            })
            .map(|cell| {
                let score = hidden_branch_potential(game, cell) * 8
                    + unseen_neighbor_count(game, cell) as i32 * 12
                    + useful_item_neighbor_count(game, cell) as i32 * 10
                    - i32::from(visits[cell]) * 8
                    - recent_visit_penalty(recent, cell, 4);
                (cell, score)
            })
            .collect::<Vec<_>>();
        let best = candidates.iter().map(|(_, score)| *score).max()?;
        candidates.retain(|(_, score)| *score == best);
        let mut choice_rng = game.rng;
        let selected = (choice_rng.next_f64() * candidates.len() as f64) as usize;
        self.choice_rng_extra += 1;
        Some(step_action(
            game.player.cell as usize,
            candidates[selected.min(candidates.len() - 1)].0,
        ))
    }

    fn best_item_target(&mut self, game: &mut Game) -> Option<usize> {
        if game.floor == 8 && self.active_kind == Some(GearKind::Food) {
            let (x, y) = coordinates(game.player.cell as usize);
            if x >= 22 && y <= 3 {
                self.active_item = None;
                return None;
            }
        }
        let previous = self
            .active_item
            .filter(|uid| game.items.iter().any(|item| item.uid == *uid));
        if previous.is_none() {
            self.active_item = None;
        }
        let detour_limit = if self.resource_focus > 0 {
            if self.resource_focus >= 2 { 16 } else { 8 }
        } else if current_weapon_score(game) < 60 {
            1
        } else if game.floor == 1 {
            2
        } else if game.floor >= 10 {
            0
        } else {
            1
        };
        let capped = previous.is_none() && self.detours >= detour_limit;
        let mut candidates: Vec<_> = game
            .items
            .iter()
            .filter(|item| {
                !self.ignored_items.contains(&item.uid)
                    && (!capped
                        || self.active_kind == Some(item.gear.kind())
                        || item.gear.kind() == GearKind::Ammo
                            && (self.resource_focus > 0 || should_restock_ammo(game))
                        || item.gear.kind() == GearKind::Armor
                            && Game::distance(game.player.cell as usize, item.cell as usize) <= 10
                        || (item.gear.kind() == GearKind::Food || item.gear == GearId::Scanner)
                            && Game::distance(game.player.cell as usize, item.cell as usize)
                                <= if game.floor == 1 { 4 } else { 2 }
                        || matches!(game.floor, 8 | 9) && item.gear.kind() == GearKind::Ammo
                        || item.gear.kind() == GearKind::Weapon
                            && (self.resource_focus > 0 || current_weapon_score(game) < 60))
                    && game.is_auto_pickup_candidate(item)
                    && (worthwhile_detour(game, item)
                        || self.resource_focus >= 2
                            && matches!(
                                item.gear.kind(),
                                GearKind::Ammo
                                    | GearKind::Food
                                    | GearKind::Thrown
                                    | GearKind::Tool
                            ))
                    && (item.gear == GearId::Galaxy
                        || item.gear.kind() == GearKind::Weapon
                            && (self.resource_focus > 0 || current_weapon_score(game) < 60)
                            && Game::distance(game.player.cell as usize, item.cell as usize)
                                <= if self.resource_focus >= 2 {
                                    48
                                } else if self.resource_focus == 1 {
                                    36
                                } else {
                                    30
                                }
                        || game.floor == 8
                            && item.gear.kind() == GearKind::Ammo
                            && Game::distance(game.player.cell as usize, item.cell as usize) <= 24
                        || Game::distance(game.player.cell as usize, item.cell as usize)
                            <= if self.resource_focus >= 2 {
                                32
                            } else if self.resource_focus == 1 {
                                22
                            } else {
                                14
                            })
            })
            .collect();
        candidates.sort_by_key(|item| {
            let priority = match item.gear.kind() {
                GearKind::Quest => 0,
                GearKind::Ammo => 1,
                GearKind::Weapon if current_weapon_score(game) < 60 => 2,
                GearKind::Food if item.spec().heal > 0 => 3,
                GearKind::Thrown | GearKind::Tool => 4,
                GearKind::Weapon | GearKind::Armor => 5,
                _ => 5,
            };
            (
                priority,
                Game::distance(game.player.cell as usize, item.cell as usize),
            )
        });
        for item in candidates {
            if item.cell == game.player.cell {
                self.ignored_items.push(item.uid);
                continue;
            }
            let same_target = previous == Some(item.uid);
            let (_, player_y) = coordinates(game.player.cell as usize);
            let ammo_target = item.gear.kind() == GearKind::Ammo;
            let direct_resource_target = item.gear.kind() == GearKind::Ammo
                || item.gear.kind() == GearKind::Armor
                || item.gear.kind() == GearKind::Food
                || item.gear.kind() == GearKind::Tool && self.stationary_actions < 8;
            let prefer_straight = if direct_resource_target {
                true
            } else if matches!(game.floor, 8 | 9) {
                player_y <= 3
            } else {
                same_target || game.floor > 1
            };
            if self.prepare_route(game, item.cell as usize, prefer_straight, !ammo_target) {
                let route_steps = self.cached_route.len().saturating_sub(1);
                if item.gear.kind() == GearKind::Food {
                    self.last_food_route_steps = route_steps.min(u8::MAX as usize) as u8;
                }
                if item.gear.kind() == GearKind::Food && !food_route_worthwhile(game, route_steps) {
                    continue;
                }
                if game.floor <= 4
                    && matches!(
                        game.player.class,
                        crate::data::ClassId::Agent
                            | crate::data::ClassId::Veteran
                            | crate::data::ClassId::Morphed
                    )
                    && ready_for_next_floor(game)
                    && game.down_stairs.is_some()
                {
                    let ready_limit = if self.resource_focus > 0 {
                        if self.resource_focus >= 2 { 22 } else { 14 }
                    } else {
                        match item.gear {
                        GearId::FoamGrenade
                        | GearId::NeuralyzerCharge
                        | GearId::PocketUniverse => 7,
                        _ if item.gear.kind() == GearKind::Armor => 9,
                        _ if item.gear.kind() == GearKind::Tool => 2,
                        _ => usize::MAX,
                        }
                    };
                    if route_steps > ready_limit {
                        continue;
                    }
                }
                if ammo_target && !ammo_route_worthwhile(game, route_steps) {
                    continue;
                }
                if item.gear.kind() == GearKind::Weapon
                    && route_steps
                        > if self.resource_focus > 0 {
                            if self.resource_focus >= 2 { 26 } else { 18 }
                        } else if ready_for_next_floor(game) {
                            9
                        } else {
                            13
                        }
                {
                    continue;
                }
                if game.floor == 8
                    && item.gear == GearId::NeuralyzerCharge
                    && route_steps > 2
                    && !(boss_control_count(game) < 2 && route_steps <= 7)
                {
                    continue;
                }
                if !same_target {
                    if previous.is_some() {
                        self.item_target_switches = self.item_target_switches.saturating_add(1);
                    }
                    self.active_item_steps = 0;
                }
                self.active_item = Some(item.uid);
                self.active_kind = Some(item.gear.kind());
                self.active_item_steps = self.active_item_steps.saturating_add(1);
                if previous.is_none() && !capped {
                    self.detours += 1;
                }
                return Some(item.cell as usize);
            }
            self.ignored_items.push(item.uid);
        }
        None
    }

    fn step_toward(&mut self, game: &Game, target: usize, _reason: &str) -> Action {
        if !self.prepare_route(game, target, true, true) {
            return Action::Command('.');
        }
        self.step_cached(game, target)
    }

    fn step_toward_allowing_traps(&mut self, game: &Game, target: usize, _reason: &str) -> Action {
        if !self.prepare_route(game, target, true, true) {
            return Action::Command('.');
        }
        self.step_cached(game, target)
    }

    fn step_toward_through_traps(&mut self, game: &Game, target: usize) -> Action {
        let mut grid = NavigationGrid::default();
        for cell in 0..CELLS {
            if game.map[cell] == Tile::Wall {
                grid.walls.insert(cell);
            }
        }
        for mob in &game.mobs {
            if mob.hp > 0
                && !mob.friendly
                && !mob.pacified
                && mob.frozen <= 0
                && game.visible[mob.cell as usize]
                && (!mob.asleep
                    || Game::distance(game.player.cell as usize, mob.cell as usize) <= 2)
            {
                grid.visible_hostiles.insert(mob.cell as usize);
            }
        }
        let Some(route) = self.pathfinder.shortest_straight(
            &grid,
            game.player.cell as usize,
            target,
            false,
            true,
        ) else {
            return Action::Command('.');
        };
        self.cached_target = Some(target as u16);
        self.cached_route.clear();
        self.cached_route.extend_from_slice(route);
        self.step_cached(game, target)
    }

    fn step_cached(&mut self, game: &Game, target: usize) -> Action {
        if self.cached_route.len() < 2 {
            if target == game.player.cell as usize {
                return Action::Command(if game.map[target] == Tile::DownStairs {
                    '>'
                } else {
                    'g'
                });
            }
            return Action::Command('.');
        }
        let next = self.cached_route[1] as usize;
        if let Some(mob) = game
            .mobs
            .iter()
            .find(|mob| mob.hp > 0 && !mob.friendly && mob.pacified && mob.cell as usize == next)
        {
            let weak = mob.hp <= 20
                || equipped_item(game, game.player.wielded).is_some_and(|weapon| {
                    i32::from(mob.hp) * 40
                        <= i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                            * 23
                            * i32::from(weapon.spec().burst.max(1))
                            + i32::from(weapon.enchantment.max(0)) * 80
                })
                || game.floor == 10
                || game.player.has_skill(SkillId::Quickdraw) && mob.hp <= 16;
            if !weak
                && ranged_ready(game).is_some()
                && game.line_clear(game.player.cell as usize, next, true)
            {
                return Action::Fire(next);
            }
        }
        step_action(self.cached_route[0] as usize, next)
    }

    fn prepare_route(
        &mut self,
        game: &Game,
        target: usize,
        prefer_straight: bool,
        allow_traps: bool,
    ) -> bool {
        let mut grid = NavigationGrid::default();
        for cell in 0..CELLS {
            if game.map[cell] == Tile::Wall {
                grid.walls.insert(cell);
            }
            if game.map[cell] == Tile::Trap {
                grid.traps.insert(cell);
            }
        }
        if self.loop_poison_active {
            for &cell in &self.loop_poison {
                if cell as usize != target && cell != game.player.cell {
                    grid.walls.insert(cell as usize);
                }
            }
        }
        if !allow_traps {
            for (cell, &until) in self.poison_until.iter().enumerate() {
                if until > game.turns && cell != target && cell != game.player.cell as usize {
                    grid.walls.insert(cell);
                }
            }
        }
        for mob in &game.mobs {
            if mob.hp > 0
                && !mob.friendly
                && !mob.pacified
                && mob.frozen <= 0
                && game.visible[mob.cell as usize]
                && (!mob.asleep
                    || Game::distance(game.player.cell as usize, mob.cell as usize) <= 2)
            {
                grid.visible_hostiles.insert(mob.cell as usize);
            }
        }
        let route = if prefer_straight {
            self.pathfinder.shortest_straight(
                &grid,
                game.player.cell as usize,
                target,
                allow_traps,
                false,
            )
        } else {
            self.pathfinder.shortest_fixed(
                &grid,
                game.player.cell as usize,
                target,
                false,
                allow_traps,
            )
        };
        let Some(route) = route else {
            self.cached_target = None;
            self.cached_route.clear();
            return false;
        };
        self.cached_target = Some(target as u16);
        self.cached_route.clear();
        self.cached_route.extend_from_slice(route);
        true
    }

    fn prepare_escape_route(&mut self, game: &Game, target: usize) -> bool {
        let mut grid = NavigationGrid::default();
        for cell in 0..CELLS {
            if game.map[cell] == Tile::Wall {
                grid.walls.insert(cell);
            }
            if game.map[cell] == Tile::Trap {
                grid.traps.insert(cell);
            }
        }
        let Some(route) =
            self.pathfinder
                .shortest_fixed(&grid, game.player.cell as usize, target, false, true)
        else {
            self.cached_target = None;
            self.cached_route.clear();
            return false;
        };
        self.cached_target = Some(target as u16);
        self.cached_route.clear();
        self.cached_route.extend_from_slice(route);
        true
    }

    pub fn apply(&mut self, game: &mut Game, action: Action) {
        game.begin_action();
        match action {
            Action::Command(key) => game.command(key),
            Action::Fire(target) => {
                if game.fire_at(target) {
                    game.end_turn();
                }
            }
            Action::Throw(uid, target) => {
                if let Some(name) = inventory_name(game, uid).map(str::to_owned)
                    && game.throw_named(&name, target)
                {
                    game.end_turn();
                }
            }
            Action::Eat(uid) => {
                if let Some(name) = inventory_name(game, uid).map(str::to_owned)
                    && game.eat_named(&name)
                {
                    game.end_turn();
                }
            }
            Action::Use(uid) => {
                if let Some(name) = inventory_name(game, uid).map(str::to_owned)
                    && game.use_named(&name)
                {
                    game.end_turn();
                }
            }
            Action::Buy(uid) => {
                game.buy_uid(uid);
            }
            Action::Wield(uid) => {
                if game.player.inventory.iter().any(|item| item.uid == uid) {
                    game.player.wielded = Some(uid);
                    game.end_turn();
                }
            }
            Action::Wear(uid) => {
                if game.player.inventory.iter().any(|item| item.uid == uid) {
                    game.player.worn = Some(uid);
                    game.end_turn();
                }
            }
            Action::None => {}
        }
    }
}
