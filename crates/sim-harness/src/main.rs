//! The harness.
//!
//! Not a milestone — an instrument (ADR-014). It exists before the systems it
//! measures and grows alongside each one. Two modes share one core: a frozen
//! seed set for regression, and parameter sweeps for exploration.
//!
//! At M0 there is no match engine, so the only thing to measure is the world
//! generator. That is the point — the instrument works before there is anything
//! interesting to point it at.
//!
//! Output is CSV plus a JSON manifest. Parquet and DuckDB take over once runs
//! produce enough rows to justify a columnar format; the writer is isolated in
//! `emit` so that swap touches one function.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

mod inspect;
mod ui;

use sim_core::determinism::{WorldSeed, RULESET_VERSION};
use sim_core::domain::player::Position;
use sim_core::data::Dataset;
use sim_core::gen::{generate, generate_from_dataset, WorldSpec};
use sim_core::tuning::Tuning;
use sim_core::World;

const DEFAULT_TUNING: &str = include_str!("../../../config/tuning.toml");

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(dir) => println!("\nrun written to {}", dir.display()),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn run(args: &[String]) -> Result<PathBuf, String> {
    let mut worlds: u64 = 8;
    let mut base_seed: u64 = 1;
    let mut out_root = PathBuf::from("runs");
    let mut overrides: Vec<(String, String)> = Vec::new();
    let mut label = "explore".to_string();
    let mut inspect_path: Option<PathBuf> = None;
    let mut data_dir: Option<PathBuf> = None;
    let mut ui_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--worlds" => {
                worlds = next(args, &mut i, "--worlds")?.parse().map_err(|_| "bad --worlds")?
            }
            "--seed" => {
                base_seed = next(args, &mut i, "--seed")?.parse().map_err(|_| "bad --seed")?
            }
            "--out" => out_root = PathBuf::from(next(args, &mut i, "--out")?),
            "--label" => label = next(args, &mut i, "--label")?,
            "--inspect" => inspect_path = Some(PathBuf::from(next(args, &mut i, "--inspect")?)),
            "--data" => data_dir = Some(PathBuf::from(next(args, &mut i, "--data")?)),
            "--ui" => ui_path = Some(PathBuf::from(next(args, &mut i, "--ui")?)),
            "--set" => {
                let kv = next(args, &mut i, "--set")?;
                let (k, v) = kv.split_once('=').ok_or("--set expects key=value")?;
                overrides.push((k.to_string(), v.to_string()));
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other}")),
        }
        i += 1;
    }

    let mut tuning = Tuning::parse(DEFAULT_TUNING).map_err(|e| e.to_string())?;
    for (k, v) in &overrides {
        tuning = tuning.with_override(k, v);
    }
    let spec = WorldSpec::from_tuning(&tuning).map_err(|e| e.to_string())?;

    // Real club data lives outside the repository (ADR-007); without --data the
    // generator invents a fictional world instead.
    let dataset = match &data_dir {
        Some(dir) => {
            let clubs = fs::read_to_string(dir.join("clubs.csv"))
                .map_err(|e| format!("{}: {e}", dir.join("clubs.csv").display()))?;
            let stadiums = fs::read_to_string(dir.join("stadiums.csv"))
                .map_err(|e| format!("{}: {e}", dir.join("stadiums.csv").display()))?;
            // players.csv is optional: without it, squads are generated.
            let players = fs::read_to_string(dir.join("players.csv")).ok();
            let d = Dataset::parse_with_players(&clubs, &stadiums, players.as_deref())
                .map_err(|e| e.to_string())?;
            if d.players.is_empty() {
                println!(
                    "imported {} clubs, {} stadiums (no players.csv — squads generated)",
                    d.clubs.len(),
                    d.stadiums.len()
                );
            } else {
                println!(
                    "imported {} clubs, {} stadiums, {} real players",
                    d.clubs.len(),
                    d.stadiums.len(),
                    d.players.len()
                );
            }
            Some(d)
        }
        None => None,
    };

    println!("generating {worlds} worlds from base seed {base_seed}");

    let mut rows: Vec<ClubRow> = Vec::new();
    let mut fingerprints: Vec<(u64, u64)> = Vec::new();

    for n in 0..worlds {
        let seed = WorldSeed::new(base_seed.wrapping_add(n));
        let world = match &dataset {
            Some(d) => generate_from_dataset(seed, &spec, d),
            None => generate(seed, &spec),
        };
        fingerprints.push((seed.raw(), world.fingerprint()));
        collect(&world, &mut rows);

        // Inspect the first world only: the report is for eyeballing one world
        // in detail, not for surveying many.
        if n == 0 {
            if let Some(path) = &inspect_path {
                fs::write(path, inspect::render(&world, dataset.as_ref()))
                    .map_err(|e| e.to_string())?;
                println!("inspector written to {}", path.display());
            }
            if let Some(path) = &ui_path {
                fs::write(path, ui::render(&world, dataset.as_ref()))
                    .map_err(|e| e.to_string())?;
                println!("ui written to {}", path.display());
            }
        }
    }

    let summary = summarise(&rows);
    print_summary(&summary);

    let dir = out_root.join(format!("{}_{:016x}", label, tuning.config_hash()));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Recorded because reputation and squad strength are editable before a
    // game starts: without this, two runs from the same seed can differ and the
    // manifest would claim they are comparable.
    let dataset_hash = dataset.as_ref().map(|d| d.content_hash());
    emit(&dir, &rows, &fingerprints, &summary, &tuning, worlds, base_seed, dataset_hash)?;
    Ok(dir)
}

fn next(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i).cloned().ok_or(format!("{flag} needs a value"))
}

fn print_help() {
    println!(
        "sim-harness — headless runner

  --worlds N      number of worlds to generate (default 8)
  --seed N        base seed; world k uses seed N+k (default 1)
  --out DIR       output root (default ./runs)
  --label NAME    run label, used in the output directory name
  --set k=v       override a tuning key; repeatable
  --inspect FILE  write a browsable HTML report of the first world
  --ui FILE       write an interactive CM-style browser of the first world
  --data DIR      import clubs.csv and stadiums.csv from DIR instead of
                  generating fictional clubs

Examples:
  sim-harness --worlds 32 --set worldgen.club_spread=2.4 --label spread_sweep
  sim-harness --worlds 1 --seed 7 --inspect world.html"
    );
}

/// One row per club. Flat and wide so it maps onto a columnar format later
/// without reshaping.
struct ClubRow {
    seed: u64,
    division: u8,
    club: u32,
    squad_size: usize,
    reputation: u8,
    mean_ability: f32,
    best_ability: f32,
    mean_age: f32,
    under21: usize,
    over32: usize,
    keepers: usize,
    mean_potential: f32,
}

fn collect(world: &World, rows: &mut Vec<ClubRow>) {
    let year = world.current_year();
    for club in &world.clubs {
        let squad: Vec<_> = world.squad_of(club.id).collect();
        let abilities: Vec<f32> = squad.iter().map(|p| p.coarse_ability()).collect();
        let ages: Vec<f32> = squad.iter().map(|p| p.age(year) as f32).collect();

        rows.push(ClubRow {
            seed: world.seed.raw(),
            division: club.division.0,
            club: club.id.0,
            squad_size: squad.len(),
            reputation: club.reputation,
            mean_ability: mean(&abilities),
            best_ability: abilities.iter().cloned().fold(f32::MIN, f32::max),
            mean_age: mean(&ages),
            under21: ages.iter().filter(|&&a| a <= 21.0).count(),
            over32: ages.iter().filter(|&&a| a >= 32.0).count(),
            keepers: squad.iter().filter(|p| p.position == Position::Goleiro).count(),
            mean_potential: mean(
                &squad.iter().map(|p| p.hidden.potential as f32).collect::<Vec<_>>(),
            ),
        });
    }
}

fn mean(v: &[f32]) -> f32 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f32>() / v.len() as f32
    }
}

/// `BTreeMap` so the manifest is byte-stable regardless of insertion order.
type Summary = BTreeMap<String, f32>;

fn summarise(rows: &[ClubRow]) -> Summary {
    let mut s = Summary::new();
    if rows.is_empty() {
        return s;
    }
    let abilities: Vec<f32> = rows.iter().map(|r| r.mean_ability).collect();
    s.insert("clubs".into(), rows.len() as f32);
    s.insert("club_ability_mean".into(), mean(&abilities));
    s.insert("club_ability_min".into(), abilities.iter().cloned().fold(f32::MAX, f32::min));
    s.insert("club_ability_max".into(), abilities.iter().cloned().fold(f32::MIN, f32::max));
    s.insert("mean_age".into(), mean(&rows.iter().map(|r| r.mean_age).collect::<Vec<_>>()));
    s.insert(
        "under21_per_squad".into(),
        mean(&rows.iter().map(|r| r.under21 as f32).collect::<Vec<_>>()),
    );
    s.insert(
        "over32_per_squad".into(),
        mean(&rows.iter().map(|r| r.over32 as f32).collect::<Vec<_>>()),
    );
    s.insert(
        "keepers_per_squad".into(),
        mean(&rows.iter().map(|r| r.keepers as f32).collect::<Vec<_>>()),
    );

    let mut by_division: BTreeMap<u8, Vec<f32>> = BTreeMap::new();
    for r in rows {
        by_division.entry(r.division).or_default().push(r.mean_ability);
    }
    for (d, vals) in &by_division {
        s.insert(format!("division_{}_ability", d), mean(vals));
    }
    s
}

fn print_summary(s: &Summary) {
    println!();
    for (k, v) in s {
        println!("  {:<28} {:>8.2}", k, v);
    }
}

#[allow(clippy::too_many_arguments)]
fn emit(
    dir: &Path,
    rows: &[ClubRow],
    fingerprints: &[(u64, u64)],
    summary: &Summary,
    tuning: &Tuning,
    worlds: u64,
    base_seed: u64,
    dataset_hash: Option<u64>,
) -> Result<(), String> {
    let mut f = fs::File::create(dir.join("club_metrics.csv")).map_err(|e| e.to_string())?;
    writeln!(f, "seed,division,club,squad_size,reputation,mean_ability,best_ability,mean_age,under21,over32,keepers,mean_potential").map_err(|e| e.to_string())?;
    for r in rows {
        writeln!(
            f,
            "{},{},{},{},{},{:.4},{:.4},{:.4},{},{},{},{:.4}",
            r.seed, r.division, r.club, r.squad_size, r.reputation, r.mean_ability,
            r.best_ability, r.mean_age, r.under21, r.over32, r.keepers, r.mean_potential
        )
        .map_err(|e| e.to_string())?;
    }

    // Fingerprints make a run replayable: same manifest, same worlds — or the
    // ruleset changed, and the manifest says so.
    let mut fp = fs::File::create(dir.join("fingerprints.csv")).map_err(|e| e.to_string())?;
    writeln!(fp, "seed,fingerprint").map_err(|e| e.to_string())?;
    for (s, h) in fingerprints {
        writeln!(fp, "{},{}", s, h).map_err(|e| e.to_string())?;
    }

    // The manifest is what answers "why did this metric move?" six months later.
    let mut m = fs::File::create(dir.join("manifest.json")).map_err(|e| e.to_string())?;
    let metrics: Vec<String> = summary
        .iter()
        .map(|(k, v)| format!("    \"{}\": {:.4}", k, v))
        .collect();
    let keys: Vec<String> = tuning.keys().map(|k| format!("\"{}\"", k)).collect();
    write!(
        m,
        "{{\n  \"ruleset_version\": {},\n  \"git_sha\": \"{}\",\n  \"config_hash\": \"{:016x}\",\n  \"dataset_hash\": {},\n  \"worlds\": {},\n  \"base_seed\": {},\n  \"tuning_keys\": [{}],\n  \"metrics\": {{\n{}\n  }}\n}}\n",
        RULESET_VERSION,
        git_sha(),
        tuning.config_hash(),
        dataset_hash
            .map(|h| format!("\"{h:016x}\""))
            .unwrap_or_else(|| "null".to_string()),
        worlds,
        base_seed,
        keys.join(", "),
        metrics.join(",\n")
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn git_sha() -> String {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
