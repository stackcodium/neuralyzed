#[inline(always)]
fn sign(value: isize) -> isize {
    value.signum()
}

fn parse_target(value: &str) -> Result<usize, String> {
    let (x, y) = value
        .split_once(',')
        .ok_or_else(|| "invalid target".to_owned())?;
    let x: usize = x.parse().map_err(|_| "invalid target x".to_owned())?;
    let y: usize = y.parse().map_err(|_| "invalid target y".to_owned())?;
    if x >= WIDTH || y >= HEIGHT {
        return Err("target out of bounds".to_owned());
    }
    Ok(index(x, y))
}

fn skill_by_name(name: &str) -> Option<SkillId> {
    const NAMES: [(&str, SkillId); 22] = [
        ("commands", SkillId::Commands),
        ("bargaining", SkillId::Bargaining),
        ("intuition", SkillId::Intuition),
        ("detection", SkillId::Detection),
        ("brawling", SkillId::Brawling),
        ("hauling", SkillId::Hauling),
        ("quickdraw", SkillId::Quickdraw),
        ("acrobatics", SkillId::Acrobatics),
        ("analysis", SkillId::Analysis),
        ("xenology", SkillId::Xenology),
        ("backup", SkillId::Backup),
        ("interrogate", SkillId::Interrogate),
        ("blackmarket", SkillId::Blackmarket),
        ("deadeye", SkillId::Deadeye),
        ("fieldsurgeon", SkillId::Fieldsurgeon),
        ("bullettime", SkillId::Bullettime),
        ("exoslayer", SkillId::Exoslayer),
        ("ghostwalk", SkillId::Ghostwalk),
        ("menINblack", SkillId::MenInBlack),
        ("galaxydefender", SkillId::GalaxyDefender),
        ("universalremote", SkillId::UniversalRemote),
        ("shapeshift", SkillId::Shapeshift),
    ];
    NAMES
        .iter()
        .find_map(|(candidate, skill)| (*candidate == name).then_some(*skill))
}

fn item_signature(item: &Item) -> String {
    let (x, y) = coordinates(item.cell as usize);
    [
        item.spec().name.to_owned(),
        x.to_string(),
        y.to_string(),
        item.count.to_string(),
        item.enchantment.to_string(),
        bit(item.cursed),
        bit(item.identified),
        pill_name(item).to_owned(),
        item.price.to_string(),
        weight_string(item.weight_tenths),
    ]
    .join(";")
}

fn pill_name(item: &Item) -> &'static str {
    if item.gear != GearId::Pill {
        return "-";
    }
    use crate::data::PillEffect;
    match item.pill_effect {
        PillEffect::Heal => "heal",
        PillEffect::Poison => "poison",
        PillEffect::Strength => "strength",
        PillEffect::Blind => "blind",
        PillEffect::Polymorph => "polymorph",
        PillEffect::Telepathy => "telepathy",
        PillEffect::Hallucinate => "hallucinate",
        PillEffect::LevelUp => "levelup",
    }
}

fn modifier_name(modifier: MobMod) -> &'static str {
    match modifier {
        MobMod::Venomous => "venomous",
        MobMod::Camouflaged => "camouflaged",
        MobMod::Shrieking => "shrieking",
        MobMod::AcidBlooded => "acid-blooded",
        MobMod::Regenerating => "regenerating",
        MobMod::Thieving => "thieving",
        MobMod::SporeLaden => "spore-laden",
    }
}

fn bit(value: bool) -> String {
    if value { "1" } else { "0" }.to_owned()
}

fn weight_string(tenths: u16) -> String {
    if tenths.is_multiple_of(10) {
        (tenths / 10).to_string()
    } else {
        format!("{}.{:01}", tenths / 10, tenths % 10)
    }
}

fn burden_name(burden: Burden) -> &'static str {
    match burden {
        Burden::Unencumbered => "unencumbered",
        Burden::Burdened => "burdened",
        Burden::Strained => "strained",
        Burden::Overloaded => "overloaded",
    }
}

fn skill_from_index(index: usize) -> Option<SkillId> {
    const IDS: [SkillId; SkillId::COUNT] = [
        SkillId::Commands,
        SkillId::Bargaining,
        SkillId::Intuition,
        SkillId::Detection,
        SkillId::Brawling,
        SkillId::Hauling,
        SkillId::Quickdraw,
        SkillId::Acrobatics,
        SkillId::Analysis,
        SkillId::Xenology,
        SkillId::Backup,
        SkillId::Interrogate,
        SkillId::Blackmarket,
        SkillId::Deadeye,
        SkillId::Fieldsurgeon,
        SkillId::Bullettime,
        SkillId::Exoslayer,
        SkillId::Ghostwalk,
        SkillId::MenInBlack,
        SkillId::GalaxyDefender,
        SkillId::UniversalRemote,
        SkillId::Shapeshift,
    ];
    IDS.get(index).copied()
}

fn skill_name(skill: SkillId) -> &'static str {
    match skill {
        SkillId::Commands => "commands",
        SkillId::Bargaining => "bargaining",
        SkillId::Intuition => "intuition",
        SkillId::Detection => "detection",
        SkillId::Brawling => "brawling",
        SkillId::Hauling => "hauling",
        SkillId::Quickdraw => "quickdraw",
        SkillId::Acrobatics => "acrobatics",
        SkillId::Analysis => "analysis",
        SkillId::Xenology => "xenology",
        SkillId::Backup => "backup",
        SkillId::Interrogate => "interrogate",
        SkillId::Blackmarket => "blackmarket",
        SkillId::Deadeye => "deadeye",
        SkillId::Fieldsurgeon => "fieldsurgeon",
        SkillId::Bullettime => "bullettime",
        SkillId::Exoslayer => "exoslayer",
        SkillId::Ghostwalk => "ghostwalk",
        SkillId::MenInBlack => "menINblack",
        SkillId::GalaxyDefender => "galaxydefender",
        SkillId::UniversalRemote => "universalremote",
        SkillId::Shapeshift => "shapeshift",
    }
}
