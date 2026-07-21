use std::{fs, io, path::Path};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Outcome {
    pub won: bool,
    pub dead: bool,
    pub floor: u8,
    pub deepest: u8,
    pub turns: u32,
    pub score: i32,
    pub hp: String,
    pub kills: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoldenTrace {
    pub seed: u64,
    pub class: char,
    pub turn_cap: u32,
    pub line_hash: String,
    pub outcome_hash: String,
    pub outcome: Outcome,
    pub actions: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredGame {
    pub class: char,
    pub seed: u64,
    pub won: bool,
    pub dead: bool,
    pub floor: u8,
    pub deepest: u8,
    pub turns: u32,
    pub score: i32,
    pub hp: String,
    pub waste: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepCheckpoint {
    pub step: i32,
    pub action: String,
    pub choose_rng: u32,
    pub turn: u32,
    pub floor: u8,
    pub rng: u32,
    pub map_hash: u64,
    pub player: Vec<String>,
    pub inventory: String,
    pub mobs: String,
    pub items: String,
}

pub fn read_golden(path: impl AsRef<Path>) -> io::Result<GoldenTrace> {
    let text = fs::read_to_string(path)?;
    let mut lines = text.lines();
    require(lines.next() == Some("MIB_TRACE_V1"), "invalid trace header")?;
    let seed = parse_value::<u64>(lines.next(), "seed")?;
    let class = value(lines.next(), "class")?
        .chars()
        .next()
        .ok_or_else(|| invalid("missing class"))?;
    let turn_cap = parse_value::<u32>(lines.next(), "turn_cap")?;
    let line_hash = value(lines.next(), "line_hash")?.to_owned();
    let outcome_hash = value(lines.next(), "outcome_hash")?.to_owned();
    let outcome_fields = fields(lines.next(), "outcome")?;
    require(outcome_fields.len() == 8, "invalid outcome field count")?;
    let outcome = Outcome {
        won: parse_bool(outcome_fields[0])?,
        dead: parse_bool(outcome_fields[1])?,
        floor: parse(outcome_fields[2])?,
        deepest: parse(outcome_fields[3])?,
        turns: parse(outcome_fields[4])?,
        score: parse(outcome_fields[5])?,
        hp: outcome_fields[6].to_owned(),
        kills: parse(outcome_fields[7])?,
    };
    let action_count = parse_value::<usize>(lines.next(), "actions")?;
    let mut actions = Vec::with_capacity(action_count);
    for (expected_index, line) in lines.enumerate() {
        let (index, action) = line
            .split_once('\t')
            .ok_or_else(|| invalid("invalid action row"))?;
        require(
            parse::<usize>(index)? == expected_index,
            "non-sequential action index",
        )?;
        actions.push(action.to_owned());
    }
    require(actions.len() == action_count, "action count mismatch")?;
    Ok(GoldenTrace {
        seed,
        class,
        turn_cap,
        line_hash,
        outcome_hash,
        outcome,
        actions,
    })
}

pub fn read_stored_games(path: impl AsRef<Path>) -> io::Result<Vec<StoredGame>> {
    let text = fs::read_to_string(path)?;
    let mut lines = text.lines();
    require(
        lines.next() == Some("MIB_STORED_GAMES_V1"),
        "invalid stored-game header",
    )?;
    require(lines.next().is_some(), "missing stored-game columns")?;
    lines
        .map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            require(fields.len() == 10, "invalid stored-game row")?;
            Ok(StoredGame {
                class: fields[0]
                    .chars()
                    .next()
                    .ok_or_else(|| invalid("missing stored class"))?,
                seed: parse(fields[1])?,
                won: parse_bool(fields[2])?,
                dead: parse_bool(fields[3])?,
                floor: parse(fields[4])?,
                deepest: parse(fields[5])?,
                turns: parse(fields[6])?,
                score: parse(fields[7])?,
                hp: fields[8].to_owned(),
                waste: parse(fields[9])?,
            })
        })
        .collect()
}

pub fn read_steps(path: impl AsRef<Path>) -> io::Result<Vec<StepCheckpoint>> {
    let text = fs::read_to_string(path)?;
    let mut lines = text.lines();
    require(lines.next() == Some("MIB_STEPS_V1"), "invalid step header")?;
    require(lines.next().is_some(), "missing step columns")?;
    lines
        .map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            require(fields.len() == 11, "invalid step row")?;
            Ok(StepCheckpoint {
                step: parse(fields[0])?,
                action: fields[1].to_owned(),
                choose_rng: parse(fields[2])?,
                turn: parse(fields[3])?,
                floor: parse(fields[4])?,
                rng: parse(fields[5])?,
                map_hash: u64::from_str_radix(fields[6], 16)
                    .map_err(|_| invalid("invalid map hash"))?,
                player: fields[7].split(';').map(str::to_owned).collect(),
                inventory: fields[8].to_owned(),
                mobs: fields[9].to_owned(),
                items: fields[10].to_owned(),
            })
        })
        .collect()
}

fn value<'a>(line: Option<&'a str>, key: &str) -> io::Result<&'a str> {
    let mut fields = line
        .ok_or_else(|| invalid("missing fixture line"))?
        .split('\t');
    require(fields.next() == Some(key), "unexpected fixture key")?;
    fields
        .next()
        .ok_or_else(|| invalid("missing fixture value"))
}

fn fields<'a>(line: Option<&'a str>, key: &str) -> io::Result<Vec<&'a str>> {
    let mut fields = line
        .ok_or_else(|| invalid("missing fixture line"))?
        .split('\t');
    require(fields.next() == Some(key), "unexpected fixture key")?;
    Ok(fields.collect())
}

fn parse_value<T: std::str::FromStr>(line: Option<&str>, key: &str) -> io::Result<T> {
    parse(value(line, key)?)
}

fn parse<T: std::str::FromStr>(value: &str) -> io::Result<T> {
    value.parse().map_err(|_| invalid("invalid fixture number"))
}

fn parse_bool(value: &str) -> io::Result<bool> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(invalid("invalid fixture boolean")),
    }
}

fn require(condition: bool, message: &'static str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(invalid(message))
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{read_golden, read_steps, read_stored_games};

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn parses_preserved_fixtures() {
        let trace = read_golden(fixture("seed-1701033-a.trace")).unwrap();
        assert_eq!(trace.seed, 1_701_033);
        assert_eq!(trace.class, 'a');
        assert_eq!(trace.actions.len(), 556);
        assert_eq!(trace.outcome.turns, 554);
        assert!(trace.outcome.won);
        let games = read_stored_games(fixture("stored-games.tsv")).unwrap();
        assert_eq!(games.len(), 180);
        assert_eq!(games.iter().filter(|game| game.won).count(), 176);
        let steps = read_steps(fixture("seed-1701033-a.steps.tsv")).unwrap();
        assert_eq!(steps.len(), 557);
        assert_eq!(steps[0].step, -1);
        assert_eq!(steps.last().unwrap().turn, 554);
    }
}
