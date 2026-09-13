//! Development inspector: renders a generated world as browsable HTML.
//!
//! This is a **debug view and shows the truth** — every attribute, potential,
//! temperament and injury proneness. The game will never show this. Once the
//! knowledge model lands (M3), the player sees estimates with bands and the
//! hidden values stay hidden; this tool exists precisely so you can check the
//! generator against reality before any of that filtering is in the way.
//!
//! The M0 question it answers: does a generated squad read like a football team?

use std::fmt::Write as _;

use sim_core::data::Dataset;
use sim_core::domain::attributes::{Attribute, Group};
use sim_core::domain::player::{Player, Position};
use sim_core::World;

pub fn render(world: &World, data: Option<&Dataset>) -> String {
    let mut h = String::new();
    let year = world.current_year();

    h.push_str(HEAD);
    let _ = write!(
        h,
        "<header><h1>Generated world</h1><p class=sub>seed <code>{}</code> · ruleset {} · {} clubs · {} players · year {}</p>\
         <p class=warn><strong>Development view.</strong> Shows true values, including potential and temperament. \
         The game never shows these — the player sees estimates with confidence bands.</p></header>",
        world.seed.to_hex(),
        world.ruleset_version,
        world.clubs.len(),
        world.players.len(),
        year
    );

    for division in &world.divisions {
        // Order by strength so the table reads like a plausible pre-season
        // power ranking rather than generation order.
        let mut ranked: Vec<(f32, &sim_core::domain::club::Club)> = division
            .clubs
            .iter()
            .map(|&c| {
                let club = world.club(c);
                let vals: Vec<f32> = world.squad_of(c).map(|p| p.coarse_ability()).collect();
                (vals.iter().sum::<f32>() / vals.len() as f32, club)
            })
            .collect();
        ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

        let _ = write!(h, "<section><h2>{}</h2>", division.name);

        for (rank, (strength, club)) in ranked.iter().enumerate() {
            let mut squad: Vec<&Player> = world.squad_of(club.id).collect();
            squad.sort_by(|a, b| {
                a.position
                    .cmp(&b.position)
                    .then(b.coarse_ability().partial_cmp(&a.coarse_ability()).unwrap())
            });

            // Match the imported record by name — the engine deliberately does
            // not carry dataset ids, since club identity is presentation.
            let source = data.and_then(|d| d.clubs.iter().find(|c| c.name == club.name));
            let swatch = match source {
                Some(c) => format!(
                    "<span class=swatch style=\"background:{};border-color:{}\"></span>",
                    c.primary_color, c.secondary_color
                ),
                None => String::new(),
            };
            let venue = match (source, data) {
                (Some(c), Some(d)) => match d.stadium(&c.stadium_id) {
                    Some(s) => format!(
                        " · {} ({}) · {}",
                        s.name,
                        thousands(s.capacity),
                        c.city
                    ),
                    None => String::new(),
                },
                _ => String::new(),
            };
            let display = source.map(|c| c.short_name.clone()).unwrap_or_else(|| club.name.clone());

            let _ = write!(
                h,
                "<details><summary><span class=rank>{}</span>{} <span class=club>{}</span> \
                 <span class=meta>squad {:.1} · stature {}{}{}</span></summary>",
                rank + 1,
                swatch,
                display,
                strength,
                club.reputation,
                gap_badge(source),
                venue
            );

            h.push_str("<table><thead><tr><th>Player</th><th>Pos</th><th class=n>Age</th>\
                        <th class=n>Abil</th><th class=n>Pot</th><th>Profile</th><th>Key attributes</th></tr></thead><tbody>");

            for p in &squad {
                let age = p.age(year);
                let ability = p.coarse_ability();
                let pot = p.hidden.potential as f32;
                let headroom = pot - ability;
                let row_class = if age <= 21 && headroom > 2.0 {
                    " class=prospect"
                } else if age >= 33 {
                    " class=veteran"
                } else {
                    ""
                };

                let _ = write!(
                    h,
                    "<tr{}><td class=name>{}</td><td>{}</td><td class=n>{}</td>\
                     <td class=n>{:.1}</td><td class=n>{}</td><td>{}</td><td class=attrs>{}</td></tr>",
                    row_class,
                    p.name,
                    p.position.name(),
                    age,
                    ability,
                    bar(pot, headroom, age),
                    profile(p),
                    key_attributes(p)
                );
            }
            h.push_str("</tbody></table></details>");
        }
        h.push_str("</section>");
    }

    h.push_str("</body></html>");
    h
}

/// Potential with a headroom marker — but only where the window is still open.
///
/// A thirty-one-year-old with potential three above his ability has *missed* it,
/// not got upside: growth is age-gated, so the gap will never close. Showing
/// "+3" there would advertise a player who is already what he is going to be.
fn bar(potential: f32, headroom: f32, age: i32) -> String {
    let window_open = age <= 24;
    if headroom > 2.0 && window_open {
        format!("{:.0} <span class=up>+{:.0}</span>", potential, headroom)
    } else if headroom > 2.0 {
        format!("{:.0} <span class=missed title=\"window closed — never reached\">missed</span>", potential)
    } else {
        format!("{:.0}", potential)
    }
}

/// Group means, which is roughly how the game will summarise a player once
/// knowledge is coarse-banded.
fn profile(p: &Player) -> String {
    let groups: &[(Group, &str)] = if p.position.is_keeper() {
        &[
            (Group::Goalkeeping, "GK"),
            (Group::Mental, "Men"),
            (Group::Physical, "Phy"),
        ]
    } else {
        &[
            (Group::Technical, "Tec"),
            (Group::Mental, "Men"),
            (Group::Physical, "Phy"),
        ]
    };
    groups
        .iter()
        .map(|(g, label)| {
            let v = p.attributes.group_mean(*g);
            format!("<span class=g title=\"{}\">{} {:.0}</span>", g.name(), label, v)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The handful of attributes that actually matter for this position, so a squad
/// list is scannable rather than a wall of twenty-five numbers.
fn key_attributes(p: &Player) -> String {
    use Attribute::*;
    let keys: &[Attribute] = match p.position {
        Position::Goleiro => &[Reflexes, Handling, CommandOfArea, Distribution, OneOnOnes],
        Position::Zagueiro => &[Tackling, Heading, Strength, Positioning, Decisions],
        Position::Lateral => &[Pace, Stamina, Crossing, Tackling, WorkRate],
        Position::Volante => &[Tackling, Positioning, WorkRate, Passing, Decisions],
        Position::Meia => &[Passing, Vision, FirstTouch, Decisions, Dribbling],
        Position::Ponta => &[Dribbling, Pace, Acceleration, Crossing, Agility],
        Position::Centroavante => &[Finishing, OffTheBall, Composure, Heading, Strength],
    };
    keys.iter()
        .map(|&a| {
            let v = p.attributes.get(a);
            let cls = if v >= 15 {
                " hi"
            } else if v <= 7 {
                " lo"
            } else {
                ""
            };
            format!(
                "<span class=\"a{}\" title=\"{}\">{}<b>{}</b></span>",
                cls,
                a.name(),
                short(a),
                v
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Flags clubs where stature and squad diverge — the cases that make the two
/// fields worth having separately.
fn gap_badge(source: Option<&sim_core::data::ClubData>) -> String {
    let Some(c) = source else { return String::new() };
    let gap = c.squad_strength as i16 - c.reputation as i16;
    if gap >= 2 {
        format!(" <span class=over>punching up {:+}</span>", gap)
    } else if gap <= -2 {
        format!(" <span class=under>living on history {:+}</span>", gap)
    } else {
        String::new()
    }
}

fn thousands(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn short(a: Attribute) -> &'static str {
    use Attribute::*;
    match a {
        Finishing => "Fin",
        Passing => "Pas",
        Crossing => "Cro",
        FirstTouch => "Tch",
        Dribbling => "Dri",
        Tackling => "Tck",
        Heading => "Hea",
        Decisions => "Dec",
        OffTheBall => "Otb",
        Positioning => "Pos",
        Composure => "Cmp",
        Vision => "Vis",
        WorkRate => "Wor",
        Aggression => "Agg",
        Pace => "Pac",
        Acceleration => "Acc",
        Stamina => "Sta",
        Strength => "Str",
        Agility => "Agi",
        Jumping => "Jmp",
        Handling => "Han",
        Reflexes => "Ref",
        CommandOfArea => "Cmd",
        Distribution => "Dis",
        OneOnOnes => "1v1",
    }
}

const HEAD: &str = r#"<!DOCTYPE html><html lang=en><head><meta charset=utf-8>
<meta name=viewport content="width=device-width,initial-scale=1">
<title>Generated world</title><style>
:root{--paper:#EDEFEA;--ink:#1B2A24;--soft:#55635B;--rule:#C6CDC3;--pitch:#2E5E4E;--oxide:#8A3A2E;--surface:#F7F8F4}
*{box-sizing:border-box}
body{margin:0;background:var(--paper);color:var(--ink);font:15px/1.5 "Iowan Old Style",Charter,Georgia,serif;padding:32px 20px 80px;max-width:1100px;margin:0 auto}
header{border-bottom:2px solid var(--ink);padding-bottom:16px;margin-bottom:28px}
h1{font-size:30px;margin:0 0 8px;font-weight:600;letter-spacing:-.01em}
.sub{margin:0 0 12px;color:var(--soft);font-size:14px}
.warn{margin:0;padding:8px 12px;border-left:3px solid var(--oxide);background:var(--surface);font-size:13.5px;color:var(--soft);max-width:70ch}
h2{font-size:20px;margin:34px 0 10px;padding-bottom:6px;border-bottom:1px solid var(--rule);font-weight:600}
details{border-bottom:1px solid var(--rule)}
summary{cursor:pointer;padding:9px 4px;list-style:none;display:flex;align-items:baseline;gap:10px}
summary::-webkit-details-marker{display:none}
summary:hover{background:var(--surface)}
.rank{display:inline-block;min-width:22px;color:var(--soft);font-variant-numeric:tabular-nums;font-size:13px}
.club{font-weight:600}
.meta{color:var(--soft);font-size:13px;font-variant-numeric:tabular-nums}
table{border-collapse:collapse;width:100%;font-size:13.5px;margin:4px 0 14px;font-variant-numeric:tabular-nums}
th{text-align:left;font-weight:600;border-bottom:1.5px solid var(--ink);padding:5px 8px 5px 0;font-size:12px;color:var(--soft)}
td{padding:5px 8px 5px 0;border-bottom:1px solid var(--rule)}
th.n,td.n{text-align:right;padding-right:14px}
.name{font-weight:600}
tr.prospect{background:rgba(46,94,78,.07)}
tr.veteran{background:rgba(138,58,46,.05)}
.up{color:var(--pitch);font-weight:600;font-size:12px}
.missed{color:var(--soft);font-size:11px;font-style:italic}\n.swatch{display:inline-block;width:12px;height:12px;border-radius:2px;border:2px solid;vertical-align:middle;margin-right:2px}\n.over{color:#2E5E4E;font-weight:600;font-size:11.5px}\n.under{color:#8A3A2E;font-weight:600;font-size:11.5px}
.g{display:inline-block;margin-right:7px;color:var(--soft);font-size:12px}
.attrs .a{display:inline-block;margin-right:8px;color:var(--soft);font-size:11.5px}
.attrs .a b{font-weight:600;color:var(--ink);margin-left:2px;font-size:12.5px}
.attrs .a.hi b{color:var(--pitch)}
.attrs .a.lo b{color:var(--oxide)}
code{font-family:ui-monospace,Menlo,monospace;font-size:.9em;background:#E2E6DF;padding:1px 4px;border-radius:2px}
</style></head><body>"#;
