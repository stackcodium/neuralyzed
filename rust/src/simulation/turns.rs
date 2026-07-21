impl Game {
    pub fn end_turn(&mut self) {
        if self.player.dead {
            return;
        }
        self.auto_learn_skills();
        self.turns += 1;
        for status in [BLIND, TELEPATHY, HALLUCINATION, HASTE] {
            if self.player.status[status] > 0 {
                self.player.status[status] -= 1;
            }
        }
        if self.player.backup_cooldown > 0 {
            self.player.backup_cooldown -= 1;
        }
        self.hunger_tick();
        self.poly_tick();
        if self.player.status[SWALLOWED] > 0 {
            let damage = self.rng.dice(1, 6) as i16;
            self.player.hp -= damage;
            self.record_player_damage(crate::world::PlayerDamageSource::Swallowed, damage);
            if self.player.hp <= 0 {
                self.player.dead = true;
                return;
            }
            if self.player.status[SWALLOWED] > 0 {
                return;
            }
        }
        let mobs_act = self.player.status[HASTE] == 0 || self.turns.is_multiple_of(2);
        if mobs_act {
            let count = self.mobs.len();
            for mob in 0..count {
                if self.mobs[mob].hp <= 0 {
                    continue;
                }
                if self.mobs[mob].friendly {
                    self.friendly_turn(mob);
                } else {
                    let speed = MOBS[self.mobs[mob].kind as usize].speed;
                    let mut acts = if speed >= 1.2 && self.rng.chance(speed - 1.0) {
                        2
                    } else {
                        1
                    };
                    if self.speed_penalty() > 2 && self.rng.chance(0.3) {
                        acts += 1;
                    }
                    for _ in 0..acts {
                        self.mob_turn(mob);
                        if self.player.dead {
                            return;
                        }
                    }
                }
            }
        }
        self.mobs.retain(|mob| mob.hp > 0);
        self.compute_fov(if self.player.status[BLIND] > 0 { 1 } else { 9 });
        if self.player.hp < self.player.max_hp
            && self.player.nutrition > 300
            && self.turns.is_multiple_of(4)
        {
            let safe = !self
                .mobs
                .iter()
                .any(|mob| mob.hp > 0 && !mob.friendly && self.visible[mob.cell as usize]);
            if safe {
                self.player.hp += 1;
                self.player.nutrition = (self.player.nutrition - 2).max(0);
            }
        }
        for mob in 0..self.mobs.len() {
            if self.mobs[mob].hp > 0
                && !self.mobs[mob].friendly
                && self.visible[self.mobs[mob].cell as usize]
                && !self.mobs[mob].spotted
            {
                self.mobs[mob].spotted = true;
                if self.player.has_skill(SkillId::Quickdraw) {
                    let target = self.mobs[mob].cell;
                    if self
                        .wielded_spec()
                        .is_some_and(|spec| spec.range > 0 && spec.flags & gear_flags::MELEE == 0)
                    {
                        self.fire_at(target as usize);
                    }
                }
            }
        }
    }

    fn hunger_tick(&mut self) {
        let burn = 1 + match self.player.burden {
            Burden::Strained => 1,
            Burden::Overloaded => 2,
            _ => 0,
        };
        if self.turns.is_multiple_of(2) {
            self.player.nutrition = (self.player.nutrition - burn).max(0);
        }
        if self.player.nutrition == 0 && self.turns.is_multiple_of(10) {
            self.player.hp -= 1;
            self.record_player_damage(crate::world::PlayerDamageSource::Starvation, 1);
            if self.player.hp <= 0 {
                self.player.dead = true;
            }
        }
    }

    fn poly_tick(&mut self) {
        if self.player.poly_form.is_some() {
            self.player.poly_turns -= 1;
            if self.player.poly_turns <= 0 {
                self.player.poly_form = None;
            }
        }
    }

    fn mob_turn(&mut self, index: usize) {
        if index >= self.mobs.len() || self.mobs[index].hp <= 0 || self.mobs[index].pacified {
            return;
        }
        if self.mobs[index].frozen > 0 {
            self.mobs[index].frozen -= 1;
            return;
        }
        let spec = MOBS[self.mobs[index].kind as usize];
        if spec.flags & mob_flags::REGEN != 0
            && self.mobs[index].hp < self.mobs[index].max_hp
            && self.turns.is_multiple_of(3)
        {
            self.mobs[index].hp += 1;
        }
        if self.mobs[index].cooldown > 0.0 {
            self.mobs[index].cooldown -= 1.0;
        }
        let player_cell = self.player.cell as usize;
        let mob_cell = self.mobs[index].cell as usize;
        let player_distance = Self::distance(mob_cell, player_cell);
        let can_see = player_distance <= 10 && self.line_clear(mob_cell, player_cell, false);
        if spec.flags & mob_flags::DISGUISED != 0 && !self.mobs[index].revealed {
            if player_distance <= 2 {
                self.mobs[index].revealed = true;
                self.wake_mob(index);
            } else {
                return;
            }
        }
        if self.mobs[index].asleep {
            if !self.player.has_skill(SkillId::Ghostwalk)
                && can_see
                && self.rng.chance(0.3 + (10 - player_distance) as f64 * 0.07)
            {
                self.wake_mob(index);
            }
            return;
        }
        if !can_see && !self.mobs[index].hunting {
            return;
        }
        if can_see {
            self.mobs[index].hunting = true;
            self.mobs[index].target_cell = Some(self.player.cell);
        }
        if self.mobs[index].boss && self.boss_act(index, player_distance, can_see) {
            return;
        }
        if !self.mobs[index].boss
            && spec.tier >= 2
            && f64::from(self.mobs[index].hp) < f64::from(self.mobs[index].max_hp) * 0.25
            && !self.mobs[index].desperate
        {
            self.mobs[index].desperate = true;
            self.mobs[index].damage_multiplier *= 0.9;
        }
        if spec.ranged > 0
            && can_see
            && player_distance <= usize::from(spec.ranged)
            && player_distance > 1
            && self.rng.chance(0.7)
        {
            self.mob_shoot(index);
            return;
        }
        if player_distance == 1 {
            self.mob_melee(index);
            return;
        }
        let target = self.mobs[index].target_cell.unwrap_or(self.player.cell) as usize;
        let (mx, my) = coordinates(mob_cell);
        let (tx, ty) = coordinates(target);
        let sx = sign(tx as isize - mx as isize);
        let sy = sign(ty as isize - my as isize);
        let options = [
            (sx, sy),
            (sx, 0),
            (0, sy),
            (
                self.rng.int_inclusive(-1, 1) as isize,
                self.rng.int_inclusive(-1, 1) as isize,
            ),
        ];
        for (ox, oy) in options {
            if ox == 0 && oy == 0 {
                continue;
            }
            let nx = mx as isize + ox;
            let ny = my as isize + oy;
            if nx < 0 || ny < 0 || nx >= WIDTH as isize || ny >= HEIGHT as isize {
                continue;
            }
            let next = crate::index(nx as usize, ny as usize);
            if self.blocked(next) || self.mob_at(next).is_some() || next == player_cell {
                continue;
            }
            if spec.flags & mob_flags::PHASE != 0 && self.rng.chance(0.3) {
                let px = mx as isize + sx;
                let py = my as isize + sy;
                if px >= 0 && py >= 0 && px < WIDTH as isize && py < HEIGHT as isize {
                    let phase = crate::index(px as usize, py as usize);
                    if phase != player_cell && self.mob_at(phase).is_none() {
                        self.mobs[index].cell = phase as u16;
                        return;
                    }
                }
            }
            self.mobs[index].cell = next as u16;
            return;
        }
    }

    fn wake_mob(&mut self, index: usize) {
        self.mobs[index].asleep = false;
        if self.mobs[index].modifier == Some(MobMod::Shrieking) {
            for mob in &mut self.mobs {
                if mob.hp > 0 {
                    mob.asleep = false;
                }
            }
        }
    }

    fn player_attack_bonus(&self, melee: bool) -> i32 {
        if melee {
            i32::from((self.stat(0) - 10) / 2)
                + if self.player.has_skill(SkillId::Brawling) {
                    2
                } else {
                    0
                }
        } else {
            i32::from((self.stat(1) - 10) / 2)
        }
    }

    fn player_armor(&self) -> i32 {
        let armor = self
            .player
            .worn
            .and_then(|uid| self.player.inventory.iter().find(|item| item.uid == uid))
            .map_or(0, |item| i32::from(item.spec().armor));
        armor + i32::from((self.stat(1) - 10) / 2)
    }

    fn roll_to_hit(&mut self, attack: i32, defense: i32) -> bool {
        self.rng.int_inclusive(1, 20) + attack >= 10 + defense
    }

    fn mob_melee(&mut self, index: usize) {
        if self.player.has_skill(SkillId::Acrobatics) && self.rng.chance(0.25) {
            if self.player.has_skill(SkillId::Bullettime) {
                self.melee_mob(index);
            }
            return;
        }
        let spec = MOBS[self.mobs[index].kind as usize];
        if !self.roll_to_hit(i32::from(spec.tier) * 2, self.player_armor()) {
            return;
        }
        let damage = ((f64::from(
            self.rng
                .int_inclusive(i32::from(spec.damage[0]), i32::from(spec.damage[1])),
        ) * self.mobs[index].damage_multiplier)
            .ceil() as i32
            - self.player_armor() / 2)
            .max(1);
        self.player.hp -= damage as i16;
        self.record_player_damage(
            crate::world::PlayerDamageSource::Mob(self.mobs[index].kind),
            damage as i16,
        );
        if self.mobs[index].modifier == Some(MobMod::Venomous) && self.rng.chance(0.3) {
            self.player.stats[0] = self.player.stats[0].max(3) - 1;
        }
        if self.mobs[index].modifier == Some(MobMod::SporeLaden) && self.rng.chance(0.25) {
            self.player.status[BLIND] += self.rng.int_inclusive(3, 8) as i16;
        }
        if spec.flags & mob_flags::GRAB != 0 {
            self.player.status[GRABBED] = 2;
        }
        if spec.flags & mob_flags::EATS != 0 && self.rng.chance(0.12) && self.player.hp > 0 {
            self.player.status[SWALLOWED] = 2;
        }
        self.check_death();
    }

    fn mob_shoot(&mut self, index: usize) {
        if self.player.has_skill(SkillId::Acrobatics) && self.rng.chance(0.25) {
            return;
        }
        let spec = MOBS[self.mobs[index].kind as usize];
        if !self.roll_to_hit(i32::from(spec.tier) * 2, self.player_armor()) {
            return;
        }
        let damage = ((f64::from(
            self.rng
                .int_inclusive(i32::from(spec.damage[0]), i32::from(spec.damage[1])),
        ) * self.mobs[index].damage_multiplier
            * 0.8)
            .ceil() as i32
            - self.player_armor() / 2)
            .max(1);
        self.player.hp -= damage as i16;
        self.record_player_damage(
            crate::world::PlayerDamageSource::Mob(self.mobs[index].kind),
            damage as i16,
        );
        self.check_death();
    }

    fn check_death(&mut self) {
        if self.player.hp <= 0 {
            if self.player.has_skill(SkillId::GalaxyDefender) && !self.player.used_galaxy_defender {
                self.player.used_galaxy_defender = true;
                self.player.hp = 1;
            } else {
                self.player.dead = true;
            }
        }
    }

    fn friendly_turn(&mut self, agent_index: usize) {
        self.mobs[agent_index].life -= 1;
        if self.mobs[agent_index].life <= 0 {
            self.mobs[agent_index].hp = 0;
            return;
        }
        let origin = self.mobs[agent_index].cell as usize;
        let Some(target) = self
            .mobs
            .iter()
            .enumerate()
            .filter(|(_, mob)| mob.hp > 0 && !mob.friendly && !mob.pacified)
            .min_by_key(|(_, mob)| Self::distance(origin, mob.cell as usize))
            .map(|(target, _)| target)
        else {
            return;
        };
        let distance = Self::distance(origin, self.mobs[target].cell as usize);
        if distance == 1 {
            let defense = (MOBS[self.mobs[target].kind as usize].speed * 3.0).floor() as i32;
            if self.roll_to_hit(4, defense) {
                self.mobs[target].hp -= self.rng.int_inclusive(3, 8) as i16;
                if self.mobs[target].hp <= 0 {
                    let xp = ((f64::from(self.mobs[target].xp) / 2.0).ceil() as u16).max(1);
                    self.gain_xp(xp);
                }
            }
            return;
        }
        let (x, y) = coordinates(origin);
        let (tx, ty) = coordinates(self.mobs[target].cell as usize);
        let nx = (x as isize + sign(tx as isize - x as isize)) as usize;
        let ny = (y as isize + sign(ty as isize - y as isize)) as usize;
        let next = index(nx, ny);
        if !self.blocked(next) && self.mob_at(next).is_none() && next != self.player.cell as usize {
            self.mobs[agent_index].cell = next as u16;
        }
    }

    fn boss_act(&mut self, index: usize, distance: usize, can_see: bool) -> bool {
        let kind = self.mobs[index].kind;
        if f64::from(self.mobs[index].hp) < f64::from(self.mobs[index].max_hp) * 0.5
            && !self.mobs[index].enraged
        {
            self.mobs[index].enraged = true;
            self.mobs[index].damage_multiplier *= 1.3;
        }
        if kind == MobId::JeebsPrime
            && f64::from(self.mobs[index].hp) < f64::from(self.mobs[index].max_hp) * 0.25
            && !self.mobs[index].regrew
        {
            self.mobs[index].regrew = true;
            self.mobs[index].hp = (f64::from(self.mobs[index].max_hp) * 0.35).ceil() as i16;
            return true;
        }
        if let Some(summoned_kind) = MOBS[kind as usize].summon
            && self.mobs[index].cooldown <= 0.0
            && can_see
        {
            let active = self
                .mobs
                .iter()
                .filter(|mob| mob.hp > 0 && !mob.boss && mob.kind == summoned_kind)
                .count();
            if kind != MobId::Serleena || active < 2 {
                self.mobs[index].cooldown = if kind == MobId::Serleena { 20.0 } else { 12.0 };
                if let Some(cell) = self.adjacent_free(self.mobs[index].cell) {
                    let mut summoned = self.make_mob(summoned_kind, cell, false);
                    summoned.asleep = false;
                    self.mobs.push(summoned);
                    return true;
                }
            }
        }
        if kind == MobId::Edgar && distance > 1 && distance <= 5 && can_see && self.rng.chance(0.4)
        {
            if !(self.player.has_skill(SkillId::Acrobatics) && self.rng.chance(0.25)) {
                let damage = (self.rng.dice(2, 6) - self.player_armor() / 2).max(1) as i16;
                self.player.hp -= damage;
                self.record_player_damage(
                    crate::world::PlayerDamageSource::Mob(self.mobs[index].kind),
                    damage,
                );
                self.check_death();
            }
            return true;
        }
        false
    }
}
