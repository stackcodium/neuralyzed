//! Immutable gameplay definitions. Hot simulation state stores these stable IDs,
//! never owned strings or hash maps.

pub const MAX_FLOOR: u8 = 15;
pub const BOSS_FLOORS: [u8; 3] = [5, 10, 15];

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassId {
    Agent,
    Rookie,
    Veteran,
    Tech,
    Morphed,
}

impl ClassId {
    pub const fn from_key(key: char) -> Option<Self> {
        match key {
            'a' => Some(Self::Agent),
            'r' => Some(Self::Rookie),
            'v' => Some(Self::Veteran),
            't' => Some(Self::Tech),
            'm' => Some(Self::Morphed),
            _ => None,
        }
    }

    pub const fn key(self) -> char {
        match self {
            Self::Agent => 'a',
            Self::Rookie => 'r',
            Self::Veteran => 'v',
            Self::Tech => 't',
            Self::Morphed => 'm',
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stat {
    Strength,
    Dexterity,
    Perception,
    Charisma,
    Intelligence,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkillId {
    Commands,
    Bargaining,
    Intuition,
    Detection,
    Brawling,
    Hauling,
    Quickdraw,
    Acrobatics,
    Analysis,
    Xenology,
    Backup,
    Interrogate,
    Blackmarket,
    Deadeye,
    Fieldsurgeon,
    Bullettime,
    Exoslayer,
    Ghostwalk,
    MenInBlack,
    GalaxyDefender,
    UniversalRemote,
    Shapeshift,
}

impl SkillId {
    pub const COUNT: usize = 22;
    pub const PURCHASABLE_COUNT: usize = 21;
    pub const fn bit(self) -> u32 {
        1 << self as u8
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SkillSpec {
    pub stat: Option<Stat>,
    pub minimum: u8,
    pub cost: u8,
    pub requirements: u32,
}

const fn req(a: SkillId, b: SkillId) -> u32 {
    a.bit() | b.bit()
}

pub const SKILLS: [SkillSpec; SkillId::PURCHASABLE_COUNT] = [
    SkillSpec {
        stat: Some(Stat::Charisma),
        minimum: 13,
        cost: 1,
        requirements: 0,
    },
    SkillSpec {
        stat: Some(Stat::Charisma),
        minimum: 13,
        cost: 1,
        requirements: 0,
    },
    SkillSpec {
        stat: Some(Stat::Perception),
        minimum: 13,
        cost: 1,
        requirements: 0,
    },
    SkillSpec {
        stat: Some(Stat::Perception),
        minimum: 13,
        cost: 1,
        requirements: 0,
    },
    SkillSpec {
        stat: Some(Stat::Strength),
        minimum: 13,
        cost: 1,
        requirements: 0,
    },
    SkillSpec {
        stat: Some(Stat::Strength),
        minimum: 13,
        cost: 1,
        requirements: 0,
    },
    SkillSpec {
        stat: Some(Stat::Dexterity),
        minimum: 13,
        cost: 1,
        requirements: 0,
    },
    SkillSpec {
        stat: Some(Stat::Dexterity),
        minimum: 13,
        cost: 1,
        requirements: 0,
    },
    SkillSpec {
        stat: Some(Stat::Intelligence),
        minimum: 13,
        cost: 1,
        requirements: 0,
    },
    SkillSpec {
        stat: Some(Stat::Intelligence),
        minimum: 13,
        cost: 1,
        requirements: 0,
    },
    SkillSpec {
        stat: None,
        minimum: 0,
        cost: 2,
        requirements: req(SkillId::Commands, SkillId::Detection),
    },
    SkillSpec {
        stat: None,
        minimum: 0,
        cost: 2,
        requirements: req(SkillId::Commands, SkillId::Xenology),
    },
    SkillSpec {
        stat: None,
        minimum: 0,
        cost: 2,
        requirements: req(SkillId::Bargaining, SkillId::Analysis),
    },
    SkillSpec {
        stat: None,
        minimum: 0,
        cost: 2,
        requirements: req(SkillId::Quickdraw, SkillId::Intuition),
    },
    SkillSpec {
        stat: None,
        minimum: 0,
        cost: 2,
        requirements: req(SkillId::Analysis, SkillId::Hauling),
    },
    SkillSpec {
        stat: None,
        minimum: 0,
        cost: 2,
        requirements: req(SkillId::Acrobatics, SkillId::Quickdraw),
    },
    SkillSpec {
        stat: None,
        minimum: 0,
        cost: 2,
        requirements: req(SkillId::Brawling, SkillId::Xenology),
    },
    SkillSpec {
        stat: None,
        minimum: 0,
        cost: 2,
        requirements: req(SkillId::Acrobatics, SkillId::Detection),
    },
    SkillSpec {
        stat: None,
        minimum: 0,
        cost: 3,
        requirements: req(SkillId::Backup, SkillId::Deadeye),
    },
    SkillSpec {
        stat: None,
        minimum: 0,
        cost: 3,
        requirements: req(SkillId::Exoslayer, SkillId::Fieldsurgeon),
    },
    SkillSpec {
        stat: None,
        minimum: 0,
        cost: 3,
        requirements: req(SkillId::Interrogate, SkillId::Blackmarket),
    },
];

#[derive(Clone, Copy, Debug)]
pub struct ClassSpec {
    pub stats: [u8; 5],
    pub hp: i16,
    pub initial_skill: Option<SkillId>,
    pub xp_multiplier: f64,
    pub shop_multiplier: f64,
    pub gear: &'static [GearId],
}

pub const CLASSES: [ClassSpec; 5] = [
    ClassSpec {
        stats: [12; 5],
        hp: 20,
        initial_skill: None,
        xp_multiplier: 1.0,
        shop_multiplier: 1.0,
        gear: &[
            GearId::NoisyCricket,
            GearId::BlackSuit,
            GearId::Ration,
            GearId::Ration,
            GearId::Battery,
            GearId::NeuralyzerCharge,
        ],
    },
    ClassSpec {
        stats: [10, 11, 11, 11, 13],
        hp: 16,
        initial_skill: None,
        xp_multiplier: 1.5,
        shop_multiplier: 1.0,
        gear: &[
            GearId::StandardPistol,
            GearId::CheapSuit,
            GearId::Ration,
            GearId::Coffee,
            GearId::BulletClip,
        ],
    },
    ClassSpec {
        stats: [14, 10, 15, 11, 12],
        hp: 24,
        initial_skill: Some(SkillId::Intuition),
        xp_multiplier: 0.8,
        shop_multiplier: 1.0,
        gear: &[
            GearId::Series4,
            GearId::WornSuit,
            GearId::Ration,
            GearId::Cigar,
            GearId::Battery,
        ],
    },
    ClassSpec {
        stats: [9, 11, 13, 10, 16],
        hp: 14,
        initial_skill: Some(SkillId::Analysis),
        xp_multiplier: 1.0,
        shop_multiplier: 1.0,
        gear: &[
            GearId::PrototypeZapper,
            GearId::LabCoat,
            GearId::Ration,
            GearId::Scanner,
            GearId::Battery,
            GearId::Battery,
            GearId::FoamGrenade,
        ],
    },
    ClassSpec {
        stats: [13, 14, 12, 8, 12],
        hp: 22,
        initial_skill: Some(SkillId::Shapeshift),
        xp_multiplier: 1.0,
        shop_multiplier: 1.3,
        gear: &[
            GearId::BoneSpur,
            GearId::SyntheticSkin,
            GearId::Ration,
            GearId::RoyalJelly,
        ],
    },
];

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GearId {
    NoisyCricket,
    StandardPistol,
    PrototypeZapper,
    Series4,
    Carbonizer,
    MutatingCarbonizer,
    TriBarrel,
    BoneSpur,
    StunBaton,
    ArquillianSaber,
    SugarWaterCannon,
    CheapSuit,
    BlackSuit,
    WornSuit,
    LabCoat,
    SyntheticSkin,
    KevlarSuit,
    BattlePlate,
    ExoskeletonHusk,
    Ration,
    Coffee,
    RoyalJelly,
    Cigar,
    Pill,
    Battery,
    BulletClip,
    NeuralyzerCharge,
    Scanner,
    FoamGrenade,
    PocketUniverse,
    Deneuralyzer,
    Galaxy,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ammo {
    None,
    Plasma,
    Bullet,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GearKind {
    Weapon,
    Armor,
    Food,
    Pill,
    Ammo,
    Tool,
    Thrown,
    Quest,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolUse {
    Neuralyze,
    Identify,
    Freeze,
    Teleport,
    RestoreStat,
}

impl GearId {
    pub const fn kind(self) -> GearKind {
        match self as u8 {
            0..=10 => GearKind::Weapon,
            11..=18 => GearKind::Armor,
            19..=22 => GearKind::Food,
            23 => GearKind::Pill,
            24..=25 => GearKind::Ammo,
            26..=27 | 29..=30 => GearKind::Tool,
            28 => GearKind::Thrown,
            _ => GearKind::Quest,
        }
    }

    pub const fn tool_use(self) -> Option<ToolUse> {
        match self {
            Self::NeuralyzerCharge => Some(ToolUse::Neuralyze),
            Self::Scanner => Some(ToolUse::Identify),
            Self::FoamGrenade => Some(ToolUse::Freeze),
            Self::PocketUniverse => Some(ToolUse::Teleport),
            Self::Deneuralyzer => Some(ToolUse::RestoreStat),
            _ => None,
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PillEffect {
    Heal,
    Poison,
    Strength,
    Blind,
    Polymorph,
    Telepathy,
    Hallucinate,
    LevelUp,
}

pub mod gear_flags {
    pub const MELEE: u16 = 1 << 0;
    pub const KICK: u16 = 1 << 1;
    pub const POLYMORPH: u16 = 1 << 2;
    pub const BUG_BAIT: u16 = 1 << 3;
    pub const BUG_SCARE: u16 = 1 << 4;
    pub const UNIDENTIFIED: u16 = 1 << 5;
}

#[derive(Clone, Copy, Debug)]
pub struct GearSpec {
    pub name: &'static str,
    pub damage: [u8; 2],
    pub range: u8,
    pub ammo: Ammo,
    pub weight_tenths: u16,
    pub tier: u8,
    pub armor: u8,
    pub quantity: [u8; 2],
    pub nutrition: u16,
    pub heal: u8,
    pub haste: u8,
    pub burst: u8,
    pub stun: f64,
    pub flags: u16,
}

const fn gear(name: &'static str) -> GearSpec {
    GearSpec {
        name,
        damage: [0, 0],
        range: 0,
        ammo: Ammo::None,
        weight_tenths: 0,
        tier: 0,
        armor: 0,
        quantity: [0, 0],
        nutrition: 0,
        heal: 0,
        haste: 0,
        burst: 0,
        stun: 0.0,
        flags: 0,
    }
}

pub const GEAR: [GearSpec; 32] = {
    let mut a = [gear(""); 32];
    a[0] = GearSpec {
        name: "noisy cricket",
        damage: [3, 18],
        range: 7,
        ammo: Ammo::Plasma,
        weight_tenths: 10,
        tier: 1,
        flags: gear_flags::KICK,
        ..gear("")
    };
    a[1] = GearSpec {
        name: "standard pistol",
        damage: [2, 8],
        range: 6,
        ammo: Ammo::Bullet,
        weight_tenths: 30,
        tier: 1,
        ..gear("")
    };
    a[2] = GearSpec {
        name: "prototype zapper",
        damage: [1, 7],
        range: 6,
        ammo: Ammo::Plasma,
        weight_tenths: 20,
        tier: 1,
        ..gear("")
    };
    a[3] = GearSpec {
        name: "series 4 de-atomizer",
        damage: [4, 12],
        range: 8,
        ammo: Ammo::Plasma,
        weight_tenths: 60,
        tier: 2,
        ..gear("")
    };
    a[4] = GearSpec {
        name: "carbonizer",
        damage: [6, 20],
        range: 9,
        ammo: Ammo::Plasma,
        weight_tenths: 90,
        tier: 3,
        ..gear("")
    };
    a[5] = GearSpec {
        name: "reverberating carbonizer w/ mutate capacity",
        damage: [8, 24],
        range: 9,
        ammo: Ammo::Plasma,
        weight_tenths: 100,
        tier: 4,
        flags: gear_flags::POLYMORPH,
        ..gear("")
    };
    a[6] = GearSpec {
        name: "tri-barrel plasma gun",
        damage: [5, 15],
        range: 7,
        ammo: Ammo::Plasma,
        weight_tenths: 80,
        tier: 3,
        burst: 3,
        ..gear("")
    };
    a[7] = GearSpec {
        name: "bone spur",
        damage: [3, 7],
        weight_tenths: 20,
        tier: 1,
        flags: gear_flags::MELEE,
        ..gear("")
    };
    a[8] = GearSpec {
        name: "stun baton",
        damage: [2, 6],
        weight_tenths: 30,
        tier: 1,
        stun: 0.3,
        flags: gear_flags::MELEE,
        ..gear("")
    };
    a[9] = GearSpec {
        name: "arquillian saber",
        damage: [5, 14],
        weight_tenths: 40,
        tier: 3,
        flags: gear_flags::MELEE,
        ..gear("")
    };
    a[10] = GearSpec {
        name: "sugar-water cannon",
        damage: [1, 4],
        range: 5,
        ammo: Ammo::Plasma,
        weight_tenths: 70,
        tier: 1,
        flags: gear_flags::BUG_BAIT,
        ..gear("")
    };
    a[11] = GearSpec {
        name: "cheap suit",
        armor: 1,
        weight_tenths: 40,
        ..gear("")
    };
    a[12] = GearSpec {
        name: "black suit",
        armor: 2,
        weight_tenths: 40,
        ..gear("")
    };
    a[13] = GearSpec {
        name: "worn suit",
        armor: 2,
        weight_tenths: 40,
        ..gear("")
    };
    a[14] = GearSpec {
        name: "lab coat",
        armor: 1,
        weight_tenths: 30,
        ..gear("")
    };
    a[15] = GearSpec {
        name: "synthetic skin",
        armor: 1,
        weight_tenths: 20,
        ..gear("")
    };
    a[16] = GearSpec {
        name: "kevlar weave suit",
        armor: 4,
        weight_tenths: 60,
        ..gear("")
    };
    a[17] = GearSpec {
        name: "arquillian battle plate",
        armor: 6,
        weight_tenths: 120,
        ..gear("")
    };
    a[18] = GearSpec {
        name: "exoskeleton husk",
        armor: 5,
        weight_tenths: 90,
        flags: gear_flags::BUG_SCARE,
        ..gear("")
    };
    a[19] = GearSpec {
        name: "ration",
        weight_tenths: 20,
        nutrition: 1000,
        heal: 6,
        ..gear("")
    };
    a[20] = GearSpec {
        name: "coffee",
        weight_tenths: 10,
        nutrition: 50,
        haste: 10,
        ..gear("")
    };
    a[21] = GearSpec {
        name: "royal jelly",
        weight_tenths: 10,
        nutrition: 400,
        heal: 22,
        ..gear("")
    };
    a[22] = GearSpec {
        name: "cigar",
        weight_tenths: 10,
        nutrition: 10,
        ..gear("")
    };
    a[23] = GearSpec {
        name: "pill",
        weight_tenths: 1,
        flags: gear_flags::UNIDENTIFIED,
        ..gear("")
    };
    a[24] = GearSpec {
        name: "battery",
        ammo: Ammo::Plasma,
        weight_tenths: 10,
        quantity: [6, 14],
        ..gear("")
    };
    a[25] = GearSpec {
        name: "bullet clip",
        ammo: Ammo::Bullet,
        weight_tenths: 10,
        quantity: [5, 12],
        ..gear("")
    };
    a[26] = GearSpec {
        name: "neuralyzer charge",
        weight_tenths: 10,
        ..gear("")
    };
    a[27] = GearSpec {
        name: "scanner",
        weight_tenths: 20,
        ..gear("")
    };
    a[28] = GearSpec {
        name: "containment foam grenade",
        weight_tenths: 10,
        ..gear("")
    };
    a[29] = GearSpec {
        name: "pocket universe marble",
        weight_tenths: 1,
        ..gear("")
    };
    a[30] = GearSpec {
        name: "deneuralyzer slip",
        weight_tenths: 1,
        ..gear("")
    };
    a[31] = GearSpec {
        name: "the galaxy",
        weight_tenths: 1,
        ..gear("")
    };
    a
};

pub mod mob_flags {
    pub const BUG: u16 = 1 << 0;
    pub const COFFEE: u16 = 1 << 1;
    pub const REGEN: u16 = 1 << 2;
    pub const SPLITS: u16 = 1 << 3;
    pub const STEALS: u16 = 1 << 4;
    pub const PHASE: u16 = 1 << 5;
    pub const SWARM: u16 = 1 << 6;
    pub const DISGUISED: u16 = 1 << 7;
    pub const SHOP: u16 = 1 << 8;
    pub const GRAB: u16 = 1 << 9;
    pub const EATS: u16 = 1 << 10;
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MobId {
    SewerSquid,
    WormGuy,
    RefugeeGrub,
    JeebsClone,
    BugDrone,
    SkinSuit,
    ArquillianMarine,
    TwinSentinels,
    FeralCat,
    BugWarrior,
    RogueAgent,
    PlasmaWraith,
    HiveGuardian,
    QueensBrood,
    JeebsPrime,
    Serleena,
    Edgar,
    MibAgent,
}

#[derive(Clone, Copy, Debug)]
pub struct MobSpec {
    pub name: &'static str,
    pub glyph: u8,
    pub hp: i16,
    pub damage: [u8; 2],
    pub speed: f64,
    pub tier: u8,
    pub xp: u16,
    pub armor: u8,
    pub ranged: u8,
    pub summon: Option<MobId>,
    pub flags: u16,
}

#[allow(clippy::too_many_arguments)] // Compact constructor used only by the immutable table.
const fn mob(
    name: &'static str,
    glyph: u8,
    hp: i16,
    damage: [u8; 2],
    speed: f64,
    tier: u8,
    xp: u16,
    flags: u16,
) -> MobSpec {
    MobSpec {
        name,
        glyph,
        hp,
        damage,
        speed,
        tier,
        xp,
        armor: 0,
        ranged: 0,
        summon: None,
        flags,
    }
}

pub const MOBS: [MobSpec; 18] = [
    mob("sewer squid", b's', 8, [1, 4], 1.0, 1, 5, 0),
    mob("worm guy", b'w', 6, [1, 3], 1.0, 1, 4, mob_flags::COFFEE),
    mob("refugee grub", b'g', 5, [1, 2], 0.7, 1, 3, 0),
    mob("jeebs clone", b'j', 10, [2, 5], 1.0, 1, 7, mob_flags::REGEN),
    mob("bug drone", b'b', 14, [2, 7], 1.2, 2, 12, mob_flags::BUG),
    mob(
        "skin-suit shambler",
        b'E',
        22,
        [3, 8],
        0.8,
        2,
        18,
        mob_flags::BUG | mob_flags::DISGUISED,
    ),
    MobSpec {
        ranged: 6,
        ..mob("arquillian marine", b'A', 18, [3, 9], 1.0, 2, 15, 0)
    },
    mob(
        "twin sentinels",
        b'T',
        16,
        [2, 6],
        1.0,
        2,
        14,
        mob_flags::SPLITS,
    ),
    mob("orion's cat (feral)", b'f', 9, [1, 4], 1.6, 2, 13, 0),
    mob("bug warrior", b'B', 28, [3, 9], 1.1, 3, 28, mob_flags::BUG),
    MobSpec {
        ranged: 7,
        ..mob(
            "rogue agent (rogue)",
            b'@',
            24,
            [3, 8],
            1.0,
            3,
            25,
            mob_flags::STEALS,
        )
    },
    mob(
        "plasma wraith",
        b'P',
        18,
        [4, 10],
        1.25,
        3,
        30,
        mob_flags::PHASE,
    ),
    MobSpec {
        armor: 3,
        ..mob(
            "hive guardian",
            b'H',
            38,
            [4, 11],
            0.9,
            4,
            50,
            mob_flags::BUG,
        )
    },
    mob(
        "bug queen's brood",
        b'q',
        14,
        [2, 7],
        1.35,
        4,
        20,
        mob_flags::BUG | mob_flags::SWARM,
    ),
    mob(
        "JACK JEEBS PRIME",
        b'J',
        42,
        [2, 7],
        1.0,
        4,
        100,
        mob_flags::REGEN | mob_flags::SHOP,
    ),
    MobSpec {
        summon: Some(MobId::BugDrone),
        ..mob(
            "SERLEENA SPROUT",
            b'S',
            82,
            [3, 9],
            1.0,
            4,
            250,
            mob_flags::GRAB,
        )
    },
    MobSpec {
        armor: 4,
        ..mob(
            "EDGAR THE BUG",
            b'E',
            140,
            [5, 13],
            1.0,
            4,
            999,
            mob_flags::BUG | mob_flags::EATS,
        )
    },
    mob("MIB agent", b'@', 15, [3, 8], 1.0, 2, 0, 0),
];

#[derive(Clone, Copy, Debug)]
pub struct MobPrefix {
    pub name: &'static str,
    pub hp_multiplier: f64,
    pub damage_multiplier: f64,
    pub xp_multiplier: u8,
}

pub const MOB_PREFIXES: [MobPrefix; 6] = [
    MobPrefix {
        name: "juvenile",
        hp_multiplier: 0.6,
        damage_multiplier: 0.7,
        xp_multiplier: 1,
    },
    MobPrefix {
        name: "",
        hp_multiplier: 1.0,
        damage_multiplier: 1.0,
        xp_multiplier: 1,
    },
    MobPrefix {
        name: "hardened",
        hp_multiplier: 1.1,
        damage_multiplier: 1.02,
        xp_multiplier: 1,
    },
    MobPrefix {
        name: "royal",
        hp_multiplier: 1.25,
        damage_multiplier: 1.08,
        xp_multiplier: 2,
    },
    MobPrefix {
        name: "unlicensed",
        hp_multiplier: 1.0,
        damage_multiplier: 1.04,
        xp_multiplier: 1,
    },
    MobPrefix {
        name: "cranky",
        hp_multiplier: 1.04,
        damage_multiplier: 1.06,
        xp_multiplier: 1,
    },
];

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MobMod {
    Venomous,
    Camouflaged,
    Shrieking,
    AcidBlooded,
    Regenerating,
    Thieving,
    SporeLaden,
}

#[derive(Clone, Copy, Debug)]
pub struct PolyForm {
    pub strength: u8,
    pub dexterity: u8,
    pub damage: [u8; 2],
    pub bug: bool,
    pub disguise: bool,
}

pub const POLY_FORMS: [PolyForm; 3] = [
    PolyForm {
        strength: 16,
        dexterity: 10,
        damage: [4, 11],
        bug: true,
        disguise: false,
    },
    PolyForm {
        strength: 6,
        dexterity: 18,
        damage: [2, 5],
        bug: false,
        disguise: false,
    },
    PolyForm {
        strength: 14,
        dexterity: 8,
        damage: [3, 8],
        bug: false,
        disguise: true,
    },
];

#[cfg(test)]
mod tests {
    use super::{CLASSES, ClassId, GEAR, GearId, MOBS, MobId, SkillId};

    #[test]
    fn stable_ids_index_the_tables() {
        assert_eq!(ClassId::from_key('v'), Some(ClassId::Veteran));
        assert_eq!(CLASSES[ClassId::Rookie as usize].hp, 16);
        assert_eq!(GEAR[GearId::Carbonizer as usize].damage, [6, 20]);
        assert_eq!(MOBS[MobId::Edgar as usize].hp, 140);
        assert_eq!(
            SkillId::UniversalRemote as usize,
            SkillId::PURCHASABLE_COUNT - 1
        );
        assert_eq!(SkillId::Shapeshift as usize, SkillId::COUNT - 1);
    }
}
