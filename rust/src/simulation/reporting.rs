impl Game {
    pub fn map_hash(&self) -> u64 {
        self.map
            .iter()
            .fold(14_695_981_039_346_656_037_u64, |hash, tile| {
                (hash ^ u64::from(tile.glyph())).wrapping_mul(1_099_511_628_211)
            })
    }

    pub fn score(&self) -> u32 {
        u32::from(self.player.kills) * 10
            + u32::from(self.player.deepest) * 100
            + u32::from(self.player.level) * 50
            + u32::from(self.player.credits)
            + if self.player.won { 5000 } else { 0 }
            + self.player.skills.count_ones() * 75
    }

    pub fn inventory_signature(&self) -> String {
        self.player
            .inventory
            .iter()
            .map(item_signature)
            .collect::<Vec<_>>()
            .join("|")
    }

    pub fn player_signature(&self) -> String {
        let player = &self.player;
        let (x, y) = coordinates(player.cell as usize);
        let skills = (0..SkillId::COUNT)
            .filter_map(skill_from_index)
            .filter(|skill| player.has_skill(*skill))
            .map(skill_name)
            .collect::<Vec<_>>()
            .join(",");
        let wield = player
            .wielded
            .and_then(|uid| player.inventory.iter().position(|item| item.uid == uid))
            .map_or(-1, |index| index as i32);
        let wear = player
            .worn
            .and_then(|uid| player.inventory.iter().position(|item| item.uid == uid))
            .map_or(-1, |index| index as i32);
        let poly = match player.poly_form {
            Some(0) => "bug warrior",
            Some(1) => "orion's cat",
            Some(2) => "skin-suit shambler",
            _ => "-",
        };
        let status = player
            .status
            .iter()
            .map(i16::to_string)
            .collect::<Vec<_>>()
            .join(",");
        [
            x.to_string(),
            y.to_string(),
            player.hp.to_string(),
            player.max_hp.to_string(),
            player.stats[0].to_string(),
            player.stats[1].to_string(),
            player.stats[2].to_string(),
            player.stats[3].to_string(),
            player.stats[4].to_string(),
            player.base_strength.to_string(),
            player.level.to_string(),
            player.xp.to_string(),
            player.xp_next.to_string(),
            player.skill_points.to_string(),
            skills,
            player.credits.to_string(),
            player.nutrition.to_string(),
            burden_name(player.burden).to_owned(),
            poly.to_owned(),
            player.poly_turns.to_string(),
            player.kills.to_string(),
            player.deepest.to_string(),
            bit(player.won),
            bit(player.dead),
            wield.to_string(),
            wear.to_string(),
            status,
        ]
        .join(";")
    }

    pub fn item_signature(&self) -> String {
        self.items
            .iter()
            .map(item_signature)
            .collect::<Vec<_>>()
            .join("|")
    }

    pub fn mob_signature(&self) -> String {
        self.mobs
            .iter()
            .map(|mob| {
                let spec = MOBS[mob.kind as usize];
                let mut name = String::new();
                if mob.sentinel_fragment {
                    name.push_str("sentinel fragment");
                } else {
                    if let Some(modifier) = mob.modifier {
                        name.push_str(modifier_name(modifier));
                        name.push(' ');
                    }
                    let prefix = crate::data::MOB_PREFIXES[mob.prefix as usize].name;
                    if !prefix.is_empty() {
                        name.push_str(prefix);
                        name.push(' ');
                    }
                    name.push_str(spec.name);
                }
                let (x, y) = coordinates(mob.cell as usize);
                let (tx, ty) = mob
                    .target_cell
                    .map(|cell| coordinates(cell as usize))
                    .unwrap_or((usize::MAX, usize::MAX));
                [
                    name,
                    x.to_string(),
                    y.to_string(),
                    mob.hp.to_string(),
                    mob.max_hp.to_string(),
                    mob.damage_multiplier.to_string(),
                    mob.xp.to_string(),
                    bit(mob.boss),
                    bit(mob.asleep),
                    bit(mob.pacified),
                    mob.frozen.to_string(),
                    mob.cooldown.to_string(),
                    bit(mob.revealed),
                    bit(mob.hunting),
                    if tx == usize::MAX {
                        "-1".into()
                    } else {
                        tx.to_string()
                    },
                    if ty == usize::MAX {
                        "-1".into()
                    } else {
                        ty.to_string()
                    },
                    bit(mob.enraged),
                    bit(mob.regrew),
                    mob.modifier.map(modifier_name).unwrap_or("").to_owned(),
                ]
                .join(";")
            })
            .collect::<Vec<_>>()
            .join("|")
    }
}
