use crate::{
    HEIGHT, WIDTH, coordinates,
    data::{GearId, GearKind, MOBS, MobId, MobMod, SkillId, gear_flags, mob_flags},
    index,
    model::{Burden, Item},
    world::{Game, PlayerDamageSource, Tile},
};

const HASTE: usize = 0;
const BLIND: usize = 1;
const TELEPATHY: usize = 2;
const HALLUCINATION: usize = 3;
const GRABBED: usize = 5;
const SWALLOWED: usize = 6;

impl Game {
    #[inline]
    pub fn stat(&self, stat: usize) -> i16 {
        let mut value = self.player.stats[stat];
        if let Some(form) = self.player.poly_form
            && stat < 2
        {
            let poly = crate::data::POLY_FORMS[form as usize];
            value = i16::from(if stat == 0 {
                poly.strength
            } else {
                poly.dexterity
            });
        }
        if self.player.has_skill(SkillId::MenInBlack) {
            value += 2;
        }
        value
    }

    #[inline]
    pub fn speed_penalty(&self) -> u8 {
        match self.player.burden {
            Burden::Unencumbered => 0,
            Burden::Burdened => 1,
            Burden::Strained => 2,
            Burden::Overloaded => 4,
        }
    }

    #[inline]
    pub fn blocked(&self, cell: usize) -> bool {
        matches!(self.map[cell], Tile::Wall | Tile::Door)
    }

    #[inline]
    pub fn distance(a: usize, b: usize) -> usize {
        let (ax, ay) = coordinates(a);
        let (bx, by) = coordinates(b);
        ax.abs_diff(bx).max(ay.abs_diff(by))
    }

    pub fn line_clear(&self, from: usize, to: usize, mobs_block: bool) -> bool {
        let (x0, y0) = coordinates(from);
        let (x1, y1) = coordinates(to);
        let dx = x0.abs_diff(x1) as isize;
        let dy = y0.abs_diff(y1) as isize;
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx - dy;
        let mut x = x0 as isize;
        let mut y = y0 as isize;
        while x != x1 as isize || y != y1 as isize {
            let twice = error * 2;
            if twice > -dy {
                error -= dy;
                x += sx;
            }
            if twice < dx {
                error += dx;
                y += sy;
            }
            if x == x1 as isize && y == y1 as isize {
                break;
            }
            let cell = index(x as usize, y as usize);
            if matches!(self.map[cell], Tile::Wall | Tile::Door)
                || mobs_block && self.mob_at(cell).is_some()
            {
                return false;
            }
        }
        true
    }

    pub fn compute_fov(&mut self, radius: usize) {
        self.visible.fill(false);
        let player_cell = self.player.cell as usize;
        let (px, py) = coordinates(player_cell);
        self.mark_visible(player_cell);
        let min_y = py.saturating_sub(radius);
        let max_y = (py + radius).min(HEIGHT - 1);
        let min_x = px.saturating_sub(radius);
        let max_x = (px + radius).min(WIDTH - 1);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let cell = index(x, y);
                if Self::distance(player_cell, cell) <= radius
                    && self.line_clear(player_cell, cell, false)
                {
                    self.mark_visible(cell);
                }
            }
        }
    }

    #[inline]
    fn mark_visible(&mut self, cell: usize) {
        self.visible[cell] = true;
        self.seen[cell] = true;
    }

    pub fn command(&mut self, key: char) {
        let movement = match key {
            'h' => Some((-1, 0)),
            'l' => Some((1, 0)),
            'k' => Some((0, -1)),
            'j' => Some((0, 1)),
            'y' => Some((-1, -1)),
            'u' => Some((1, -1)),
            'b' => Some((-1, 1)),
            'n' => Some((1, 1)),
            _ => None,
        };
        if let Some((dx, dy)) = movement {
            if self.try_move(dx, dy) {
                self.end_turn();
            }
            return;
        }
        match key {
            '.' => self.end_turn(),
            '>' if self.descend() => self.end_turn(),
            '<' if self.ascend() => self.end_turn(),
            'g' if self.pick_up() => self.end_turn(),
            'c' if self.player.has_skill(SkillId::Commands) => {
                for mob in &mut self.mobs {
                    if mob.hp > 0
                        && !mob.friendly
                        && MOBS[mob.kind as usize].tier <= 2
                        && self.visible[mob.cell as usize]
                        && self.rng.chance(0.5)
                    {
                        mob.frozen = self.rng.int_inclusive(2, 5) as i16;
                    }
                }
                self.end_turn();
            }
            'B' if self.player.has_skill(SkillId::Backup) && self.summon_backup() => {
                self.end_turn();
            }
            'P' if self.player.has_skill(SkillId::Shapeshift)
                || self.player.has_skill(SkillId::UniversalRemote) =>
            {
                if self.player.poly_form.is_some() {
                    if self.player.has_skill(SkillId::UniversalRemote) {
                        self.player.poly_form = None;
                    }
                } else {
                    self.polymorph_player();
                }
                self.end_turn();
            }
            _ => {}
        }
    }

    fn summon_backup(&mut self) -> bool {
        if self.player.backup_cooldown > 0 {
            return false;
        }
        let count = self.rng.int_inclusive(1, 2);
        let mut spawned = 0;
        for _ in 0..count {
            let Some(cell) = self.adjacent_free(self.player.cell) else {
                continue;
            };
            let mut agent = self.make_mob(MobId::MibAgent, cell, false);
            agent.hp = 15;
            agent.max_hp = 15;
            agent.damage_override = Some([3, 8]);
            agent.tier_override = Some(2);
            agent.xp = 0;
            agent.friendly = true;
            agent.asleep = false;
            agent.life = 60;
            self.mobs.push(agent);
            spawned += 1;
        }
        self.player.backup_cooldown = 80;
        spawned > 0
    }

    pub fn apply_action_signature(&mut self, signature: &str) -> Result<(), String> {
        self.begin_action();
        let mut fields = signature.split(':');
        let kind = fields.next().ok_or("missing action kind")?;
        match kind {
            "command" => {
                let key = fields
                    .next()
                    .and_then(|value| value.chars().next())
                    .ok_or("missing command key")?;
                self.command(key);
            }
            "fire" => {
                let target = parse_target(fields.next().ok_or("missing fire target")?)?;
                if self.fire_at(target) {
                    self.end_turn();
                }
            }
            "throw" => {
                let name = fields.next().ok_or("missing thrown item")?;
                let target = parse_target(fields.next().ok_or("missing throw target")?)?;
                if self.throw_named(name, target) {
                    self.end_turn();
                }
            }
            "eat" => {
                let name = fields.next().ok_or("missing food")?;
                if self.eat_named(name) {
                    self.end_turn();
                }
            }
            "use" => {
                let name = fields.next().ok_or("missing tool")?;
                if self.use_named(name) {
                    self.end_turn();
                }
            }
            "buy" => {
                self.buy_named(fields.next().ok_or("missing shop item")?);
            }
            "wield" => {
                let name = fields.next().ok_or("missing weapon")?;
                if self.wield_named(name) {
                    self.end_turn();
                }
            }
            "wear" => {
                let name = fields.next().ok_or("missing armor")?;
                if self.wear_named(name) {
                    self.end_turn();
                }
            }
            "learn" => {
                let name = fields.next().ok_or("missing skill")?;
                if let Some(skill) = skill_by_name(name) {
                    self.learn_skill(skill);
                }
            }
            "none" => {}
            _ => return Err(format!("unknown action kind {kind}")),
        }
        Ok(())
    }

    pub fn wield_named(&mut self, name: &str) -> bool {
        let Some(index) = self.inventory_index_named(name) else {
            return false;
        };
        if self.player.inventory[index].gear.kind() != GearKind::Weapon {
            return false;
        }
        if self
            .player
            .wielded
            .and_then(|uid| self.player.inventory.iter().find(|item| item.uid == uid))
            .is_some_and(|item| item.cursed)
        {
            return false;
        }
        if self.player.wielded == Some(self.player.inventory[index].uid) {
            return false;
        }
        self.player.wielded = Some(self.player.inventory[index].uid);
        if self.player.inventory[index].cursed {
            self.player.inventory[index].identified = true;
        }
        true
    }

    pub fn wear_named(&mut self, name: &str) -> bool {
        let Some(index) = self.inventory_index_named(name) else {
            return false;
        };
        if self.player.inventory[index].gear.kind() != GearKind::Armor {
            return false;
        }
        if self
            .player
            .worn
            .and_then(|uid| self.player.inventory.iter().find(|item| item.uid == uid))
            .is_some_and(|item| item.cursed)
        {
            return false;
        }
        if self.player.worn == Some(self.player.inventory[index].uid) {
            return false;
        }
        self.player.worn = Some(self.player.inventory[index].uid);
        if self.player.inventory[index].cursed {
            self.player.inventory[index].identified = true;
        }
        true
    }

    pub fn buy_named(&mut self, name: &str) -> bool {
        let Some(room) = self.shop_room else {
            return false;
        };
        let Some(uid) = self.rooms[room]
            .stock
            .iter()
            .find(|item| item.spec().name == name)
            .map(|item| item.uid)
        else {
            return false;
        };
        self.buy_uid(uid)
    }

    pub fn buy_uid(&mut self, uid: u16) -> bool {
        let Some(room) = self.shop_room else {
            return false;
        };
        let Some(index) = self.rooms[room]
            .stock
            .iter()
            .position(|item| item.uid == uid)
        else {
            return false;
        };
        let price = self.rooms[room].stock[index].price;
        if self.player.credits < price {
            return false;
        }
        self.player.credits -= price;
        let mut item = self.rooms[room].stock.remove(index);
        item.price = 0;
        self.player.add_inventory(item);
        self.player.update_burden();
        if self.player.has_skill(SkillId::Analysis)
            && self.rng.chance(0.3)
            && let Some(last) = self.player.inventory.last_mut()
        {
            last.identified = true;
        }
        true
    }

    pub fn eat_named(&mut self, name: &str) -> bool {
        let Some(index) = self.inventory_index_named(name) else {
            return false;
        };
        match self.player.inventory[index].gear.kind() {
            GearKind::Food => {
                let spec = *self.player.inventory[index].spec();
                let mut healing = i16::from(spec.heal);
                if self.player.has_skill(SkillId::Fieldsurgeon) {
                    healing = if healing == 0 { 4 } else { healing * 2 };
                }
                self.player.nutrition = (self.player.nutrition + spec.nutrition as i16).min(3000);
                self.player.hp = self.player.max_hp.min(self.player.hp + healing);
                self.player.status[HASTE] += i16::from(spec.haste);
                self.consume_inventory(index);
                self.player.update_burden();
                true
            }
            GearKind::Pill => {
                self.swallow_pill(index);
                true
            }
            _ => false,
        }
    }

    fn swallow_pill(&mut self, index: usize) {
        let effect = self.player.inventory[index].pill_effect;
        self.consume_inventory(index);
        use crate::data::PillEffect;
        match effect {
            PillEffect::Heal => {
                self.player.hp = self
                    .player
                    .max_hp
                    .min(self.player.hp + self.rng.dice(3, 8) as i16)
            }
            PillEffect::Poison if !self.player.has_skill(SkillId::Fieldsurgeon) => {
                let damage = self.rng.dice(2, 6) as i16;
                self.player.hp -= damage;
                self.record_player_damage(PlayerDamageSource::Poison, damage);
                self.player.stats[0] = (self.player.stats[0] - 1).max(3);
            }
            PillEffect::Strength => {
                self.player.stats[0] += 1;
                self.player.base_strength += 1;
            }
            PillEffect::Blind => self.player.status[BLIND] += self.rng.int_inclusive(20, 40) as i16,
            PillEffect::Polymorph => self.polymorph_player(),
            PillEffect::Telepathy => {
                self.player.status[TELEPATHY] += self.rng.int_inclusive(40, 80) as i16
            }
            PillEffect::Hallucinate => {
                self.player.status[HALLUCINATION] += self.rng.int_inclusive(30, 60) as i16
            }
            PillEffect::LevelUp => {
                let amount = self.player.xp_next - self.player.xp;
                self.gain_xp(amount as u16);
            }
            PillEffect::Poison => {}
        }
        self.player.known_pills |= 1 << effect as u8;
        self.player.update_burden();
        self.check_death();
    }

    pub fn use_named(&mut self, name: &str) -> bool {
        let Some(index) = self.inventory_index_named(name) else {
            return false;
        };
        let gear = self.player.inventory[index].gear;
        match gear {
            GearId::NeuralyzerCharge => {
                for mob in &mut self.mobs {
                    if mob.hp > 0 && self.visible[mob.cell as usize] && !mob.boss {
                        mob.pacified = true;
                        mob.asleep = true;
                    }
                }
                if !self.player.has_skill(SkillId::MenInBlack) {
                    self.consume_inventory(index);
                }
                true
            }
            GearId::Scanner => {
                let unidentified: Vec<_> = self
                    .player
                    .inventory
                    .iter()
                    .enumerate()
                    .filter_map(|(i, item)| (!item.identified).then_some(i))
                    .collect();
                if unidentified.is_empty() {
                    return false;
                }
                let picked = unidentified[self.rng.pick_index(unidentified.len())];
                self.player.inventory[picked].identified = true;
                self.consume_inventory(index);
                true
            }
            GearId::FoamGrenade => {
                let Some(target) = self.best_freeze_target() else {
                    return false;
                };
                self.throw_inventory(index, target)
            }
            GearId::PocketUniverse => {
                let uid = self.player.inventory[index].uid;
                if let Some(destination) = self.random_floor() {
                    self.player.cell = destination;
                    self.auto_pick_up();
                }
                // Auto-pickup can merge or drop inventory entries, so the
                // pre-teleport positional index is no longer stable. The TS
                // implementation retains the item object across this step;
                // mirror that behavior by locating its stable uid again.
                if let Some(current) = self
                    .player
                    .inventory
                    .iter()
                    .position(|item| item.uid == uid)
                {
                    self.consume_inventory(current);
                }
                true
            }
            GearId::Deneuralyzer => {
                self.player.stats[0] = self.player.base_strength;
                self.player.status.fill(0);
                self.consume_inventory(index);
                true
            }
            _ => false,
        }
    }

    pub fn throw_named(&mut self, name: &str, target: usize) -> bool {
        let Some(index) = self.inventory_index_named(name) else {
            return false;
        };
        self.throw_inventory(index, target)
    }

    fn throw_inventory(&mut self, index: usize, target: usize) -> bool {
        let range = 4 + self.stat(0) as usize / 4;
        if Self::distance(self.player.cell as usize, target) > range {
            return false;
        }
        let mut item = self.take_inventory(
            index,
            self.player.inventory[index].gear.kind() == GearKind::Thrown,
        );
        self.player.update_burden();
        if item.gear == GearId::FoamGrenade {
            for mob in &mut self.mobs {
                if mob.hp > 0 && Self::distance(target, mob.cell as usize) <= 2 {
                    mob.frozen = self.rng.int_inclusive(8, 15) as i16;
                    mob.asleep = false;
                }
            }
            return true;
        }
        if let Some(mob) = self.mob_at(target) {
            self.wake_mob(mob);
            if self.roll_to_hit(
                self.player_attack_bonus(false),
                (MOBS[self.mobs[mob].kind as usize].speed * 3.0).floor() as i32,
            ) {
                let damage =
                    (self.rng.dice(1, 4) + i32::from(item.weight_tenths.max(10)) / 20).max(1);
                let damage = self.damage_vs_mob(mob, f64::from(damage));
                self.mobs[mob].hp -= damage;
                if self.mobs[mob].hp <= 0 {
                    self.kill_mob(mob);
                }
            }
        }
        item.cell = target as u16;
        self.items.push(item);
        true
    }

    fn best_freeze_target(&self) -> Option<usize> {
        let range = 4 + self.stat(0) as usize / 4;
        self.mobs
            .iter()
            .filter(|mob| {
                mob.hp > 0
                    && !mob.friendly
                    && !mob.pacified
                    && self.visible[mob.cell as usize]
                    && mob.frozen <= 0
                    && Self::distance(self.player.cell as usize, mob.cell as usize) <= range
                    && self.line_clear(self.player.cell as usize, mob.cell as usize, true)
            })
            .max_by_key(|mob| (mob.boss, MOBS[mob.kind as usize].tier, mob.hp))
            .map(|mob| mob.cell as usize)
    }

    fn polymorph_player(&mut self) {
        self.player.poly_form = Some(self.rng.pick_index(3) as u8);
        self.player.poly_turns = if self.player.has_skill(SkillId::UniversalRemote) {
            999
        } else {
            self.rng.int_inclusive(40, 90) as i16
        };
    }

    fn inventory_index_named(&self, name: &str) -> Option<usize> {
        self.player
            .inventory
            .iter()
            .position(|item| item.spec().name == name)
    }

    fn consume_inventory(&mut self, index: usize) {
        let stackable = matches!(
            self.player.inventory[index].gear.kind(),
            GearKind::Pill | GearKind::Food | GearKind::Tool | GearKind::Thrown
        );
        if stackable
            && self.player.inventory[index].gear.kind() != GearKind::Ammo
            && self.player.inventory[index].count.max(1) > 1
        {
            let count = self.player.inventory[index].count.max(1);
            self.player.inventory[index].count = count - 1;
            self.player.inventory[index].weight_tenths =
                (u32::from(self.player.inventory[index].weight_tenths) * u32::from(count - 1)
                    / u32::from(count)) as u16;
        } else {
            self.remove_inventory(index);
        }
        self.player.update_burden();
    }

    fn take_inventory(&mut self, index: usize, single: bool) -> Item {
        if single && self.player.inventory[index].count.max(1) > 1 {
            let count = self.player.inventory[index].count.max(1);
            let mut unit = self.player.inventory[index].clone();
            unit.count = 0;
            unit.weight_tenths = self.player.inventory[index].weight_tenths / count;
            self.player.inventory[index].count = count - 1;
            self.player.inventory[index].weight_tenths -= unit.weight_tenths;
            unit
        } else {
            self.remove_inventory(index)
        }
    }

    fn remove_inventory(&mut self, index: usize) -> Item {
        let removed = self.player.inventory.remove(index);
        if self.player.wielded == Some(removed.uid) {
            self.player.wielded = None;
        }
        if self.player.worn == Some(removed.uid) {
            self.player.worn = None;
        }
        removed
    }

    pub fn try_move(&mut self, dx: isize, dy: isize) -> bool {
        if self.player.status[GRABBED] > 0 {
            self.player.status[GRABBED] -= 1;
            return true;
        }
        if self.player.status[SWALLOWED] > 0 {
            self.cut_out_of_bug();
            return true;
        }
        let (x, y) = coordinates(self.player.cell as usize);
        let nx = x as isize + dx;
        let ny = y as isize + dy;
        if nx < 0 || ny < 0 || nx >= WIDTH as isize || ny >= HEIGHT as isize {
            return false;
        }
        let next = index(nx as usize, ny as usize);
        if let Some(mob) = self.mob_at(next) {
            if !self.mobs[mob].friendly {
                return self.melee_mob(mob);
            }
            let old = self.player.cell;
            self.player.cell = next as u16;
            self.mobs[mob].cell = old;
            return true;
        }
        match self.map[next] {
            Tile::Wall => false,
            Tile::Door => {
                self.map[next] = Tile::OpenDoor;
                true
            }
            tile => {
                self.player.cell = next as u16;
                if tile == Tile::Water && self.rng.chance(0.1) && self.player.nutrition > 0 {
                    self.player.nutrition -= 10;
                }
                if tile == Tile::Trap {
                    self.trigger_trap();
                }
                if !self.player.dead && self.items.iter().any(|item| item.cell as usize == next) {
                    self.auto_pick_up();
                }
                true
            }
        }
    }

    pub fn pick_up(&mut self) -> bool {
        if !self.items.iter().any(|item| item.cell == self.player.cell) {
            return false;
        }
        self.take_items_at_player(false);
        true
    }

    pub fn auto_pick_up(&mut self) -> bool {
        self.take_items_at_player(true) > 0
    }

    fn take_items_at_player(&mut self, automatic: bool) -> usize {
        let cell = self.player.cell;
        let mut picked = 0;
        let mut index = 0;
        while index < self.items.len() {
            if self.items[index].cell != cell
                || automatic && !self.should_auto_pick_up(&self.items[index])
            {
                index += 1;
                continue;
            }
            let item = self.items.remove(index);
            let auto_identify = self.player.has_skill(SkillId::Analysis);
            self.player.add_inventory(item);
            if auto_identify && self.rng.chance(0.3) {
                let last = self.player.inventory.len() - 1;
                self.player.inventory[last].identified = true;
            }
            picked += 1;
        }
        if picked > 0 {
            self.player.update_burden();
            self.drop_excess_carry_weight();
        }
        picked
    }

    fn drop_excess_carry_weight(&mut self) {
        let cap = self.player.carry_capacity_tenths();
        if self.player.carry_weight_tenths() * 10 <= cap * 13 {
            return;
        }
        let mut candidates: Vec<_> = self
            .player
            .inventory
            .iter()
            .filter(|item| {
                Some(item.uid) != self.player.wielded
                    && Some(item.uid) != self.player.worn
                    && item.gear.kind() != GearKind::Quest
            })
            .map(|item| (item.uid, self.auto_carry_value(item), item.weight_tenths))
            .collect();
        candidates.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| right.2.cmp(&left.2)));
        for (uid, value, _) in candidates {
            if self.player.carry_weight_tenths() * 100 <= cap * 125 || value >= 220 {
                break;
            }
            if let Some(index) = self
                .player
                .inventory
                .iter()
                .position(|item| item.uid == uid)
            {
                let mut item = self.remove_inventory(index);
                item.cell = self.player.cell;
                self.items.push(item);
            }
        }
        self.player.update_burden();
    }

    fn should_auto_pick_up(&self, item: &Item) -> bool {
        if item.gear.kind() == GearKind::Quest {
            return true;
        }
        let projected = self.player.carry_weight_tenths() + u32::from(item.weight_tenths);
        let cap = self.player.carry_capacity_tenths();
        projected * 100 <= cap * 125
            || item.weight_tenths <= 2
            || self.auto_carry_value(item) >= 220 && projected * 10 <= cap * 16
    }

    pub fn is_auto_pickup_candidate(&self, item: &Item) -> bool {
        self.should_auto_pick_up(item)
    }

    fn auto_carry_value(&self, item: &Item) -> i32 {
        let spec = item.spec();
        match item.gear.kind() {
            GearKind::Quest => 9999,
            _ if item.weight_tenths <= 2 => 400,
            GearKind::Thrown if item.gear == GearId::FoamGrenade => 260,
            GearKind::Tool if item.gear == GearId::NeuralyzerCharge => 260,
            GearKind::Tool if item.gear == GearId::PocketUniverse => 210,
            GearKind::Tool if item.gear == GearId::Deneuralyzer => 170,
            GearKind::Pill => 150,
            GearKind::Food => {
                let units = i32::from(item.count.max(1));
                let value = if spec.heal > 0 {
                    180 + i32::from(spec.heal) * if self.floor >= 10 { 8 } else { 3 }
                } else if spec.haste > 0 {
                    120
                } else {
                    70
                };
                value * units - i32::from(item.weight_tenths) * 3 / 10
            }
            GearKind::Ammo => {
                let matching = self
                    .wielded_item_index()
                    .is_some_and(|index| self.player.inventory[index].spec().ammo == spec.ammo)
                    || self.player.inventory.iter().any(|held| {
                        held.uid != item.uid
                            && held.gear.kind() == GearKind::Weapon
                            && held.spec().ammo == spec.ammo
                    });
                (if matching { 260 } else { 180 }) + i32::from(item.count.min(27)) * 3
                    - i32::from(item.weight_tenths) * 3 / 10
            }
            GearKind::Armor => {
                60 + i32::from(spec.armor) * 28 - i32::from(item.weight_tenths) * 3 / 5
            }
            GearKind::Weapon => {
                45 + self.simple_weapon_score(item) * 10 - i32::from(item.weight_tenths) * 3 / 5
            }
            _ => 30 - i32::from(item.weight_tenths) / 2,
        }
    }

    fn simple_weapon_score(&self, item: &Item) -> i32 {
        let spec = item.spec();
        let average = i32::from(spec.damage[0] + spec.damage[1]) / 2;
        average * i32::from(spec.burst.max(1))
            + if spec.flags & gear_flags::MELEE == 0 {
                i32::from(spec.range) * 3 / 2
            } else {
                4
            }
            + i32::from(item.enchantment) * 2
    }

    pub fn trigger_trap(&mut self) {
        let cell = self.player.cell as usize;
        self.map[cell] = Tile::Floor;
        match self.rng.int_inclusive(1, 4) {
            1 => {
                let damage = self.rng.dice(2, 4) as i16;
                self.player.hp -= damage;
                self.record_player_damage(PlayerDamageSource::Trap, damage);
                if self.player.hp <= 0 {
                    self.player.dead = true;
                }
            }
            2 => self.player.status[BLIND] += self.rng.int_inclusive(5, 15) as i16,
            3 => {
                if let Some(destination) = self.random_floor() {
                    self.player.cell = destination;
                    self.auto_pick_up();
                }
            }
            _ => self.player.status[GRABBED] = self.rng.int_inclusive(2, 4) as i16,
        }
    }

    pub fn descend(&mut self) -> bool {
        if self.map[self.player.cell as usize] != Tile::DownStairs || self.floor >= 15 {
            return false;
        }
        self.generate_floor(self.floor + 1);
        self.player.deepest = self.player.deepest.max(self.floor);
        self.player.cell = self.up_stairs.expect("generated up stairs");
        self.auto_pick_up();
        true
    }

    pub fn ascend(&mut self) -> bool {
        if self.map[self.player.cell as usize] != Tile::UpStairs {
            return false;
        }
        if self
            .player
            .inventory
            .iter()
            .any(|item| item.gear == GearId::Galaxy)
            && matches!(self.floor, 1 | 10)
        {
            self.player.won = true;
            self.player.dead = true;
            return true;
        }
        if self.floor == 1 {
            return false;
        }
        self.generate_floor(self.floor - 1);
        self.player.cell = self
            .down_stairs
            .or(self.up_stairs)
            .expect("generated stairs");
        self.auto_pick_up();
        true
    }
}
