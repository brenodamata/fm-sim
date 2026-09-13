//! Name generation.
//!
//! No real club or player data lives in this repository (ADR-007). These are
//! generated names with a Brazilian distribution — including the single-name and
//! nickname forms the league actually uses, because a squad list full of
//! "Firstname Lastname" reads wrong immediately.

use rand::Rng;
use rand_chacha::ChaCha8Rng;

const FIRST_NAMES: &[&str] = &[
    "Anderson", "André", "Bruno", "Caio", "Carlos", "Cléber", "Daniel", "Danilo", "Diego",
    "Douglas", "Eduardo", "Emerson", "Everton", "Fábio", "Felipe", "Fernando", "Gabriel",
    "Geraldo", "Gustavo", "Henrique", "Igor", "Jefferson", "João", "Jonas", "Josué", "Juliano",
    "Leandro", "Lucas", "Luiz", "Marcelo", "Marcos", "Mateus", "Matheus", "Maurício", "Murilo",
    "Nelson", "Otávio", "Paulo", "Pedro", "Rafael", "Raul", "Renato", "Ricardo", "Roberto",
    "Rodrigo", "Ronaldo", "Samuel", "Sérgio", "Thiago", "Vinícius", "Wagner", "Wallace",
    "Washington", "Wesley", "William",
];

const SURNAMES: &[&str] = &[
    "Alves", "Almeida", "Araújo", "Azevedo", "Barbosa", "Barros", "Batista", "Cardoso",
    "Carvalho", "Castro", "Correia", "Costa", "Cunha", "Dias", "Duarte", "Ferreira", "Fernandes",
    "Fonseca", "Freitas", "Gomes", "Gonçalves", "Lima", "Lopes", "Machado", "Marques",
    "Martins", "Melo", "Mendes", "Moraes", "Moreira", "Nascimento", "Neves", "Nogueira",
    "Oliveira", "Pacheco", "Pereira", "Pinto", "Pires", "Ramos", "Ribeiro", "Rocha",
    "Rodrigues", "Sales", "Santos", "Silva", "Siqueira", "Soares", "Sousa", "Teixeira",
    "Vasconcelos", "Vieira",
];

/// Diminutives and nicknames — extremely common as the *only* name a Brazilian
/// player is known by.
const NICKNAME_SUFFIXES: &[&str] = &["inho", "ão", "zinho", "ito"];

const CLUB_PREFIXES: &[&str] = &[
    "Atlético", "Associação", "Clube", "Esporte Clube", "Grêmio", "Sociedade", "União",
    "Sport Club", "Real", "Náutico",
];

const CLUB_PLACES: &[&str] = &[
    "Araguaia", "Bandeirante", "Cabrália", "Caiçara", "Catarinense", "Guarani", "Ipiranga",
    "Itaquera", "Jaguaré", "Juazeiro", "Mantiqueira", "Marajó", "Mococa", "Paranaense",
    "Pindorama", "Piratininga", "Recife", "Rio Branco", "Serrano", "Sertão", "Tijuca",
    "Tocantins", "Tupã", "Uberaba", "Vale do Sol", "Vila Nova", "Xingu",
];

const CLUB_SUFFIXES: &[&str] = &["FC", "EC", "AC", "SC", ""];

/// A player name, in one of the forms Brazilian football actually uses.
pub fn player_name(rng: &mut ChaCha8Rng) -> String {
    let first = FIRST_NAMES[rng.gen_range(0..FIRST_NAMES.len())];
    // Weighted toward the mononym and nickname forms, which dominate in Brazil.
    match rng.gen_range(0..100) {
        0..=34 => first.to_string(),
        35..=54 => {
            let stem = trim_for_suffix(first);
            let suffix = NICKNAME_SUFFIXES[rng.gen_range(0..NICKNAME_SUFFIXES.len())];
            format!("{stem}{suffix}")
        }
        55..=79 => {
            let last = SURNAMES[rng.gen_range(0..SURNAMES.len())];
            format!("{first} {last}")
        }
        _ => SURNAMES[rng.gen_range(0..SURNAMES.len())].to_string(),
    }
}

/// Drops a trailing vowel so "Ronaldo" + "inho" reads as "Ronaldinho" rather
/// than "Ronaldoinho".
fn trim_for_suffix(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    match chars.last() {
        Some(c) if "aeiouAEIOU".contains(*c) => chars[..chars.len() - 1].iter().collect(),
        _ => name.to_string(),
    }
}

pub fn club_name(rng: &mut ChaCha8Rng) -> String {
    let prefix = CLUB_PREFIXES[rng.gen_range(0..CLUB_PREFIXES.len())];
    let place = CLUB_PLACES[rng.gen_range(0..CLUB_PLACES.len())];
    let suffix = CLUB_SUFFIXES[rng.gen_range(0..CLUB_SUFFIXES.len())];
    if suffix.is_empty() {
        format!("{prefix} {place}")
    } else {
        format!("{prefix} {place} {suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::determinism::{Domain, SeedPath, WorldSeed};

    #[test]
    fn names_are_non_empty_and_deterministic() {
        let path = || SeedPath::new(WorldSeed::new(5), Domain::PlayerGen).with_u64("n", 1);
        let mut a = path().rng();
        let mut b = path().rng();
        for _ in 0..200 {
            let na = player_name(&mut a);
            let nb = player_name(&mut b);
            assert!(!na.trim().is_empty());
            assert_eq!(na, nb);
        }
    }

    #[test]
    fn nickname_suffix_avoids_doubled_vowels() {
        assert_eq!(trim_for_suffix("Ronaldo"), "Ronald");
        assert_eq!(trim_for_suffix("Wagner"), "Wagner");
    }

    #[test]
    fn generates_a_plausible_spread_of_forms() {
        let mut rng = SeedPath::new(WorldSeed::new(11), Domain::PlayerGen).rng();
        let names: Vec<String> = (0..500).map(|_| player_name(&mut rng)).collect();
        let mononyms = names.iter().filter(|n| !n.contains(' ')).count();
        // Mononyms and nicknames should dominate, but not be everything.
        assert!(mononyms > 250, "too few mononyms: {mononyms}");
        assert!(mononyms < 450, "too many mononyms: {mononyms}");
    }

    #[test]
    fn club_names_are_non_empty() {
        let mut rng = SeedPath::new(WorldSeed::new(3), Domain::ClubGen).rng();
        for _ in 0..100 {
            assert!(!club_name(&mut rng).trim().is_empty());
        }
    }
}
