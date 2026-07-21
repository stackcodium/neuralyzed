impl Game {
    fn wielded_item_index(&self) -> Option<usize> {
        let uid = self.player.wielded?;
        self.player
            .inventory
            .iter()
            .position(|item| item.uid == uid)
    }

    fn wielded_spec(&self) -> Option<&'static crate::data::GearSpec> {
        self.wielded_item_index()
            .map(|index| self.player.inventory[index].spec())
    }

    fn weapon_damage(&mut self, item: Option<usize>, melee: bool) -> i32 {
        let Some(index) = item else {
            return (self.rng.dice(1, 3)
                + if melee {
                    i32::from((self.stat(0) - 10) / 2)
                } else {
                    0
                })
            .max(1);
        };
        let spec = self.player.inventory[index].spec();
        let mut damage = self
            .rng
            .int_inclusive(i32::from(spec.damage[0]), i32::from(spec.damage[1]))
            + i32::from(self.player.inventory[index].enchantment);
        if melee {
            damage += i32::from((self.stat(0) - 10) / 2)
                + if self.player.has_skill(SkillId::Brawling) {
                    2
                } else {
                    0
                };
        }
        damage.max(1)
    }

    fn damage_vs_mob(&self, index: usize, base: f64) -> i16 {
        let spec = MOBS[self.mobs[index].kind as usize];
        let mut damage = base;
        if spec.flags & mob_flags::BUG != 0 && self.player.has_skill(SkillId::Exoslayer) {
            damage += 4.0;
        }
        if spec.flags & mob_flags::BUG != 0
            && self
                .wielded_spec()
                .is_some_and(|weapon| weapon.flags & gear_flags::BUG_BAIT != 0)
        {
            damage += 6.0;
        }
        (damage - f64::from(spec.armor)).max(1.0) as i16
    }

    pub fn melee_mob(&mut self, index: usize) -> bool {
        self.wake_mob(index);
        self.mobs[index].pacified = false;
        let critical = self.rng.chance(0.08);
        let defense = (MOBS[self.mobs[index].kind as usize].speed * 3.0).floor() as i32;
        if !self.roll_to_hit(self.player_attack_bonus(true), defense) {
            return true;
        }
        let weapon = if self.player.poly_form.is_some() {
            None
        } else {
            self.wielded_item_index()
                .filter(|&i| self.player.inventory[i].spec().flags & gear_flags::MELEE != 0)
        };
        let damage = self.weapon_damage(weapon, true) * if critical { 2 } else { 1 };
        let damage = self.damage_vs_mob(index, f64::from(damage));
        self.mobs[index].hp -= damage;
        if self.mobs[index].hp <= 0 {
            self.kill_mob(index);
        }
        true
    }

    pub fn fire_at(&mut self, target: usize) -> bool {
        let Some(weapon_index) = self.wielded_item_index() else {
            return false;
        };
        let weapon = *self.player.inventory[weapon_index].spec();
        if weapon.range == 0 || weapon.flags & gear_flags::MELEE != 0 {
            return false;
        }
        let Some(ammo_index) =
            self.player.inventory.iter().position(|item| {
                item.gear.kind() == GearKind::Ammo && item.spec().ammo == weapon.ammo
            })
        else {
            return false;
        };
        if self.player.inventory[ammo_index].count == 0 {
            return false;
        }
        let range = usize::from(weapon.range)
            + if self.player.has_skill(SkillId::Deadeye) {
                2
            } else {
                0
            };
        if Self::distance(self.player.cell as usize, target) > range {
            return false;
        }
        if !self.line_clear(self.player.cell as usize, target, true)
            && self.mob_at(target).is_none()
        {
            return false;
        }
        for _ in 0..weapon.burst.max(1) {
            if self.player.inventory[ammo_index].count == 0 {
                break;
            }
            self.player.inventory[ammo_index].count -= 1;
            let Some(mob) = self.mob_at(target) else {
                continue;
            };
            self.wake_mob(mob);
            if !self.roll_to_hit(
                self.player_attack_bonus(false)
                    + i32::from(self.player.inventory[weapon_index].enchantment),
                (MOBS[self.mobs[mob].kind as usize].speed * 3.0).floor() as i32,
            ) {
                continue;
            }
            let critical = self.rng.chance(if self.player.has_skill(SkillId::Deadeye) {
                0.15
            } else {
                0.08
            });
            let bug_bonus = if weapon.flags & gear_flags::BUG_BAIT != 0
                && MOBS[self.mobs[mob].kind as usize].flags & mob_flags::BUG != 0
            {
                1.9
            } else {
                1.0
            };
            let multiplier = bug_bonus
                * if critical {
                    if self.player.has_skill(SkillId::Deadeye) {
                        3.0
                    } else {
                        2.0
                    }
                } else {
                    1.0
                };
            let raw = f64::from(self.weapon_damage(Some(weapon_index), false)) * multiplier;
            let damage = self.damage_vs_mob(mob, raw);
            self.mobs[mob].hp -= damage;
            if weapon.flags & gear_flags::KICK != 0 && self.rng.chance(0.5) {
                self.apply_recoil(target);
            }
            if self.mobs[mob].hp > 0
                && !self.mobs[mob].boss
                && Self::distance(self.player.cell as usize, self.mobs[mob].cell as usize) <= 1
            {
                self.knock_mob_away(mob);
            }
            if weapon.flags & gear_flags::POLYMORPH != 0
                && !self.mobs[mob].boss
                && self.rng.chance(0.25)
                && self.mobs[mob].hp > 0
            {
                let forms = [
                    MobId::SewerSquid,
                    MobId::WormGuy,
                    MobId::RefugeeGrub,
                    MobId::JeebsClone,
                ];
                let kind = forms[self.rng.pick_index(forms.len())];
                let cell = self.mobs[mob].cell;
                let remaining_hp = self.mobs[mob].hp;
                let mut replacement = self.make_mob(kind, cell, false);
                replacement.hp = replacement.hp.min(remaining_hp);
                self.mobs[mob] = replacement;
                continue;
            }
            if self.mobs[mob].hp <= 0 {
                self.kill_mob(mob);
            }
        }
        if self
            .player
            .inventory
            .get(ammo_index)
            .is_some_and(|ammo| ammo.count == 0)
        {
            self.player.inventory.remove(ammo_index);
        }
        true
    }

    fn apply_recoil(&mut self, target: usize) {
        let (px, py) = coordinates(self.player.cell as usize);
        let (tx, ty) = coordinates(target);
        let nx = px as isize - sign(tx as isize - px as isize);
        let ny = py as isize - sign(ty as isize - py as isize);
        if nx >= 0 && ny >= 0 && nx < WIDTH as isize && ny < HEIGHT as isize {
            let next = index(nx as usize, ny as usize);
            if !self.blocked(next) && self.mob_at(next).is_none() {
                self.player.cell = next as u16;
            }
        }
    }

    fn knock_mob_away(&mut self, mob: usize) {
        let (px, py) = coordinates(self.player.cell as usize);
        let (mx, my) = coordinates(self.mobs[mob].cell as usize);
        let nx = mx as isize + sign(mx as isize - px as isize);
        let ny = my as isize + sign(my as isize - py as isize);
        if nx < 0 || ny < 0 || nx >= WIDTH as isize || ny >= HEIGHT as isize {
            return;
        }
        let next = index(nx as usize, ny as usize);
        if !self.blocked(next) && self.mob_at(next).is_none() {
            self.mobs[mob].cell = next as u16;
        }
    }

    fn kill_mob(&mut self, index: usize) {
        if self.mobs[index].hp > 0 {
            self.mobs[index].hp = 0;
        }
        self.player.kills += 1;
        let xp = self.mobs[index].xp;
        self.gain_xp(xp);
        if MOBS[self.mobs[index].kind as usize].flags & mob_flags::SPLITS != 0
            && !self.mobs[index].did_split
        {
            for _ in 0..2 {
                if let Some(cell) = self.adjacent_free(self.mobs[index].cell) {
                    let mut fragment = self.make_mob(MobId::TwinSentinels, cell, false);
                    fragment.hp = (f64::from(self.mobs[index].max_hp) / 3.0).ceil() as i16;
                    fragment.max_hp = fragment.hp;
                    fragment.damage_override = Some([1, 3]);
                    fragment.tier_override = Some(1);
                    fragment.xp = ((f64::from(fragment.xp) / 2.0).ceil() as u16).max(4);
                    fragment.did_split = true;
                    fragment.sentinel_fragment = true;
                    self.mobs.push(fragment);
                }
            }
        }
        if self.rng.chance(0.25) {
            let tier = (MOBS[self.mobs[index].kind as usize].tier
                + if self.mobs[index].boss { 1 } else { 0 })
            .min(4);
            let gear = self.random_item_for_tier(tier);
            let item = self.make_item(gear, self.mobs[index].cell);
            self.items.push(item);
        }
        if self.rng.chance(0.4) {
            self.player.credits += (self.rng.int_inclusive(3, 12)
                * i32::from(MOBS[self.mobs[index].kind as usize].tier))
                as u16;
        }
        if self.mobs[index].boss {
            self.boss_defeated(index);
        }
    }

    pub fn gain_xp(&mut self, amount: u16) {
        self.player.xp += (f64::from(amount) * self.player.xp_multiplier).ceil() as u32;
        while self.player.xp >= self.player.xp_next {
            self.player.xp -= self.player.xp_next;
            self.player.level += 1;
            self.player.xp_next = (f64::from(self.player.xp_next) * 1.6).ceil() as u32;
            self.player.skill_points += 1;
            let hp_gain = self.rng.dice(1, 6) + i32::from((self.stat(0) - 10) / 3);
            self.player.max_hp += hp_gain.max(1) as i16;
            self.player.hp = self
                .player
                .max_hp
                .min(self.player.hp + (f64::from(self.player.max_hp) * 0.6).ceil() as i16);
            if self.player.level.is_multiple_of(3) {
                let stat = self.rng.pick_index(5);
                self.player.stats[stat] += 1;
                if stat == 0 {
                    self.player.base_strength += 1;
                }
            }
        }
        self.auto_learn_skills();
    }

    fn available_skill(&self, skill: SkillId) -> bool {
        if skill as usize >= crate::data::SKILLS.len() || self.player.has_skill(skill) {
            return false;
        }
        let spec = crate::data::SKILLS[skill as usize];
        if let Some(stat) = spec.stat
            && self.stat(stat as usize) < i16::from(spec.minimum)
        {
            return false;
        }
        self.player.skills & spec.requirements == spec.requirements
    }

    pub fn learn_skill(&mut self, skill: SkillId) -> bool {
        if !self.available_skill(skill) {
            return false;
        }
        let cost = crate::data::SKILLS[skill as usize].cost;
        if self.player.skill_points < cost {
            return false;
        }
        self.player.skill_points -= cost;
        self.player.skills |= skill.bit();
        if skill == SkillId::MenInBlack {
            self.player.max_hp += 10;
            self.player.hp += 10;
        }
        if skill == SkillId::Hauling {
            self.player.update_burden();
        }
        true
    }

    fn auto_learn_skills(&mut self) {
        const COMBO: [SkillId; 11] = [
            SkillId::Fieldsurgeon,
            SkillId::GalaxyDefender,
            SkillId::Deadeye,
            SkillId::Backup,
            SkillId::Exoslayer,
            SkillId::Bullettime,
            SkillId::Ghostwalk,
            SkillId::Blackmarket,
            SkillId::Interrogate,
            SkillId::MenInBlack,
            SkillId::UniversalRemote,
        ];
        const FOUNDATION: [SkillId; 10] = [
            SkillId::Analysis,
            SkillId::Hauling,
            SkillId::Intuition,
            SkillId::Detection,
            SkillId::Quickdraw,
            SkillId::Brawling,
            SkillId::Acrobatics,
            SkillId::Xenology,
            SkillId::Commands,
            SkillId::Bargaining,
        ];
        loop {
            let skill = COMBO.into_iter().chain(FOUNDATION).find(|&skill| {
                self.available_skill(skill)
                    && self.player.skill_points >= crate::data::SKILLS[skill as usize].cost
            });
            let Some(skill) = skill else {
                break;
            };
            if !self.learn_skill(skill) {
                break;
            }
        }
    }

    fn boss_defeated(&mut self, index: usize) {
        self.player.skill_points += 2;
        self.auto_learn_skills();
        let drop_cell = self
            .adjacent_free(self.mobs[index].cell)
            .unwrap_or(self.mobs[index].cell);
        for _ in 0..3 {
            let gear = self.random_item_for_tier(4);
            let item = self.make_item(gear, drop_cell);
            self.items.push(item);
        }
        if self.mobs[index].kind == MobId::JeebsPrime {
            for gear in [GearId::RoyalJelly, GearId::FoamGrenade, GearId::Battery] {
                let item = self.make_item(gear, drop_cell);
                self.items.push(item);
            }
        }
        self.player.credits += self.rng.int_inclusive(100, 250) as u16;
        if self.mobs[index].kind == MobId::Edgar {
            self.player.won = true;
            self.player.dead = true;
        }
    }

    fn adjacent_free(&self, center: u16) -> Option<u16> {
        let (x, y) = coordinates(center as usize);
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
                let cell = index(nx as usize, ny as usize);
                if !self.blocked(cell)
                    && self.mob_at(cell).is_none()
                    && cell != self.player.cell as usize
                {
                    return Some(cell as u16);
                }
            }
        }
        None
    }

    fn cut_out_of_bug(&mut self) {
        let damage = self.weapon_damage(self.wielded_item_index(), true) * 3;
        if let Some(edgar) = self
            .mobs
            .iter()
            .position(|mob| mob.kind == MobId::Edgar && mob.boss && mob.hp > 0)
        {
            self.mobs[edgar].hp -= damage as i16;
            if self.mobs[edgar].hp <= 0 {
                self.kill_mob(edgar);
                self.player.status[SWALLOWED] = 0;
                return;
            }
        }
        self.player.status[SWALLOWED] -= 1;
    }
}
