use crate::{
    data::{Ammo, CLASSES, ClassId, GEAR, GearId, GearKind, PillEffect, SkillId, gear_flags},
    rng::Rng,
};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Burden {
    #[default]
    Unencumbered,
    Burdened,
    Strained,
    Overloaded,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub uid: u16,
    pub gear: GearId,
    pub cell: u16,
    pub identified: bool,
    pub count: u16,
    pub enchantment: i8,
    pub cursed: bool,
    pub appearance: u8,
    pub pill_effect: PillEffect,
    pub price: u16,
    /// Runtime total weight. TypeScript mutates stack weight independently of count.
    pub weight_tenths: u16,
}

impl Item {
    #[inline(always)]
    pub fn spec(&self) -> &'static crate::data::GearSpec {
        &GEAR[self.gear as usize]
    }
}

#[derive(Clone, Debug)]
pub struct Player {
    pub class: ClassId,
    pub agent_letter: u8,
    pub cell: u16,
    pub hp: i16,
    pub max_hp: i16,
    pub stats: [i16; 5],
    pub base_strength: i16,
    pub level: u8,
    pub xp: u32,
    pub xp_next: u32,
    pub skill_points: u8,
    pub skills: u32,
    pub xp_multiplier: f64,
    pub shop_multiplier: f64,
    pub inventory: Vec<Item>,
    pub wielded: Option<u16>,
    pub worn: Option<u16>,
    pub credits: u16,
    pub nutrition: i16,
    pub burden: Burden,
    /// haste, blind, telepathy, hallucination, confused, grabbed, swallowed
    pub status: [i16; 7],
    pub poly_form: Option<u8>,
    pub poly_turns: i16,
    pub kills: u16,
    pub deepest: u8,
    pub won: bool,
    pub dead: bool,
    pub used_galaxy_defender: bool,
    /// Effect -> shuffled appearance and known-effect bitset.
    pub pill_appearance: [u8; 8],
    pub known_pills: u8,
    pub backup_cooldown: i16,
    next_item_uid: u16,
}

impl Player {
    pub fn new(class: ClassId, rng: &mut Rng) -> Self {
        let spec = &CLASSES[class as usize];
        let agent_letter = *b"JKLMXZDWV"
            .get(rng.pick_index(9))
            .expect("fixed letter table");
        let mut player = Self {
            class,
            agent_letter,
            cell: 0,
            hp: spec.hp,
            max_hp: spec.hp,
            stats: spec.stats.map(i16::from),
            base_strength: i16::from(spec.stats[0]),
            level: 1,
            xp: 0,
            xp_next: 20,
            skill_points: 0,
            skills: spec.initial_skill.map_or(0, SkillId::bit),
            xp_multiplier: spec.xp_multiplier,
            shop_multiplier: spec.shop_multiplier,
            inventory: Vec::with_capacity(16),
            wielded: None,
            worn: None,
            credits: rng.int_inclusive(30, 80) as u16,
            nutrition: 2400,
            burden: Burden::Unencumbered,
            status: [0; 7],
            poly_form: None,
            poly_turns: 0,
            kills: 0,
            deepest: 1,
            won: false,
            dead: false,
            used_galaxy_defender: false,
            pill_appearance: [0; 8],
            known_pills: 0,
            backup_cooldown: 0,
            next_item_uid: 0,
        };
        for &gear in spec.gear {
            let mut item = player.make_item(gear, 0, rng);
            item.identified = true;
            item.enchantment = item.enchantment.max(0);
            item.cursed = false;
            let kind = gear.kind();
            let uid = player.add_inventory(item);
            if kind == GearKind::Weapon && player.wielded.is_none() {
                player.wielded = Some(uid);
            }
            if kind == GearKind::Armor && player.worn.is_none() {
                player.worn = Some(uid);
            }
        }
        let mut descriptions = [0_u8, 1, 2, 3, 4, 5, 6, 7];
        for i in (1..descriptions.len()).rev() {
            let j = rng.int_inclusive(0, i as i32) as usize;
            descriptions.swap(i, j);
        }
        player.pill_appearance = descriptions;
        player.update_burden();
        player
    }

    pub fn make_item(&mut self, gear: GearId, cell: u16, rng: &mut Rng) -> Item {
        let spec = &GEAR[gear as usize];
        let uid = self.next_item_uid;
        self.next_item_uid = self.next_item_uid.wrapping_add(1);
        let mut item = Item {
            uid,
            gear,
            cell,
            identified: spec.flags & gear_flags::UNIDENTIFIED == 0 && gear.kind() != GearKind::Pill,
            count: 0,
            enchantment: 0,
            cursed: false,
            appearance: 0,
            pill_effect: PillEffect::Heal,
            price: 0,
            weight_tenths: spec.weight_tenths,
        };
        if gear.kind() == GearKind::Pill {
            item.pill_effect = match rng.pick_index(8) {
                0 => PillEffect::Heal,
                1 => PillEffect::Poison,
                2 => PillEffect::Strength,
                3 => PillEffect::Blind,
                4 => PillEffect::Polymorph,
                5 => PillEffect::Telepathy,
                6 => PillEffect::Hallucinate,
                _ => PillEffect::LevelUp,
            };
        }
        if spec.quantity[1] != 0 {
            item.count =
                rng.int_inclusive(i32::from(spec.quantity[0]), i32::from(spec.quantity[1])) as u16;
        }
        if spec.damage != [0, 0] {
            item.enchantment = if rng.chance(0.15) {
                rng.int_inclusive(1, 3) as i8
            } else if rng.chance(0.1) {
                -(rng.int_inclusive(1, 2) as i8)
            } else {
                0
            };
            item.cursed = item.enchantment < 0 || rng.chance(0.05);
            item.identified = false;
            item.appearance = rng.pick_index(10) as u8;
        }
        item
    }

    pub fn add_inventory(&mut self, item: Item) -> u16 {
        if is_stackable(item.gear)
            && let Some(existing) = self
                .inventory
                .iter_mut()
                .find(|held| stack_matches(held, &item))
        {
            existing.count = stack_count(existing).saturating_add(stack_count(&item));
            existing.weight_tenths = existing.weight_tenths.saturating_add(item.weight_tenths);
            existing.identified |= item.identified;
            return existing.uid;
        }
        let uid = item.uid;
        self.inventory.push(item);
        uid
    }

    pub fn has_skill(&self, skill: SkillId) -> bool {
        self.skills & skill.bit() != 0
    }

    pub fn carry_weight_tenths(&self) -> u32 {
        self.inventory
            .iter()
            .map(|item| u32::from(item.weight_tenths))
            .sum()
    }

    pub fn carry_capacity_tenths(&self) -> u32 {
        let mut capacity = 25 + i32::from(self.stats[0]) * 3;
        if self.has_skill(SkillId::Hauling) {
            capacity = (f64::from(capacity) * 1.4).ceil() as i32;
        }
        capacity.max(0) as u32 * 10
    }

    pub fn update_burden(&mut self) {
        let weight = self.carry_weight_tenths();
        let cap = self.carry_capacity_tenths();
        self.burden = if weight <= cap {
            Burden::Unencumbered
        } else if weight * 10 <= cap * 13 {
            Burden::Burdened
        } else if weight * 10 <= cap * 16 {
            Burden::Strained
        } else {
            Burden::Overloaded
        };
    }

    pub fn ammo_count(&self, ammo: Ammo) -> u32 {
        self.inventory
            .iter()
            .filter(|item| item.spec().ammo == ammo && item.gear.kind() == GearKind::Ammo)
            .map(|item| u32::from(item.count.max(1)))
            .sum()
    }
}

fn is_stackable(gear: GearId) -> bool {
    matches!(
        gear.kind(),
        GearKind::Ammo | GearKind::Pill | GearKind::Food | GearKind::Tool | GearKind::Thrown
    )
}

fn stack_count(item: &Item) -> u16 {
    item.count.max(1)
}

fn stack_matches(left: &Item, right: &Item) -> bool {
    if left.gear.kind() != right.gear.kind() {
        return false;
    }
    match left.gear.kind() {
        GearKind::Ammo => left.spec().ammo == right.spec().ammo,
        GearKind::Pill => left.pill_effect == right.pill_effect,
        GearKind::Food | GearKind::Tool | GearKind::Thrown => left.gear == right.gear,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::Player;
    use crate::{
        data::{ClassId, GearId},
        rng::Rng,
    };

    #[test]
    fn starting_rations_stack_and_equipment_uses_stable_ids() {
        let mut rng = Rng::new(1_701_033);
        let player = Player::new(ClassId::Agent, &mut rng);
        let ration = player
            .inventory
            .iter()
            .find(|item| item.gear == GearId::Ration)
            .unwrap();
        assert_eq!(ration.count, 2);
        assert_eq!(ration.weight_tenths, 40);
        assert!(player.wielded.is_some());
        assert!(player.worn.is_some());
    }

    #[test]
    fn player_creation_matches_typescript_rng_consumption() {
        let cases = [
            (ClassId::Agent, 646_149_610),
            (ClassId::Rookie, 646_149_610),
            (ClassId::Veteran, 646_149_610),
            (ClassId::Tech, 11_692_391),
            (ClassId::Morphed, 1_021_329_383),
        ];
        for (class, expected_state) in cases {
            let mut rng = Rng::new(1_701_033);
            let player = Player::new(class, &mut rng);
            assert_eq!(player.agent_letter, b'L');
            assert_eq!(player.credits, 36);
            assert_eq!(rng.state(), expected_state, "class {}", class.key());
        }
    }
}
