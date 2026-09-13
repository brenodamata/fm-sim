//! A Championship Manager 01/02 pastiche over generated worlds.
//!
//! Deliberately a *development* view, like `inspect`: it shows true attributes,
//! potential and temperament. The real interface (M10) renders everything
//! through the knowledge filter and never shows these. This one exists so you
//! can judge whether a squad reads like a football team, which is the M0
//! question and the one no metric answers.
//!
//! The thing worth taking from Championship Manager is density, not chrome: a
//! whole squad with twenty attributes on one screen, keyboard navigation, no
//! scrolling and no cards. That is the right answer for a game whose entire
//! interaction is reading tables and making comparisons.
//!
//! So the styling is a dark, monospaced data surface — numbers are tabular and
//! colour-graded by value, rows are tight, and chrome is nearly absent. Getting
//! the density right now means the eventual React frontend has something
//! concrete to match rather than a blank page.

use std::fmt::Write as _;

use sim_core::data::Dataset;
use sim_core::domain::attributes::Attribute;
use sim_core::domain::player::{Player, Position};
use sim_core::World;

pub fn render(world: &World, data: Option<&Dataset>) -> String {
    let year = world.current_year();
    let mut h = String::new();
    h.push_str(HEAD);

    // Title bar
    let _ = write!(
        h,
        r#"<div class=win>
  <div class=titlebar><span class=titletext>{}</span><span class=seed>seed {}</span></div>
  <div class=body>
    <div class=sidebar>
      <div class=sidehdr>Clubs</div>
      <div class=clublist id=clublist></div>
    </div>
    <div class=main>
      <div class=clubhdr id=clubhdr></div>
      <div class=tabs>
        <div class="tab sel" data-view=squad>Squad</div>
        <div class=tab data-view=attrs>Attributes</div>
        <div class=tab data-view=info>Club Information</div>
      </div>
      <div class=panel id=panel></div>
    </div>
  </div>
  <div class=statusbar><span id=status></span><span class=clock>{} clubs · {} players</span></div>
</div>"#,
        division_label(world),
        world.seed.to_hex(),
        world.clubs.len(),
        world.players.len()
    );

    // Data payload. Embedded rather than fetched so the file works from disk
    // with no server, which is the whole point of a throwaway dev view.
    h.push_str("<script>const DATA=");
    h.push_str(&payload(world, data, year));
    h.push_str(";\n");
    h.push_str(SCRIPT);
    h.push_str("</script></body></html>");
    h
}

fn division_label(world: &World) -> String {
    world
        .divisions
        .iter()
        .map(|d| d.name.clone())
        .collect::<Vec<_>>()
        .join(" / ")
}

fn payload(world: &World, data: Option<&Dataset>, year: i32) -> String {
    let mut clubs = String::from("[");
    for division in &world.divisions {
        // Ranked by squad quality so the list reads like a power order, which is
        // how you actually want to scan a league.
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

        for (strength, club) in ranked {
            let source = data.and_then(|d| d.clubs.iter().find(|c| c.name == club.name));
            let (short, city, colour, second, stadium, cap) = match (source, data) {
                (Some(c), Some(d)) => {
                    let st = d.stadium(&c.stadium_id);
                    (
                        c.short_name.clone(),
                        c.city.clone(),
                        c.primary_color.clone(),
                        c.secondary_color.clone(),
                        st.map(|s| s.name.clone()).unwrap_or_default(),
                        st.map(|s| s.capacity).unwrap_or(0),
                    )
                }
                _ => (
                    club.name.clone(),
                    String::new(),
                    "#000080".into(),
                    "#FFFFFF".into(),
                    String::new(),
                    0,
                ),
            };
            let squad_rating = source.map(|c| c.squad_strength).unwrap_or(club.reputation);

            let mut players = String::from("[");
            let mut squad: Vec<&Player> = world.squad_of(club.id).collect();
            squad.sort_by(|a, b| {
                a.position
                    .cmp(&b.position)
                    .then(b.coarse_ability().partial_cmp(&a.coarse_ability()).unwrap())
            });
            for p in squad {
                let attrs: Vec<String> = Attribute::ALL
                    .iter()
                    .map(|&a| p.attributes.get(a).to_string())
                    .collect();
                let _ = write!(
                    players,
                    r#"{{"n":{},"p":"{}","a":{},"ab":{:.1},"pot":{},"gk":{},"at":[{}]}},"#,
                    json_str(&p.name),
                    pos_abbrev(p.position),
                    p.age(year),
                    p.coarse_ability(),
                    p.hidden.potential,
                    p.position.is_keeper(),
                    attrs.join(",")
                );
            }
            if players.ends_with(',') {
                players.pop();
            }
            players.push(']');

            let _ = write!(
                clubs,
                r#"{{"name":{},"short":{},"div":{},"rep":{},"squad":{},"str":{:.1},"city":{},"col":"{}","col2":"{}","stad":{},"cap":{},"players":{}}},"#,
                json_str(&club.name),
                json_str(&short),
                club.division.0,
                club.reputation,
                squad_rating,
                strength,
                json_str(&city),
                colour,
                second,
                json_str(&stadium),
                cap,
                players
            );
        }
    }
    if clubs.ends_with(',') {
        clubs.pop();
    }
    clubs.push(']');

    let divisions: Vec<String> = world.divisions.iter().map(|d| json_str(&d.name)).collect();
    format!(
        r#"{{"divisions":[{}],"clubs":{}}}"#,
        divisions.join(","),
        clubs
    )
}

fn json_str(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn pos_abbrev(p: Position) -> &'static str {
    match p {
        Position::Goleiro => "GOL",
        Position::Zagueiro => "ZAG",
        Position::Lateral => "LAT",
        Position::Volante => "VOL",
        Position::Meia => "MEI",
        Position::Ponta => "PON",
        Position::Centroavante => "ATA",
    }
}

const HEAD: &str = r##"<!DOCTYPE html><html lang=en><head><meta charset=utf-8>
<meta name=viewport content="width=device-width,initial-scale=1">
<title>Football Manager</title><style>
/* Dense data surface. Numbers are tabular monospace and colour-graded by value,
   so a squad is scannable as a heat field rather than read row by row. */
:root{
  --bg:#0B0E14; --panel:#111620; --raise:#161C28; --line:#1E2634;
  --text:#DCE3EC; --dim:#78859A; --faint:#4A5568;
  --accent:#3FD07F; --accent-dim:#1F6B45; --warn:#E0785A;
  --row:20px;
}
*{box-sizing:border-box}
html,body{height:100%}
body{
  margin:0;background:var(--bg);color:var(--text);
  font:13px/1.4 system-ui,-apple-system,"Segoe UI",sans-serif;
  -webkit-font-smoothing:antialiased;
}
.num,td.n,th.n{font-family:ui-monospace,"SF Mono",Menlo,Consolas,monospace;font-variant-numeric:tabular-nums}

.win{height:100vh;display:flex;flex-direction:column}
.titlebar{
  display:flex;align-items:center;gap:10px;padding:9px 14px;
  border-bottom:1px solid var(--line);background:var(--panel);
}
.titletext{font-weight:600;letter-spacing:-.01em}
.titlebar .seed{color:var(--faint);font-size:11.5px;font-family:ui-monospace,Menlo,monospace;margin-left:auto}
.menubar{display:none}
.body{flex:1;display:flex;min-height:0}

.sidebar{width:230px;display:flex;flex-direction:column;border-right:1px solid var(--line);background:var(--panel)}
.sidehdr{
  padding:8px 14px;font-size:10.5px;font-weight:600;letter-spacing:.09em;
  text-transform:uppercase;color:var(--faint);
}
.clublist{flex:1;overflow-y:auto}
.divhdr{
  padding:6px 14px 4px;font-size:10.5px;font-weight:600;letter-spacing:.08em;
  text-transform:uppercase;color:var(--faint);position:sticky;top:0;background:var(--panel);
}
.clubrow{
  display:flex;align-items:center;gap:9px;padding:0 14px;height:var(--row);
  cursor:default;border-left:2px solid transparent;
}
.clubrow:hover{background:var(--raise)}
.clubrow.sel{background:var(--raise);border-left-color:var(--accent)}
.clubrow.sel span:not(.chip){color:#fff;font-weight:600}
.chip{width:8px;height:8px;flex:0 0 8px;border-radius:1px;border:1px solid rgba(255,255,255,.25)}
.clubrow .rt{
  margin-left:auto;color:var(--dim);font-size:11.5px;
  font-family:ui-monospace,Menlo,monospace;font-variant-numeric:tabular-nums;
}

.main{flex:1;display:flex;flex-direction:column;min-width:0}
.clubhdr{display:flex;align-items:center;gap:10px;padding:12px 18px 10px;font-size:16px;font-weight:600;letter-spacing:-.01em}
.clubhdr .sub{margin-left:auto;font-size:12px;font-weight:400;color:var(--dim)}
.tabs{display:flex;gap:2px;padding:0 18px;border-bottom:1px solid var(--line)}
.tab{
  padding:7px 2px;margin-right:20px;cursor:default;color:var(--dim);font-size:12.5px;
  border-bottom:2px solid transparent;margin-bottom:-1px;
}
.tab:hover{color:var(--text)}
.tab.sel{color:var(--text);font-weight:600;border-bottom-color:var(--accent)}
.panel{flex:1;overflow:auto}

table{border-collapse:collapse;width:100%}
th{
  position:sticky;top:0;z-index:1;background:var(--bg);
  text-align:left;padding:7px 10px;font-size:10.5px;font-weight:600;
  letter-spacing:.06em;text-transform:uppercase;color:var(--faint);
  border-bottom:1px solid var(--line);cursor:default;white-space:nowrap;
}
th.n,td.n{text-align:right}
th:hover{color:var(--text)}
td{padding:0 10px;height:var(--row);border-bottom:1px solid rgba(30,38,52,.5);white-space:nowrap;font-size:12.5px}
tr:hover td{background:var(--raise)}
tr.sel td{background:var(--raise);box-shadow:inset 2px 0 0 var(--accent)}
td.gk{color:var(--accent)}

/* Value grading — the point of the whole screen. */
td.v{font-family:ui-monospace,Menlo,monospace;font-variant-numeric:tabular-nums}
.v16{color:var(--accent);font-weight:600}
.v13{color:var(--text)}
.v10{color:var(--dim)}
.v7{color:var(--faint)}
.v1{color:#3A4352}
.hi{color:var(--accent);font-weight:600}
.lo{color:var(--faint)}
.up{color:var(--accent)}
.missed{color:var(--faint);font-style:italic}

.info{padding:20px 18px}
.info table{width:auto}
.info td{border:none;padding:5px 28px 5px 0;height:auto}
.info td.k{color:var(--faint);width:170px;font-size:11px;letter-spacing:.05em;text-transform:uppercase}

.statusbar{
  display:flex;justify-content:space-between;padding:7px 18px;
  border-top:1px solid var(--line);background:var(--panel);
  color:var(--faint);font-size:11.5px;
}
.statusbar .clock{font-family:ui-monospace,Menlo,monospace}
</style></head><body>"##;

const SCRIPT: &str = r##"
const ATTRS=["Fin","Pas","Cro","Tch","Dri","Tck","Hea","Dec","Otb","Pos","Cmp","Vis","Wor","Agg","Pac","Acc","Sta","Str","Agi","Jmp","Han","Ref","Cmd","Dis","1v1"];
const ATTR_FULL=["Finishing","Passing","Crossing","First touch","Dribbling","Tackling","Heading","Decisions","Off the ball","Positioning","Composure","Vision","Work rate","Aggression","Pace","Acceleration","Stamina","Strength","Agility","Jumping","Handling","Reflexes","Command of area","Distribution","One-on-ones"];
// Outfield players never have their keeping attributes checked, so showing all
// 25 columns for everyone would be 5 columns of noise on 24 of 26 rows.
const OUT_IDX=[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19];
const GK_IDX=[20,21,22,23,24,7,10,9,17,18,19];

let sel=0, view="squad", sortKey=null, sortDir=1;

// Five bands rather than a continuous ramp: the eye reads steps, and a
// continuous gradient makes 13 and 14 indistinguishable.
function grade(v){
  if(v==="")return "";
  return v>=16?"v16":v>=13?"v13":v>=10?"v10":v>=7?"v7":"v1";
}

function esc(s){return String(s).replace(/[&<>]/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;"}[c]))}

function buildClubList(){
  const el=document.getElementById("clublist");
  let html="", lastDiv=-1;
  DATA.clubs.forEach((c,i)=>{
    if(c.div!==lastDiv){ html+=`<div class=divhdr>${esc(DATA.divisions[c.div]||"Division")}</div>`; lastDiv=c.div; }
    html+=`<div class="clubrow${i===sel?" sel":""}" data-i="${i}">
      <span class=chip style="background:${c.col};border-color:${c.col2}"></span>
      <span>${esc(c.short)}</span><span class=rt>${c.str.toFixed(1)}</span></div>`;
  });
  el.innerHTML=html;
  el.querySelectorAll(".clubrow").forEach(r=>{
    r.onclick=()=>{ sel=+r.dataset.i; sortKey=null; render(); };
  });
}

function sortPlayers(list){
  if(sortKey===null) return list;
  const v=p=>{
    if(sortKey==="n")return p.n.toLowerCase();
    if(sortKey==="p")return p.p;
    if(typeof sortKey==="number")return p.at[sortKey];
    return p[sortKey];
  };
  return [...list].sort((a,b)=>{
    const x=v(a),y=v(b);
    if(x<y)return -1*sortDir; if(x>y)return 1*sortDir; return 0;
  });
}

function header(cols){
  return "<tr>"+cols.map(c=>
    `<th class="${c.n?"n":""}" data-k="${c.k}" title="${esc(c.t||c.label)}">${esc(c.label)}</th>`
  ).join("")+"</tr>";
}

function renderSquad(c){
  const cols=[{k:"n",label:"Name"},{k:"p",label:"Pos"},{k:"a",label:"Age",n:1},
              {k:"ab",label:"Ability",n:1},{k:"pot",label:"Potential",n:1,t:"True value — the game will never show this"}];
  let h="<table><thead>"+header(cols)+"</thead><tbody>";
  sortPlayers(c.players).forEach((p,i)=>{
    const gap=p.pot-p.ab;
    const note=p.a<=24&&gap>2?` <span class=up>+${gap.toFixed(0)}</span>`
              :gap>2?` <span class=missed>missed</span>`:"";
    h+=`<tr${i===0?" class=sel":""}><td>${esc(p.n)}</td><td class="${p.gk?"gk":""}">${p.p}</td>
        <td class="n v">${p.a}</td><td class="n v ${grade(Math.round(p.ab))}">${p.ab.toFixed(1)}</td>
        <td class="n v">${p.pot}${note}</td></tr>`;
  });
  return h+"</tbody></table>";
}

function renderAttrs(c){
  const gkView=c.players.filter(p=>p.gk).length&&
               c.players.every(p=>p.gk);
  const idx=gkView?GK_IDX:OUT_IDX;
  const cols=[{k:"n",label:"Name"},{k:"p",label:"Pos"},{k:"a",label:"Age",n:1}]
    .concat(idx.map(i=>({k:i,label:ATTRS[i],n:1,t:ATTR_FULL[i]})));
  let h="<table><thead>"+header(cols)+"</thead><tbody>";
  sortPlayers(c.players).forEach((p,r)=>{
    const use=p.gk?GK_IDX:OUT_IDX;
    h+=`<tr${r===0?" class=sel":""}><td>${esc(p.n)}</td><td class="${p.gk?"gk":""}">${p.p}</td><td class="n v">${p.a}</td>`;
    idx.forEach((col,k)=>{
      // Keepers and outfielders are shown on the same grid, so a keeper's row
      // maps its own attribute set onto the same columns.
      const src=p.gk?use[k]:col;
      const v=src===undefined?"":p.at[src];
      h+=`<td class="n v ${grade(v)}">${v}</td>`;
    });
    h+="</tr>";
  });
  return h+"</tbody></table>";
}

function renderInfo(c){
  const gap=c.squad-c.rep;
  const verdict=gap<=-2?"Living on history — stature exceeds the squad"
              :gap>=2?"Punching up — squad exceeds the stature"
              :"Stature and squad broadly aligned";
  return `<div class=info><table>
    <tr><td class=k>Full name</td><td>${esc(c.name)}</td></tr>
    <tr><td class=k>City</td><td>${esc(c.city||"—")}</td></tr>
    <tr><td class=k>Stadium</td><td>${esc(c.stad||"—")}${c.cap?" ("+c.cap.toLocaleString()+")":""}</td></tr>
    <tr><td class=k>Reputation</td><td>${c.rep} <span class=lo>— who will sign, sponsorship, expectations</span></td></tr>
    <tr><td class=k>Squad rating</td><td>${c.squad} <span class=lo>— pre-game lever, drives the generated squad</span></td></tr>
    <tr><td class=k>Generated squad</td><td>${c.str.toFixed(1)}</td></tr>
    <tr><td class=k>Assessment</td><td><b>${verdict}</b></td></tr>
  </table></div>`;
}

function render(){
  const c=DATA.clubs[sel];
  buildClubList();
  document.getElementById("clubhdr").innerHTML=
    `<span class=chip style="background:${c.col};border-color:${c.col2}"></span>
     <span>${esc(c.name)}</span>
     <span class=sub>${esc(c.stad||"")}${c.cap?" · "+c.cap.toLocaleString():""}${c.city?" · "+esc(c.city):""}</span>`;
  document.querySelectorAll(".tab").forEach(t=>{
    t.classList.toggle("sel",t.dataset.view===view);
    t.onclick=()=>{ view=t.dataset.view; sortKey=null; render(); };
  });
  const panel=document.getElementById("panel");
  panel.innerHTML = view==="squad"?renderSquad(c) : view==="attrs"?renderAttrs(c) : renderInfo(c);
  panel.querySelectorAll("th").forEach(th=>{
    th.onclick=()=>{
      const k=isNaN(+th.dataset.k)?th.dataset.k:+th.dataset.k;
      sortDir = sortKey===k ? -sortDir : (typeof k==="number"?-1:1);
      sortKey=k; render();
    };
  });
  panel.querySelectorAll("tbody tr").forEach(tr=>{
    tr.onclick=()=>{ panel.querySelectorAll("tbody tr").forEach(x=>x.classList.remove("sel")); tr.classList.add("sel"); };
  });
  document.getElementById("status").textContent=
    `${c.players.length} players · click a column to sort · ↑↓ to change club · true values shown (development view)`;
}

// Arrow keys move between clubs, as in the original.
document.addEventListener("keydown",e=>{
  if(e.key==="ArrowDown"&&sel<DATA.clubs.length-1){sel++;render();e.preventDefault();}
  if(e.key==="ArrowUp"&&sel>0){sel--;render();e.preventDefault();}
});

render();
"##;
