# Data directory

**Nothing real goes in this repository** (ADR-007). This directory holds the
format and a fictional example. Your actual dataset lives outside the repo, or
here but gitignored — `/data/*.csv` and `/data/crests/` are excluded.

## Files

- `stadiums.csv` — `id,name,formal_name,city,state,capacity,opened,lat,lon`
- `clubs.csv` — `id,name,short_name,nickname,founded,city,state,stadium_id,division,reputation,primary_color,secondary_color,crest`
- `crests/` — images, referenced by path from `clubs.csv`. Never read by `sim-core`.

`name` is the everyday name, `formal_name` the registered one. Stadium naming
rights change every few years, so keep both and re-run the import rather than
seeding once.

`reputation` is 1..=20 and hand-set — no public dataset encodes it usefully.

## Sourcing

| Field | Source |
|---|---|
| Club name, founding, city, state | Wikidata (CC0) |
| Stadium, capacity, coordinates | Wikidata, CBF's CNEF register |
| Colours | hand-curated; Wikidata coverage is patchy |
| Crests, mascots | club sites — copyrighted artwork, stays local |

Wikidata sketch (untested — check it in the query UI first):

```sparql
SELECT ?clubLabel ?venueLabel ?cap ?cityLabel WHERE {
  ?club wdt:P31/wdt:P279* wd:Q476028 ;
        wdt:P17 wd:Q155 ;
        wdt:P115 ?venue .
  ?venue wdt:P1083 ?cap .
  OPTIONAL { ?club wdt:P131 ?city }
  SERVICE wikibase:label { bd:serviceParam wikibase:language "pt,en" }
}
```

## Real players

Squads turn over completely twice a year, so players arrive via a scraper you
re-run rather than a dataset shipped once:

```sh
python3 scripts/scrape_squads.py --out /path/to/brasileirao-2026/players.csv
```

Standard library only. It reads each club's Wikipedia season article, parses the
`{{fs player}}` templates, and writes `club_id,name,position,nationality,birth_year,number`.

Three things it does deliberately:

- **Only the first squad block.** Later blocks on those pages are "out on loan"
  and "left during the season". Taking all of them turns a 30-man squad into 60.
- **Auto-corrects page titles.** Titles are inconsistent — `2026 CR Flamengo
  season` but `2026 Clube do Remo season` — and change yearly. A miss falls back
  to Wikipedia search and prints the correction so you can update the script.
- **Keeps positions broad.** Wikipedia records only GK/DF/MF/FW. The CSV stores
  that; the Rust importer specialises it into the seven Brazilian families using
  a seed derived from the player's own name, so a re-scrape never silently moves
  someone between zagueiro and lateral.

Failures are printed loudly at the end. A club that scrapes zero players falls
back to generated names, and you would not otherwise notice until the squad
looked wrong.

**Identity is imported, ability is not.** Name, age, position and nationality
come from the scrape; attributes and hidden values are generated from the club's
`squad_strength`. No public source encodes ability, and a stale roster then
degrades gracefully instead of breaking.

### Parser notes

Wikipedia's squad markup has three traps, each of which shipped as a bug before
being caught against a real article:

- `{{sort|0.01|}}` ends with `|}}`. A character-level scan reads `|}` and closes
  the table early. Table delimiters must be line-anchored.
- `{{birth date and age2|2026|12|31|1995|8|21}}` leads with the "age as of" date.
  The birth year is the *fourth* number; taking the first gives the season year
  for every player in the squad.
- Nationality lives in `{{flagicon|ARG}}`. Stripping templates — correct for
  names — deletes it.

`scripts/tests/fixture_roster.wiki` is fictional but structurally identical, and
`scripts/tests/test_parser.py` asserts all three. Run it after any parser change:

```sh
python3 scripts/tests/test_parser.py
```
