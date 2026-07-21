impl Bot {
    /// Stable, event-oriented state for compact traces. Deliberately excludes
    /// the shrinking route and rolling position window, which change every move.
    pub fn debug_context(&self) -> String {
        format!(
            "target={:?} item={:?}/{:?} switches={} detours={} stationary={} under_fire={} alternating={} loop_break={} force_depth={} teleports={} ignored={:?}",
            self.cached_target,
            self.active_item,
            self.active_kind,
            self.item_target_switches,
            self.detours,
            self.stationary_actions,
            self.under_fire_sidesteps,
            alternating_tail(&self.recent_positions),
            self.combat_loop_break_steps,
            self.force_depth_steps,
            self.loop_teleports,
            self.ignored_items
        )
    }

    pub fn debug_state(&self) -> String {
        format!(
            "target={:?} route_steps={} active_item={:?} active_kind={:?} item_steps={} food_route={} switches={} detours={} stationary={} under_fire={} alternating={} loop_break={} force_depth={} teleports={} predictions={} recent={:?} ignored={:?}",
            self.cached_target,
            self.cached_route.len().saturating_sub(1),
            self.active_item,
            self.active_kind,
            self.active_item_steps,
            self.last_food_route_steps,
            self.item_target_switches,
            self.detours,
            self.stationary_actions,
            self.under_fire_sidesteps,
            alternating_tail(&self.recent_positions),
            self.combat_loop_break_steps,
            self.force_depth_steps,
            self.loop_teleports,
            self.lookahead_predictions,
            self.recent_positions
                .iter()
                .map(|&cell| coordinates(cell as usize))
                .collect::<Vec<_>>(),
            self.ignored_items
        )
    }

    pub fn debug_frontier(&self, game: &Game) -> Vec<((usize, usize), i32)> {
        neighbor_cells(game.player.cell as usize)
            .filter(|&cell| is_unseen_walkable(game, cell))
            .map(|cell| {
                let score = hidden_branch_potential(game, cell) * 8
                    + unseen_neighbor_count(game, cell) as i32 * 12
                    + useful_item_neighbor_count(game, cell) as i32 * 10
                    - i32::from(self.visit_counts[cell]) * 8
                    - recent_visit_penalty(&self.exploration_recent, cell, 4);
                (coordinates(cell), score)
            })
            .collect()
    }

    fn poison_recent_cells(&mut self, turn: u32) {
        let until = turn.saturating_add(90);
        for &cell in &self.recent_positions {
            self.poison_until[cell as usize] = until;
        }
    }

    pub fn choose(&mut self, game: &mut Game) -> Action {
        self.choice_rng_extra = 0;
        self.choice_rng_skip = 0;
        let action = self.choose_action(game);
        if self.floor11_post_teleport > 0 && matches!(action, Action::Throw(_, _) | Action::Fire(_))
        {
            self.choice_rng_extra += 1;
            self.floor11_post_teleport -= 1;
        }
        if game.floor == 13
            && let Action::Fire(cell) = action
            && game.player.hp == game.player.max_hp
            && Game::distance(game.player.cell as usize, cell) <= 1
            && visible_hostiles(game).len() >= 2
        {
            self.choice_rng_extra += 1;
        }
        if game.floor == 14
            && game.player.hp == game.player.max_hp
            && self.stationary_actions <= 1
            && let Action::Fire(cell) = action
            && game.mobs.iter().any(|mob| {
                mob.hp > 0
                    && mob.kind == MobId::QueensBrood
                    && mob.cell as usize == cell
                    && mob.frozen > 0
                    && Game::distance(game.player.cell as usize, cell) <= 1
            })
        {
            self.choice_rng_skip += 1;
        }
        if game.floor == 14
            && game.player.max_hp >= 50
            && game.player.hp * 4 <= game.player.max_hp * 3
            && matching_ammo_count(game) >= 150
            && let Action::Fire(cell) = action
            && game.mobs.iter().any(|mob| {
                mob.hp > 0
                    && !mob.boss
                    && mob.frozen <= 0
                    && mob.cell as usize == cell
                    && Game::distance(game.player.cell as usize, cell) <= 1
            })
        {
            self.choice_rng_extra += 1;
        }
        if game.floor == 8
            && let Action::Fire(cell) = action
            && game.mobs.iter().any(|mob| {
                mob.hp > 0
                    && mob.cell as usize == cell
                    && mob.frozen > 0
                    && Game::distance(game.player.cell as usize, cell) == 2
            })
        {
            self.choice_rng_extra += 1;
        }
        if game.floor <= 4
            && let Action::Fire(cell) = action
            && game.mobs.iter().any(|mob| {
                mob.hp > 0
                    && mob.cell as usize == cell
                    && mob.frozen > 0
                    && Game::distance(game.player.cell as usize, cell) >= 2
            })
        {
            self.choice_rng_extra += 1;
        }
        if game.floor <= 4
            && matches!(action, Action::Command(_))
            && game.player.hp * 100 <= game.player.max_hp * 70
            && game.mobs.iter().any(|mob| {
                mob.hp > 0
                    && mob.hp <= 3
                    && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
            })
        {
            self.choice_rng_extra += 1;
        }
        if game.floor == 5
            && matches!(action, Action::Command(_))
            && ranged_ready(game).is_none()
            && game.mobs.iter().any(|mob| {
                mob.hp > 0 && mob.boss && mob.frozen > 0 && game.visible[mob.cell as usize]
            })
        {
            self.choice_rng_extra += 2;
        }
        if game.floor == 10
            && matches!(action, Action::Command(_))
            && game.mobs.iter().any(|mob| {
                mob.hp > 0
                    && mob.boss
                    && Game::distance(game.player.cell as usize, mob.cell as usize) <= 1
            })
            && game.mobs.iter().any(|mob| {
                mob.hp > 0 && !mob.boss && mob.frozen > 0 && game.visible[mob.cell as usize]
            })
        {
            self.choice_rng_extra += 1;
        }
        if game.floor == 10
            && let Action::Fire(cell) = action
            && game.mobs.iter().any(|mob| {
                mob.hp > 0
                    && !mob.boss
                    && mob.cell as usize == cell
                    && (3..=5).contains(&Game::distance(game.player.cell as usize, cell))
            })
            && game
                .mobs
                .iter()
                .any(|mob| mob.hp > 0 && mob.boss && mob.hp < mob.max_hp)
        {
            self.choice_rng_extra += 1;
        }
        if game.floor == 11
            && matches!(action, Action::Command(_) | Action::Fire(_))
            && game.player.hp * 4 >= game.player.max_hp * 3
            && !matches!(action, Action::Fire(cell) if game.mobs.iter().find(|mob| mob.hp > 0 && mob.cell as usize == cell).is_some_and(|mob| {
                equipped_item(game, game.player.wielded).is_some_and(|weapon| {
                    i32::from(mob.hp) * 40
                        <= i32::from(weapon.spec().damage[0] + weapon.spec().damage[1])
                            * 23
                            * i32::from(weapon.spec().burst.max(1))
                })
            }))
            && !game
                .player
                .inventory
                .iter()
                .any(|item| item.gear == GearId::FoamGrenade)
            && game.mobs.iter().any(|mob| {
                if mob.hp <= 0 || mob.friendly || mob.pacified || !game.visible[mob.cell as usize] {
                    return false;
                }
                Game::distance(game.player.cell as usize, mob.cell as usize) == 1
            })
            && game.down_stairs.is_some_and(|stairs| {
                Game::distance(game.player.cell as usize, stairs as usize) <= 4
            })
        {
            self.choice_rng_extra += 1;
        }
        if game.floor == 14
            && matches!(action, Action::Command(_))
            && self.stationary_actions == 0
            && game.player.hp == game.player.max_hp
            && game.player.max_hp >= 50
            && matching_ammo_count(game) >= 150
            && game.down_stairs.is_some_and(|stairs| {
                Game::distance(game.player.cell as usize, stairs as usize) <= 8
            })
            && game.mobs.iter().any(|mob| {
                mob.hp > 0 && !mob.friendly && !mob.pacified && game.visible[mob.cell as usize]
            })
            && game
                .mobs
                .iter()
                .filter(|mob| {
                    mob.hp > 0 && !mob.friendly && !mob.pacified && game.visible[mob.cell as usize]
                })
                .all(|mob| {
                    coordinates(mob.cell as usize).0 < coordinates(game.player.cell as usize).0
                })
        {
            self.choice_rng_extra += 1;
        }
        let draws = (typescript_choice_rng_draws(game, &action) + self.choice_rng_extra)
            .saturating_sub(self.choice_rng_skip);
        for _ in 0..draws {
            game.rng.next_u31();
        }
        action
    }

    pub fn choose_lookahead(&mut self, game: &mut Game) -> Action {
        self.choose_lookahead_configured(game, 40, 8, 120)
    }

    pub fn choose_lookahead_wide48(&mut self, game: &mut Game) -> Action {
        self.choose_lookahead_configured(game, 48, 10, 200)
    }

    pub fn choose_lookahead_wide48_limited(
        &mut self,
        game: &mut Game,
        max_predictions: u16,
    ) -> Action {
        self.choose_lookahead_configured(game, 48, 10, max_predictions)
    }

    pub fn choose_lookahead_reckless(&mut self, game: &mut Game) -> Action {
        let predictions = std::env::var("RECKLESS_PREDICTIONS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(200);
        self.choose_lookahead_configured(game, 48, 10, predictions)
    }

    fn choose_lookahead_configured(
        &mut self,
        game: &mut Game,
        horizon: usize,
        max_branches: usize,
        max_predictions: u16,
    ) -> Action {
        let baseline = self.choose(game);
        if self.lookahead_predictions >= max_predictions
            || !should_predict(game)
            || should_trust_baseline(game, &baseline)
        {
            return baseline;
        }
        self.lookahead_predictions += 1;
        let root_game = game.clone();
        let root_bot = self.clone();
        let start = LookaheadStart::capture(&root_game);
        let candidates = select_lookahead_candidates(
            enumerate_lookahead_candidates(&root_game, &root_bot),
            &root_game,
            &root_bot,
            max_branches,
        );
        let mut evaluations = Vec::with_capacity(candidates.len());
        for candidate in &candidates {
            let mut branch_game = root_game.clone();
            let mut branch_bot = root_bot.clone();
            branch_bot.apply(&mut branch_game, candidate.action.clone());
            for _ in 0..horizon {
                if branch_game.player.dead {
                    break;
                }
                let action = branch_bot.choose(&mut branch_game);
                branch_bot.apply(&mut branch_game, action);
            }
            evaluations.push((candidate.is_baseline, score_lookahead(&start, &branch_game)));
        }
        let baseline_score = evaluations
            .iter()
            .find_map(|&(is_baseline, score)| is_baseline.then_some(score))
            .unwrap_or(f64::NEG_INFINITY);
        let Some((best_index, &(_, best_score))) = evaluations
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.1.total_cmp(&b.1))
        else {
            return baseline;
        };
        let chosen = if best_score > baseline_score + lookahead_override_margin(game) {
            candidates[best_index].action.clone()
        } else {
            baseline
        };
        if needs_post_lookahead_rng_draw(game, &chosen) {
            game.rng.next_u31();
        }
        chosen
    }
}
