//! Importing real club data.
//!
//! No real club or player data lives in this repository (ADR-007) and none ever
//! will. This module defines the format real data arrives in; the files
//! themselves live outside the repo, in a directory you point the game at.
//!
//! That separation is not ceremony. It means the repo can be published or
//! shared without any decision about data licensing, and it means the import
//! path is exercised from day one rather than bolted on later — because it is
//! how the game is actually played.
//!
//! # Format
//!
//! Two CSV files. Header row required, column order irrelevant, `#` comments
//! allowed at line start.
//!
//! `stadiums.csv`
//! ```text
//! id,name,formal_name,city,state,capacity,opened,lat,lon
//! ```
//!
//! `clubs.csv`
//! ```text
//! id,name,short_name,nickname,founded,city,state,stadium_id,division,reputation,primary_color,secondary_color,crest
//! ```
//!
//! `name` is the everyday name and `formal_name` the registered one — stadium
//! naming rights change every few years, so the two must be separable and the
//! import must be re-runnable rather than a one-time seeding.
//!
//! `crest` is a path relative to the data directory. Images are never read by
//! `sim-core`; the field is carried through for the interface to resolve.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum ImportError {
    MissingColumn { file: &'static str, column: &'static str },
    BadRow { file: &'static str, line: usize, reason: String },
    UnknownStadium { club: String, stadium_id: String },
    DuplicateId { file: &'static str, id: String },
    Empty(&'static str),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::MissingColumn { file, column } => {
                write!(f, "{file}: missing required column `{column}`")
            }
            ImportError::BadRow { file, line, reason } => {
                write!(f, "{file}:{line}: {reason}")
            }
            ImportError::UnknownStadium { club, stadium_id } => {
                write!(f, "club `{club}` references unknown stadium `{stadium_id}`")
            }
            ImportError::DuplicateId { file, id } => write!(f, "{file}: duplicate id `{id}`"),
            ImportError::Empty(file) => write!(f, "{file}: no rows"),
        }
    }
}

impl std::error::Error for ImportError {}

#[derive(Debug, Clone, PartialEq)]
pub struct Stadium {
    pub id: String,
    /// Everyday name, including current naming rights.
    pub name: String,
    /// Registered name, which outlives sponsors.
    pub formal_name: Option<String>,
    pub city: String,
    pub state: String,
    pub capacity: u32,
    pub opened: Option<i32>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClubData {
    pub id: String,
    pub name: String,
    pub short_name: String,
    pub nickname: Option<String>,
    pub founded: Option<i32>,
    pub city: String,
    pub state: String,
    pub stadium_id: String,
    pub division: u8,
    /// Stature, 1..=20. Who will sign for you, sponsorship and merchandising,
    /// youth stream quality, and how much the board and fans expect. Slow-moving
    /// and largely inherited.
    pub reputation: u8,
    /// Current squad quality, 1..=20. Drives generated squads only.
    ///
    /// Separate from reputation because the gap between them is a game mechanic,
    /// not noise. A club whose stature exceeds its squad can sign above its
    /// weight and has fans expecting more than it can deliver; a club whose squad
    /// exceeds its stature must overpay to attract anyone and gets no credit for
    /// overachieving. Defaults to reputation when the column is absent.
    pub squad_strength: u8,
    pub primary_color: String,
    pub secondary_color: String,
    /// Path relative to the data directory. Never loaded by `sim-core`.
    pub crest: Option<String>,
}

/// A real player's identity. Ability is never imported — no public source
/// encodes it, and inventing one per player would be worse than generating it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerData {
    pub club_id: String,
    pub name: String,
    /// Broad position as recorded upstream: GK, DF, MF or FW.
    ///
    /// Deliberately not specialised here. Wikipedia records only these four, so
    /// storing the broad value keeps the CSV faithful to its source and means a
    /// re-scrape never silently moves a player between zagueiro and lateral.
    /// Specialisation happens at world generation, seeded and squad-constrained.
    pub broad_position: BroadPosition,
    pub nationality: String,
    pub birth_year: Option<i32>,
    pub number: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadPosition {
    Goalkeeper,
    // Specific — no guessing needed.
    CentreBack,
    FullBack,
    DefensiveMid,
    CentralMid,
    AttackingMid,
    Winger,
    Striker,
    // Broad — the importer has to split these by seed.
    Defender,
    Midfielder,
    Forward,
}

impl BroadPosition {
    /// Accepts both the four broad codes and the specific ones Brazilian season
    /// articles actually use.
    ///
    /// The specific codes matter: `CB` and `LB` are both defenders, but one is a
    /// zagueiro and the other a lateral, and only the source knows which.
    /// Anything unrecognised is rejected rather than guessed.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "GK" => Some(BroadPosition::Goalkeeper),
            "CB" | "SW" => Some(BroadPosition::CentreBack),
            "RB" | "LB" | "RWB" | "LWB" => Some(BroadPosition::FullBack),
            "DM" => Some(BroadPosition::DefensiveMid),
            "CM" | "LM" | "RM" => Some(BroadPosition::CentralMid),
            "AM" => Some(BroadPosition::AttackingMid),
            "RW" | "LW" => Some(BroadPosition::Winger),
            "CF" | "ST" | "SS" => Some(BroadPosition::Striker),
            // Broad fallbacks, used when a source only records four positions.
            "DF" => Some(BroadPosition::Defender),
            "MF" => Some(BroadPosition::Midfielder),
            "FW" => Some(BroadPosition::Forward),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dataset {
    pub stadiums: Vec<Stadium>,
    pub clubs: Vec<ClubData>,
    /// Empty when no players.csv is supplied — the generator then invents names.
    pub players: Vec<PlayerData>,
}

impl Dataset {
    pub fn parse(clubs_csv: &str, stadiums_csv: &str) -> Result<Self, ImportError> {
        Self::parse_with_players(clubs_csv, stadiums_csv, None)
    }

    pub fn parse_with_players(
        clubs_csv: &str,
        stadiums_csv: &str,
        players_csv: Option<&str>,
    ) -> Result<Self, ImportError> {
        let stadiums = parse_stadiums(stadiums_csv)?;
        let clubs = parse_clubs(clubs_csv)?;

        // Referential integrity is checked at import, not at use. A dataset that
        // parses should never blow up mid-season.
        let known: Vec<&str> = stadiums.iter().map(|s| s.id.as_str()).collect();
        for c in &clubs {
            if !known.contains(&c.stadium_id.as_str()) {
                return Err(ImportError::UnknownStadium {
                    club: c.name.clone(),
                    stadium_id: c.stadium_id.clone(),
                });
            }
        }
        let players = match players_csv {
            Some(src) => {
                let parsed = parse_players(src)?;
                // Players naming a club that is not in the dataset are dropped
                // rather than rejected: a scrape may cover clubs you have since
                // removed, and that should not fail the whole import.
                let ids: Vec<&str> = clubs.iter().map(|c| c.id.as_str()).collect();
                parsed
                    .into_iter()
                    .filter(|p| ids.contains(&p.club_id.as_str()))
                    .collect()
            }
            None => Vec::new(),
        };
        Ok(Dataset { stadiums, clubs, players })
    }

    /// Stable hash of everything the dataset contributes to world generation.
    ///
    /// Reputation and squad strength are pre-game levers the player is expected
    /// to edit, so a seed alone no longer identifies a world. Recording this
    /// alongside the seed is what keeps a save reproducible and two harness runs
    /// honestly comparable.
    ///
    /// Covers only fields that affect generation. Colours, crests and stadium
    /// names are presentation: editing them must not invalidate a save.
    pub fn content_hash(&self) -> u64 {
        use siphasher::sip::SipHasher13;
        use std::hash::Hasher;
        let mut h = SipHasher13::new_with_keys(0x2b7e_1516_28ae_d2a6, 0xabf7_1588_09cf_4f3c);
        // Dataset order is the file's order, which is stable; sorting would hide
        // a genuine reordering that does change generation.
        for c in &self.clubs {
            h.write(c.id.as_bytes());
            h.write_u8(0);
            h.write(c.name.as_bytes());
            h.write_u8(c.division);
            h.write_u8(c.reputation);
            h.write_u8(c.squad_strength);
        }
        for p in &self.players {
            h.write(p.club_id.as_bytes());
            h.write(p.name.as_bytes());
            h.write_u8(p.broad_position as u8);
            h.write_i32(p.birth_year.unwrap_or(0));
        }
        h.finish()
    }

    pub fn players_of<'a>(&'a self, club_id: &'a str) -> impl Iterator<Item = &'a PlayerData> + 'a {
        self.players.iter().filter(move |p| p.club_id == club_id)
    }

    pub fn stadium(&self, id: &str) -> Option<&Stadium> {
        self.stadiums.iter().find(|s| s.id == id)
    }

    pub fn clubs_in_division(&self, division: u8) -> impl Iterator<Item = &ClubData> {
        self.clubs.iter().filter(move |c| c.division == division)
    }
}

/// Minimal CSV reader: splits on commas, honours double-quoted fields.
///
/// Deliberately not a CSV crate — `sim-core` stays dependency-light, and the
/// format is ours to keep simple. Swap in `csv` if the data ever needs embedded
/// newlines.
fn split_row(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

struct Header {
    columns: BTreeMap<String, usize>,
    file: &'static str,
}

impl Header {
    fn new(line: &str, file: &'static str) -> Self {
        let columns = split_row(line)
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name.to_lowercase(), i))
            .collect();
        Header { columns, file }
    }

    fn required(&self, name: &'static str) -> Result<usize, ImportError> {
        self.columns
            .get(name)
            .copied()
            .ok_or(ImportError::MissingColumn { file: self.file, column: name })
    }

    fn optional(&self, name: &str) -> Option<usize> {
        self.columns.get(name).copied()
    }
}

fn field<'a>(row: &'a [String], idx: Option<usize>) -> Option<&'a str> {
    idx.and_then(|i| row.get(i))
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
}

fn rows(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
}

fn parse_stadiums(source: &str) -> Result<Vec<Stadium>, ImportError> {
    const FILE: &str = "stadiums.csv";
    let mut iter = rows(source);
    let (_, header_line) = iter.next().ok_or(ImportError::Empty(FILE))?;
    let h = Header::new(header_line, FILE);

    let (c_id, c_name, c_city, c_state, c_cap) = (
        h.required("id")?,
        h.required("name")?,
        h.required("city")?,
        h.required("state")?,
        h.required("capacity")?,
    );
    let (c_formal, c_opened, c_lat, c_lon) = (
        h.optional("formal_name"),
        h.optional("opened"),
        h.optional("lat"),
        h.optional("lon"),
    );

    let mut out: Vec<Stadium> = Vec::new();
    for (line, raw) in iter {
        let r = split_row(raw);
        let id = r.get(c_id).cloned().unwrap_or_default();
        if id.is_empty() {
            return Err(ImportError::BadRow { file: FILE, line, reason: "empty id".into() });
        }
        if out.iter().any(|s| s.id == id) {
            return Err(ImportError::DuplicateId { file: FILE, id });
        }
        let capacity = r
            .get(c_cap)
            .and_then(|v| v.replace([',', '.'], "").parse().ok())
            .ok_or(ImportError::BadRow {
                file: FILE,
                line,
                reason: format!("bad capacity for `{id}`"),
            })?;
        out.push(Stadium {
            id,
            name: r.get(c_name).cloned().unwrap_or_default(),
            formal_name: field(&r, c_formal).map(str::to_string),
            city: r.get(c_city).cloned().unwrap_or_default(),
            state: r.get(c_state).cloned().unwrap_or_default(),
            capacity,
            opened: field(&r, c_opened).and_then(|v| v.parse().ok()),
            lat: field(&r, c_lat).and_then(|v| v.parse().ok()),
            lon: field(&r, c_lon).and_then(|v| v.parse().ok()),
        });
    }
    if out.is_empty() {
        return Err(ImportError::Empty(FILE));
    }
    Ok(out)
}

fn parse_clubs(source: &str) -> Result<Vec<ClubData>, ImportError> {
    const FILE: &str = "clubs.csv";
    let mut iter = rows(source);
    let (_, header_line) = iter.next().ok_or(ImportError::Empty(FILE))?;
    let h = Header::new(header_line, FILE);

    let (c_id, c_name, c_stadium, c_division) = (
        h.required("id")?,
        h.required("name")?,
        h.required("stadium_id")?,
        h.required("division")?,
    );
    let (c_short, c_nick, c_founded, c_city, c_state, c_rep, c_squad, c_p1, c_p2, c_crest) = (
        h.optional("short_name"),
        h.optional("nickname"),
        h.optional("founded"),
        h.optional("city"),
        h.optional("state"),
        h.optional("reputation"),
        h.optional("squad_strength"),
        h.optional("primary_color"),
        h.optional("secondary_color"),
        h.optional("crest"),
    );

    let mut out: Vec<ClubData> = Vec::new();
    for (line, raw) in iter {
        let r = split_row(raw);
        let id = r.get(c_id).cloned().unwrap_or_default();
        if id.is_empty() {
            return Err(ImportError::BadRow { file: FILE, line, reason: "empty id".into() });
        }
        if out.iter().any(|c| c.id == id) {
            return Err(ImportError::DuplicateId { file: FILE, id });
        }
        let name = r.get(c_name).cloned().unwrap_or_default();
        let division = r
            .get(c_division)
            .and_then(|v| v.parse::<u8>().ok())
            .ok_or(ImportError::BadRow {
                file: FILE,
                line,
                reason: format!("bad division for `{name}`"),
            })?;
        let short_name = field(&r, c_short)
            .map(str::to_string)
            .unwrap_or_else(|| name.clone());
        let reputation = field(&r, c_rep)
            .and_then(|v| v.parse::<u8>().ok())
            .unwrap_or(10)
            .clamp(1, 20);
        out.push(ClubData {
            id,
            name,
            short_name,
            nickname: field(&r, c_nick).map(str::to_string),
            founded: field(&r, c_founded).and_then(|v| v.parse().ok()),
            city: field(&r, c_city).unwrap_or("").to_string(),
            state: field(&r, c_state).unwrap_or("").to_string(),
            stadium_id: r.get(c_stadium).cloned().unwrap_or_default(),
            division,
            reputation,
            squad_strength: field(&r, c_squad)
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(reputation)
                .clamp(1, 20),
            primary_color: field(&r, c_p1).unwrap_or("#2E5E4E").to_string(),
            secondary_color: field(&r, c_p2).unwrap_or("#F7F8F4").to_string(),
            crest: field(&r, c_crest).map(str::to_string),
        });
    }
    if out.is_empty() {
        return Err(ImportError::Empty(FILE));
    }
    Ok(out)
}

fn parse_players(source: &str) -> Result<Vec<PlayerData>, ImportError> {
    const FILE: &str = "players.csv";
    let mut iter = rows(source);
    let (_, header_line) = iter.next().ok_or(ImportError::Empty(FILE))?;
    let h = Header::new(header_line, FILE);

    let (c_club, c_name, c_pos) = (
        h.required("club_id")?,
        h.required("name")?,
        h.required("position")?,
    );
    let (c_nat, c_birth, c_num) = (
        h.optional("nationality"),
        h.optional("birth_year"),
        h.optional("number"),
    );

    let mut out = Vec::new();
    for (line, raw) in iter {
        let r = split_row(raw);
        let name = r.get(c_name).cloned().unwrap_or_default();
        if name.is_empty() {
            return Err(ImportError::BadRow { file: FILE, line, reason: "empty name".into() });
        }
        let broad_position = r
            .get(c_pos)
            .and_then(|v| BroadPosition::parse(v))
            .ok_or(ImportError::BadRow {
                file: FILE,
                line,
                reason: format!("unrecognised position for `{name}` (want GK/DF/MF/FW)"),
            })?;
        out.push(PlayerData {
            club_id: r.get(c_club).cloned().unwrap_or_default(),
            name,
            broad_position,
            nationality: field(&r, c_nat).unwrap_or("BRA").to_string(),
            birth_year: field(&r, c_birth).and_then(|v| v.parse().ok()),
            number: field(&r, c_num).and_then(|v| v.parse().ok()),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fictional throughout — the repository must never contain real club data,
    // including in fixtures.
    const STADIUMS: &str = "\
id,name,formal_name,city,state,capacity,opened,lat,lon
est_serrano,Arena Serrano,Estádio Municipal do Vale,Vale do Sol,SP,\"32,410\",1974,-23.55,-46.63
est_pindorama,Campo do Pindorama,,Pindorama,MG,8900,1951,,
";

    const CLUBS: &str = "\
# fictional sample
id,name,short_name,nickname,founded,city,state,stadium_id,division,reputation,primary_color,secondary_color,crest
uniao_serrano,União Serrano SC,Serrano,Os Verdes,1912,Vale do Sol,SP,est_serrano,0,15,#0B7A3B,#FFFFFF,crests/serrano.png
gremio_pindorama,Grêmio Pindorama,Pindorama,,1948,Pindorama,MG,est_pindorama,1,8,#C8102E,#111111,
";

    #[test]
fn dbg_header() {
    let s = "\
id,name,formal_name,city,state,capacity,opened,lat,lon
est_serrano,Arena Serrano,X,Vale,SP,\"32,410\",1974,-23.55,-46.63
";
    for (i,l) in s.lines().enumerate() { println!("{i}: {:?}", l); }
    println!("split: {:?}", super::split_row(s.lines().next().unwrap()));
}

    #[test]
    fn parses_a_complete_dataset() {
        let d = Dataset::parse(CLUBS, STADIUMS).unwrap();
        assert_eq!(d.clubs.len(), 2);
        assert_eq!(d.stadiums.len(), 2);

        let serrano = &d.clubs[0];
        assert_eq!(serrano.short_name, "Serrano");
        assert_eq!(serrano.nickname.as_deref(), Some("Os Verdes"));
        assert_eq!(serrano.reputation, 15);
        assert_eq!(serrano.crest.as_deref(), Some("crests/serrano.png"));
    }

    #[test]
    fn quoted_thousands_separators_parse() {
        let d = Dataset::parse(CLUBS, STADIUMS).unwrap();
        assert_eq!(d.stadium("est_serrano").unwrap().capacity, 32410);
    }

    #[test]
    fn formal_name_survives_a_sponsor_rename() {
        // Naming rights churn every few years, so the registered name has to be
        // stored separately and the import has to be re-runnable.
        let d = Dataset::parse(CLUBS, STADIUMS).unwrap();
        let s = d.stadium("est_serrano").unwrap();
        assert_eq!(s.name, "Arena Serrano");
        assert_eq!(s.formal_name.as_deref(), Some("Estádio Municipal do Vale"));
    }

    #[test]
    fn empty_optionals_become_none_not_empty_strings() {
        let d = Dataset::parse(CLUBS, STADIUMS).unwrap();
        assert_eq!(d.stadium("est_pindorama").unwrap().formal_name, None);
        assert_eq!(d.clubs[1].nickname, None);
        assert_eq!(d.clubs[1].crest, None);
    }

    #[test]
    fn short_name_defaults_to_name() {
        let csv = "id,name,stadium_id,division\nx,Clube Exemplo,est_serrano,0\n";
        let d = Dataset::parse(csv, STADIUMS).unwrap();
        assert_eq!(d.clubs[0].short_name, "Clube Exemplo");
    }

    #[test]
    fn column_order_does_not_matter() {
        let csv = "division,stadium_id,name,id\n0,est_serrano,Clube Exemplo,x\n";
        let d = Dataset::parse(csv, STADIUMS).unwrap();
        assert_eq!(d.clubs[0].name, "Clube Exemplo");
    }

    #[test]
    fn dangling_stadium_reference_is_caught_at_import() {
        let csv = "id,name,stadium_id,division\nx,Clube Exemplo,nope,0\n";
        assert!(matches!(
            Dataset::parse(csv, STADIUMS),
            Err(ImportError::UnknownStadium { .. })
        ));
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let csv = "id,name,stadium_id,division\nx,A,est_serrano,0\nx,B,est_serrano,0\n";
        assert!(matches!(
            Dataset::parse(csv, STADIUMS),
            Err(ImportError::DuplicateId { .. })
        ));
    }

    #[test]
    fn missing_required_column_names_the_column() {
        let csv = "id,name,division\nx,A,0\n";
        match Dataset::parse(csv, STADIUMS) {
            Err(ImportError::MissingColumn { column, .. }) => assert_eq!(column, "stadium_id"),
            other => panic!("expected MissingColumn, got {other:?}"),
        }
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let d = Dataset::parse(CLUBS, STADIUMS).unwrap();
        assert_eq!(d.clubs.len(), 2, "comment line was treated as data");
    }

    #[test]
    fn reputation_is_clamped_and_defaulted() {
        let csv = "id,name,stadium_id,division,reputation\na,A,est_serrano,0,99\nb,B,est_serrano,0,\n";
        let d = Dataset::parse(csv, STADIUMS).unwrap();
        assert_eq!(d.clubs[0].reputation, 20);
        assert_eq!(d.clubs[1].reputation, 10);
    }
}

#[cfg(test)]
mod player_tests {
    use super::*;

    const STADIUMS: &str = "id,name,city,state,capacity\nest_a,Arena A,Cidade A,SP,20000\n";
    const CLUBS: &str = "id,name,stadium_id,division\nclube_a,Clube A,est_a,0\nclube_b,Clube B,est_a,0\n";
    const PLAYERS: &str = "\
club_id,name,position,nationality,birth_year,number
clube_a,Jajá,FW,BRA,1998,7
clube_a,Leonel Picco,MF,ARG,1998,14
clube_a,Marcelo Rangel,GK,BRA,1988,88
clube_b,Matheus Alexandre,DF,BRA,1999,2
clube_ausente,Fantasma,DF,BRA,1990,3
";

    #[test]
    fn imports_players_and_attaches_them_to_clubs() {
        let d = Dataset::parse_with_players(CLUBS, STADIUMS, Some(PLAYERS)).unwrap();
        assert_eq!(d.players_of("clube_a").count(), 3);
        assert_eq!(d.players_of("clube_b").count(), 1);
    }

    #[test]
    fn players_for_unknown_clubs_are_dropped_not_rejected() {
        // A scrape may cover clubs since removed from clubs.csv. That should not
        // fail the whole import.
        let d = Dataset::parse_with_players(CLUBS, STADIUMS, Some(PLAYERS)).unwrap();
        assert_eq!(d.players.len(), 4, "the orphan row should be dropped");
        assert!(d.players.iter().all(|p| p.name != "Fantasma"));
    }

    #[test]
    fn accented_names_survive() {
        let d = Dataset::parse_with_players(CLUBS, STADIUMS, Some(PLAYERS)).unwrap();
        assert!(d.players.iter().any(|p| p.name == "Jajá"));
    }

    #[test]
    fn broad_positions_parse_and_reject() {
        assert_eq!(BroadPosition::parse("gk"), Some(BroadPosition::Goalkeeper));
        assert_eq!(BroadPosition::parse(" DF "), Some(BroadPosition::Defender));
        assert_eq!(BroadPosition::parse("zagueiro"), None);
    }

    #[test]
    fn unrecognised_position_names_the_player() {
        let bad = "club_id,name,position\nclube_a,Fulano,ZAG\n";
        match Dataset::parse_with_players(CLUBS, STADIUMS, Some(bad)) {
            Err(ImportError::BadRow { reason, .. }) => assert!(reason.contains("Fulano")),
            other => panic!("expected BadRow, got {other:?}"),
        }
    }

    #[test]
    fn players_are_optional() {
        let d = Dataset::parse(CLUBS, STADIUMS).unwrap();
        assert!(d.players.is_empty());
    }
}

#[cfg(test)]
mod lever_tests {
    use super::*;

    const STADIUMS: &str = "id,name,city,state,capacity\nest_a,Arena A,Cidade A,SP,20000\n";

    fn clubs(rep: u8, squad: &str) -> String {
        format!(
            "id,name,stadium_id,division,reputation,squad_strength\nclube_a,Clube A,est_a,0,{rep},{squad}\n"
        )
    }

    #[test]
    fn squad_strength_defaults_to_reputation() {
        let csv = "id,name,stadium_id,division,reputation\nclube_a,Clube A,est_a,0,15\n";
        let d = Dataset::parse(csv, STADIUMS).unwrap();
        assert_eq!(d.clubs[0].reputation, 15);
        assert_eq!(d.clubs[0].squad_strength, 15);
    }

    #[test]
    fn the_two_levers_are_independent() {
        let d = Dataset::parse(&clubs(18, "14"), STADIUMS).unwrap();
        assert_eq!(d.clubs[0].reputation, 18, "stature");
        assert_eq!(d.clubs[0].squad_strength, 14, "current quality");
    }

    #[test]
    fn editing_a_lever_changes_the_content_hash() {
        // The point of the hash: a seed no longer identifies a world once these
        // are editable before kickoff.
        let a = Dataset::parse(&clubs(18, "14"), STADIUMS).unwrap();
        let b = Dataset::parse(&clubs(18, "15"), STADIUMS).unwrap();
        let c = Dataset::parse(&clubs(17, "14"), STADIUMS).unwrap();
        assert_ne!(a.content_hash(), b.content_hash(), "squad edit not recorded");
        assert_ne!(a.content_hash(), c.content_hash(), "reputation edit not recorded");
    }

    #[test]
    fn presentation_edits_do_not_invalidate_a_save() {
        // Recolouring a club or renaming a ground must not make an existing save
        // look like it came from a different world.
        let base = "id,name,stadium_id,division,reputation,squad_strength,primary_color\nclube_a,Clube A,est_a,0,18,14,#000000\n";
        let recoloured = "id,name,stadium_id,division,reputation,squad_strength,primary_color\nclube_a,Clube A,est_a,0,18,14,#FF0000\n";
        let a = Dataset::parse(base, STADIUMS).unwrap();
        let b = Dataset::parse(recoloured, STADIUMS).unwrap();
        assert_eq!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn content_hash_is_stable_across_processes() {
        let d = Dataset::parse(&clubs(18, "14"), STADIUMS).unwrap();
        assert_eq!(d.content_hash(), 0x49db_fb72_5c96_e7ba, "dataset hashing drifted");
    }
}

#[cfg(test)]
mod position_tests {
    use super::*;

    #[test]
    fn specific_codes_are_unambiguous() {
        // The whole point: CB and LB are both "defenders", but one is a zagueiro
        // and the other a lateral. Only the source knows which.
        assert_eq!(BroadPosition::parse("CB"), Some(BroadPosition::CentreBack));
        assert_eq!(BroadPosition::parse("LB"), Some(BroadPosition::FullBack));
        assert_eq!(BroadPosition::parse("RB"), Some(BroadPosition::FullBack));
        assert_eq!(BroadPosition::parse("DM"), Some(BroadPosition::DefensiveMid));
        assert_eq!(BroadPosition::parse("AM"), Some(BroadPosition::AttackingMid));
        assert_eq!(BroadPosition::parse("RW"), Some(BroadPosition::Winger));
        assert_eq!(BroadPosition::parse("CF"), Some(BroadPosition::Striker));
    }

    #[test]
    fn broad_codes_still_parse_for_poorer_sources() {
        assert_eq!(BroadPosition::parse("DF"), Some(BroadPosition::Defender));
        assert_eq!(BroadPosition::parse("mf"), Some(BroadPosition::Midfielder));
        assert_eq!(BroadPosition::parse(" FW "), Some(BroadPosition::Forward));
    }

    #[test]
    fn unknown_codes_are_rejected_not_guessed() {
        assert_eq!(BroadPosition::parse("ZAG"), None);
        assert_eq!(BroadPosition::parse(""), None);
        assert_eq!(BroadPosition::parse("attacker"), None);
    }
}
