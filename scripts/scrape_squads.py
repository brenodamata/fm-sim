#!/usr/bin/env python3
"""Scrape Série A squads from Wikipedia into players.csv.

Re-run this each transfer window. Squads turn over completely twice a year, so
this is a refresh tool rather than a one-time seeding — which is exactly why it
exists as a script rather than as a dataset I hand you.

Source is the per-club season article's wikitext, parsed for {{fs player}}
templates. Wikitext rather than rendered HTML because the template is structured
and stable; the HTML tables are neither.

    python3 scrape_squads.py --out ../brasileirao-2026/players.csv

Requires only the standard library.

WHAT THIS DOES NOT DO
---------------------
Wikipedia records positions as GK/DF/MF/FW only. The game uses seven Brazilian
position families, so this script emits the broad position and the Rust importer
specialises it (DF becomes zagueiro or lateral, and so on) using a seeded,
squad-shape-constrained assignment. Keeping the CSV faithful to the source means
a re-scrape never silently changes someone's position.

Nothing here writes into the repository. Point --out at your data directory.
"""

import argparse
import csv
import os
import json
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

VERSION = "1.2"

API = "https://en.wikipedia.org/w/api.php"
# Wikipedia asks for a descriptive User-Agent. Generic ones get throttled hard,
# which is what produced a wall of HTTP 429s on the first real run.
# ASCII only: HTTP headers are latin-1, and a stray em-dash here breaks every
# request with a UnicodeEncodeError that looks nothing like the real cause.
UA = "football-sim-squad-scraper/1.2 (personal non-commercial use)"

# club_id must match clubs.csv. Page titles are the English Wikipedia season
# articles; adjust the year when you re-run for a new season.
DEFAULT_CLUBS = {
    "flamengo": "2026 CR Flamengo season",
    "palmeiras": "2026 Sociedade Esportiva Palmeiras season",
    "corinthians": "2026 Sport Club Corinthians Paulista season",
    "sao_paulo": "2026 São Paulo FC season",
    "atletico_mg": "2026 Clube Atlético Mineiro season",
    "cruzeiro": "2026 Cruzeiro Esporte Clube season",
    "gremio": "2026 Grêmio Foot-Ball Porto Alegrense season",
    "internacional": "2026 Sport Club Internacional season",
    "botafogo": "2026 Botafogo de Futebol e Regatas season",
    "fluminense": "2026 Fluminense FC season",
    "vasco": "2026 CR Vasco da Gama season",
    "bahia": "2026 Esporte Clube Bahia season",
    "santos": "2026 Santos FC season",
    "athletico_pr": "2026 Club Athletico Paranaense season",
    "vitoria": "2026 Esporte Clube Vitória season",
    "bragantino": "2026 Red Bull Bragantino season",
    "coritiba": "2026 Coritiba Foot Ball Club season",
    "mirassol": "2026 Mirassol Futebol Clube season",
    "chapecoense": "2026 Associação Chapecoense de Futebol season",
    "remo": "2026 Clube do Remo season",
}


def api_get(params, attempts=6, api=None):
    """GET the API, backing off on 429/503 rather than giving up.

    Wikipedia rate-limits aggressively. Without backoff a 20-club run fails
    around club six and the rest cascade into failures that look like missing
    articles but are really throttling.
    """
    url = f"{api or API}?{urllib.parse.urlencode(params)}"
    delay = 2.0
    for attempt in range(attempts):
        req = urllib.request.Request(url, headers={"User-Agent": UA})
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            if e.code in (429, 503) and attempt < attempts - 1:
                wait = delay * (2 ** attempt)
                print(f"    rate limited, waiting {wait:.0f}s", file=sys.stderr)
                time.sleep(wait)
                continue
            raise
    raise RuntimeError("unreachable")


def fetch_wikitext(title):
    """Raw wikitext for a page, or None if it does not exist."""
    params = {
        "action": "query",
        "prop": "revisions",
        "rvprop": "content",
        "rvslots": "main",
        "format": "json",
        "formatversion": "2",
        "titles": title,
        "redirects": "1",
    }
    data = api_get(params)
    pages = data.get("query", {}).get("pages", [])
    if not pages or pages[0].get("missing"):
        return None
    return pages[0]["revisions"][0]["slots"]["main"]["content"]


def search_title(club_name, year):
    """Find a club's season article by searching, not guessing.

    Season article titles are wildly inconsistent — "2026 CR Flamengo season",
    "2026 Clube do Remo season", "2026 Sport Club Internacional season" — and
    they change every year. Hardcoding them guarantees breakage, so this is the
    primary lookup rather than a fallback.

    The search is deliberately unquoted: quoted phrases fail whenever the article
    words the club name differently from clubs.csv, which is most of the time.
    """
    params = {
        "action": "query",
        "list": "search",
        "srsearch": f"{year} {club_name} season",
        "srlimit": "10",
        "format": "json",
        "formatversion": "2",
    }
    data = api_get(params)

    # Distinctive words from the club name, ignoring the generic ones that appear
    # in half the league ("clube", "futebol", "sport"...).
    generic = {"clube", "club", "futebol", "football", "sport", "de", "do", "da",
               "e", "associacao", "associação", "esporte", "esportiva", "regatas",
               "sociedade", "atletico", "atlético", "foot-ball", "fc", "ec", "ac", "sc"}
    words = [w for w in re.split(r"[\s.]+", club_name.lower()) if w and w not in generic]

    best, best_score = None, 0
    for hit in data.get("query", {}).get("search", []):
        title = hit["title"]
        low = title.lower()
        if str(year) not in title or "season" not in low:
            continue
        score = sum(1 for w in words if w in low)
        if score > best_score:
            best, best_score = title, score
    return best if best_score else None


def strip_links(value):
    """[[Matheus Alexandre]] -> Matheus Alexandre, [[A|B]] -> B."""
    value = re.sub(r"\[\[([^\]|]+)\|([^\]]+)\]\]", r"\2", value)
    value = re.sub(r"\[\[([^\]]+)\]\]", r"\1", value)
    value = re.sub(r"<[^>]+>", "", value)
    value = re.sub(r"\{\{[^}]*\}\}", "", value)
    return value.strip()


# Country names appear in full in {{Fs player|nat=Brazil}}, and as codes in
# {{flagicon|BRA}}. Both have to resolve to the same three letters.
COUNTRY_CODES = {
    "brazil": "BRA", "argentina": "ARG", "uruguay": "URU", "colombia": "COL",
    "paraguay": "PAR", "chile": "CHI", "ecuador": "ECU", "venezuela": "VEN",
    "peru": "PER", "bolivia": "BOL", "portugal": "POR", "spain": "ESP",
    "italy": "ITA", "france": "FRA", "england": "ENG", "germany": "GER",
    "netherlands": "NED", "belgium": "BEL", "croatia": "CRO", "serbia": "SRB",
    "mexico": "MEX", "united states": "USA", "japan": "JPN", "south korea": "KOR",
    "nigeria": "NGA", "ghana": "GHA", "cameroon": "CMR", "senegal": "SEN",
    "morocco": "MAR", "ivory coast": "CIV", "switzerland": "SUI",
    "austria": "AUT", "poland": "POL", "ukraine": "UKR", "russia": "RUS",
    "sweden": "SWE", "norway": "NOR", "denmark": "DEN", "cape verde": "CPV",
    "angola": "ANG", "guinea": "GUI", "panama": "PAN", "costa rica": "CRC",
}


def parse_nationality(value):
    """Nationality as a three-letter code.

    Handles {{flagicon|BRA}}, {{Fs player|nat=Brazil}} and a bare code. Stripping
    templates wholesale — which is right for names — deletes it entirely, and
    truncating a country name to three letters turns Brazil into BRA by luck and
    Uruguay into URU by luck but Netherlands into NET, which is wrong.
    """
    if not value:
        return None
    m = re.search(r"\{\{\s*flag[a-z]*\s*\|\s*([A-Za-z ]{2,})", value, re.I)
    raw = m.group(1) if m else strip_links(value)
    raw = re.sub(r"\s+", " ", raw).strip()
    if not raw:
        return None

    mapped = COUNTRY_CODES.get(raw.lower())
    if mapped:
        return mapped
    letters = re.sub(r"[^A-Za-z]", "", raw)
    if len(letters) == 3 and letters.isupper():
        return letters
    return letters[:3].upper() if letters else None


def link_target(value):
    """The article a wikilink points at, or None.

    Worth keeping rather than discarding: disambiguated football articles are
    routinely titled "Name (footballer, born 1987)", so the target carries a
    birth year for free, and it is the lookup key for fetching one when it does
    not.
    """
    m = re.search(r"\[\[([^\]|]+)", value)
    return m.group(1).strip() if m else None


def year_from_title(title):
    """Birth year out of "Cássio (footballer, born 1987)"."""
    if not title:
        return None
    m = re.search(r"born\s+(\d{4})", title, re.I)
    return int(m.group(1)) if m else None


def parse_birth_year(value):
    """Year of birth from the date templates these articles use.

    Two variants, and they differ in a way that quietly poisons every age:

        {{birth date and age|1995|8|21}}              -> birth year is 1st
        {{birth date and age2|2026|12|31|1995|8|21}}  -> birth year is 4th

    `age2` leads with the "age as of" date, so taking the first four-digit number
    returns the season year for every player in the squad.
    """
    m = re.search(r"\{\{\s*birth date and age\s*2\s*\|(.*?)\}\}", value, re.I | re.S)
    if m:
        nums = re.findall(r"\b(\d{4})\b", m.group(1))
        if len(nums) >= 2:
            return int(nums[1])

    m = re.search(r"\{\{\s*birth date[^|]*\|(.*?)\}\}", value, re.I | re.S)
    if m:
        nums = re.findall(r"\b(18\d{2}|19\d{2}|20\d{2})\b", m.group(1))
        if nums:
            return int(nums[0])

    m = re.search(r"\b(19\d{2}|20\d{2})\b", value)
    return int(m.group(1)) if m else None


def split_template_args(body):
    """Split on | at nesting depth zero.

    Both braces and square brackets count. Piped wiki-links are the reason:
    [[Jaja (footballer, born 1998)|Jaja]] contains a | that is not an argument
    separator, and splitting on it silently truncates the name.
    """
    parts, depth, cur = [], 0, ""
    i = 0
    while i < len(body):
        c = body[i]
        if body.startswith("{{", i) or body.startswith("[[", i):
            depth += 1
            cur += body[i : i + 2]
            i += 2
            continue
        if body.startswith("}}", i) or body.startswith("]]", i):
            depth -= 1
            cur += body[i : i + 2]
            i += 2
            continue
        if c == "|" and depth == 0:
            parts.append(cur)
            cur = ""
        else:
            cur += c
        i += 1
    parts.append(cur)
    return parts


# The specific codes Brazilian season articles actually use. Far richer than the
# GK/DF/MF/FW I originally assumed, which means almost no guessing is needed.
SPECIFIC_POSITIONS = {
    "GK", "RB", "CB", "LB", "RWB", "LWB", "SW",
    "DM", "CM", "AM", "LM", "RM",
    "RW", "LW", "CF", "SS", "ST", "FW", "DF", "MF",
}


def find_tables(wikitext):
    """Yield each top-level wikitable, respecting nesting.

    Line-anchored, because wikitext only treats `{|` and `|}` as table
    delimiters at the start of a line. Scanning the raw character stream finds
    `|}` inside templates such as {{sort|0.01|}} and closes the table three rows
    in, which looks exactly like a squad with three players in it.
    """
    tables, depth, start = [], 0, None
    lines = wikitext.split("\n")
    for idx, line in enumerate(lines):
        stripped = line.lstrip()
        if stripped.startswith("{|"):
            if depth == 0:
                start = idx
            depth += 1
        elif stripped.startswith("|}") and depth > 0:
            depth -= 1
            if depth == 0 and start is not None:
                tables.append("\n".join(lines[start : idx + 1]))
                start = None
    return tables


def parse_row_cells(row):
    """Split one wikitable row into cells, handling both || and newline styles."""
    cells = []
    for line in row.split("\n"):
        line = line.strip()
        if line.startswith("|-") or line.startswith("|}"):
            continue
        # Both markers appear as data cells. `! scope="row"|` is standard for a
        # row's header cell, which in squad tables is the player's name — so
        # skipping `!` lines drops exactly the column that matters.
        if not line.startswith("|") and not line.startswith("!"):
            continue
        line = line[1:]
        if "||" in line:
            parts = line.split("||")
        elif "!!" in line:
            parts = line.split("!!")
        else:
            parts = [line]
        for part in parts:
            # Drop cell attributes: style="..."|value
            if "|" in part and re.search(r'=\s*"', part.split("|")[0]):
                part = part.split("|", 1)[1]
            cells.append(part.strip())
    return cells


def header_columns(table):
    """Normalised header cell text, in column order.

    Three layouts have to work: all cells on one line separated by `!!`, one
    cell per line, and either of those before or after the first `|-`. Header
    cells also carry inline CSS and wrap their label in
    {{abbr|Pos.|Primary position}}.

    Collecting only the first `!` line — which this did originally — returns a
    single column for the one-cell-per-line layout, so the table never matches.
    """
    cols = []
    for line in table.split("\n")[1:]:
        stripped = line.strip()
        if not stripped:
            continue
        if stripped.startswith("|-"):
            # Header rows may continue after a separator; keep going until data.
            if cols:
                break
            continue
        if not stripped.startswith("!"):
            if cols:
                break
            continue
        # A full-width divider ("Goalkeepers") is not a header row.
        if "colspan" in stripped.lower() and "!!" not in stripped:
            if cols:
                break
            continue

        body = stripped.lstrip("!")
        parts = body.split("!!") if "!!" in body else [body]
        for part in parts:
            # Drop cell attributes: style="..."|Label, and the bare `|` some
            # articles leave behind when there are no attributes at all.
            if "|" in part and re.search(r'=\s*"', part.split("|")[0]):
                part = part.split("|", 1)[1]
            elif part.lstrip().startswith("|"):
                part = part.lstrip()[1:]
            # {{abbr|Pos.|Primary position}} carries the label in its first
            # argument; strip_links would delete the whole template.
            cols.append(normalise_label(part))
    return cols


BOLD_MARK = "'" * 3
ITALIC_MARK = "'" * 2


def normalise_label(text):
    """A header label, stripped of markup.

    Handles bold, {{abbr|...}} wrappers, wikilinks and inline CSS — all of which
    turn up in the same cell in practice.
    """
    part = text
    if "|" in part and re.search(r'=\s*"', part.split("|")[0]):
        part = part.split("|", 1)[1]
    elif part.lstrip().startswith("|"):
        part = part.lstrip()[1:]
    abbr = re.search(r"\{\{\s*abbr\s*\|\s*([^|}]+)", part, re.I)
    label = abbr.group(1) if abbr else strip_links(part)
    label = label.replace(BOLD_MARK, "").replace(ITALIC_MARK, "")
    return re.sub(r"\s+", " ", label).strip().lower().rstrip(".")


def find_header(table):
    """Column labels for a table, trying both header conventions.

    Most articles mark header cells with `!`. Some use `|` with bold text, which
    is indistinguishable from a data row by marker alone — so those are found by
    trying the first few rows and keeping whichever maps to real columns.
    """
    cols = header_columns(table)
    if map_columns(cols):
        return cols
    for row in table.split("|-")[:4]:
        candidate = [normalise_label(c) for c in parse_row_cells(row)]
        if map_columns(candidate):
            return candidate
    return cols


def map_columns(cols):
    """Locate the columns we need, by name.

    Reading by fixed position is what produced squads whose 'name' column held
    appearance counts and whose 'nationality' held three letters of somebody's
    surname: every table has a different column order, so a positional read of
    the wrong table yields confident nonsense instead of failing.

    Returns None unless a date-of-birth column is present. That column is the
    one reliable marker of a roster: transfer, appearance and goalscorer tables
    never carry it.
    """
    idx = {}
    for i, c in enumerate(cols):
        if "pos" in c and "position" not in idx.values() and "pos" not in idx:
            idx.setdefault("pos", i)
        elif c in ("name", "player") or c.endswith(" name"):
            idx.setdefault("name", i)
        elif "nat" in c or c == "country":
            idx.setdefault("nat", i)
        elif "birth" in c or "born" in c or c in ("age", "dob", "d.o.b"):
            idx.setdefault("dob", i)
        elif c in ("no", "number", "squad number", "#"):
            idx.setdefault("no", i)

    if "dob" not in idx or "name" not in idx or "pos" not in idx:
        return None
    return idx


def extract_squad_table(wikitext):
    """Parse the roster wikitable, mapping columns by header rather than index."""
    best = None
    for table in find_tables(wikitext):
        cols = find_header(table)
        if not cols:
            continue
        idx = map_columns(cols)
        if idx is None:
            continue

        players = []
        for row in table.split("|-")[1:]:
            cells = parse_row_cells(row)
            if len(cells) <= max(idx.values()):
                continue
            pos = strip_links(cells[idx["pos"]]).strip().upper()
            pos = re.sub(r"[^A-Z]", "", pos)
            if pos not in SPECIFIC_POSITIONS:
                continue
            name = strip_links(cells[idx["name"]])
            if not name or len(name) < 2:
                continue
            raw_name = cells[idx["name"]]
            title = link_target(raw_name)
            players.append({
                "name": name,
                "position": pos,
                "nationality": (parse_nationality(cells[idx["nat"]])
                                if "nat" in idx else None) or "BRA",
                "birth_year": (parse_birth_year(cells[idx["dob"]])
                               or year_from_title(title) or ""),
                "number": re.sub(r"[^0-9]", "", cells[idx["no"]]) if "no" in idx else "",
                "_title": title or "",
            })

        if players and (best is None or len(players) > len(best)):
            best = players
    return best or []


def extract_squad(wikitext):
    """Players from the FIRST {{fs start}}..{{fs end}} block.

    Later blocks on these pages are 'out on loan' and 'left during the season',
    which are not the current squad. Taking only the first block is the
    difference between a 30-man squad and a 60-man one.
    """
    # Most articles use a plain table; a minority use {{fs player}}. Try the
    # table first because it carries specific positions (CB, DM, RW) rather than
    # the four broad ones, which removes most of the guesswork downstream.
    table = extract_squad_table(wikitext)
    if table:
        return table

    # Case matters here: articles use {{Fs start}} and {{fs start}}
    # interchangeably, and a case-sensitive search silently skips half of them.
    # {{Fs start}}, {{fs start}} and {{Football squad start}} are all in use.
    m_start = re.search(r"\{\{\s*(?:fs|football squad)\s*start", wikitext, re.I)
    if not m_start:
        return []
    m_end = re.search(r"\{\{\s*(?:fs|football squad)\s*end", wikitext[m_start.start():], re.I)
    block = wikitext[m_start.start(): m_start.start() + (m_end.start() if m_end else len(wikitext))]

    players = []
    for m in re.finditer(
        r"\{\{\s*(?:fs|football squad)\s*player\s*\|(.*?)\}\}\s*(?=\{\{|\n)",
        block, re.S | re.I):
        args = {}
        for part in split_template_args(m.group(1)):
            if "=" not in part:
                continue
            k, v = part.split("=", 1)
            args[k.strip().lower()] = v.strip()

        name = strip_links(args.get("name", ""))
        pos = re.sub(r"[^A-Z]", "", args.get("pos", "").strip().upper())
        if not name or pos not in SPECIFIC_POSITIONS:
            continue

        title = link_target(args.get("name", ""))
        players.append(
            {
                "name": name,
                "position": pos,
                # nat= may be a full country name ("Brazil") or a code.
                "nationality": parse_nationality(args.get("nat", "")) or "BRA",
                # {{Fs player}} carries no date of birth at all, which is why
                # whole clubs came back with none. The link target often does.
                "birth_year": (parse_birth_year(args.get("age", ""))
                               or year_from_title(title) or ""),
                "number": args.get("no", "").strip(),
                "_title": title or "",
            }
        )
    return players


WIKIDATA_API = "https://www.wikidata.org/w/api.php"


def fill_birth_years(rows, delay=1.0):
    """Fill missing birth years from Wikidata, in batches.

    {{Fs player}} carries no date of birth, so entire clubs arrive without one.
    Wikidata keeps it as P569 and can be queried by English Wikipedia article
    title, fifty at a time — so roughly 250 unknowns cost five requests rather
    than 250 page fetches.

    Anything that cannot be resolved is left blank rather than guessed. A wrong
    age is worse than a missing one: the game can generate a plausible age, but
    it cannot detect a confidently wrong one.
    """
    unknown = [r for r in rows if not r["birth_year"] and r.get("_title")]
    if not unknown:
        return 0

    by_title = {}
    for r in unknown:
        by_title.setdefault(r["_title"], []).append(r)
    titles = list(by_title)
    print(f"\nlooking up {len(titles)} birth dates on Wikidata", file=sys.stderr)

    filled = 0
    for i in range(0, len(titles), 50):
        batch = titles[i : i + 50]
        params = {
            "action": "wbgetentities",
            "sites": "enwiki",
            "titles": "|".join(batch),
            # sitelinks is required, not optional: without it the response
            # cannot be matched back to the article that was asked for, and the
            # only alternative is pairing by dict order, which silently
            # mismatches. That is what limited the first version to 49 of 221.
            "props": "claims|sitelinks",
            "format": "json",
        }
        try:
            data = api_get(params, api=WIKIDATA_API)
        except Exception as e:
            print(f"  batch {i // 50 + 1} failed: {e}", file=sys.stderr)
            continue

        for ent in (data.get("entities") or {}).values():
            if not isinstance(ent, dict):
                continue
            title = ((ent.get("sitelinks") or {}).get("enwiki") or {}).get("title")
            if not title or title not in by_title:
                continue
            p569 = (ent.get("claims") or {}).get("P569") or []
            if not p569:
                continue
            iso = (
                p569[0].get("mainsnak", {}).get("datavalue", {}).get("value", {})
            ).get("time", "")
            m = re.match(r"[+-](\d{4})", iso)
            if not m:
                continue
            year = int(m.group(1))
            # Sanity bound: a birth year outside this range means the entity is
            # not a current footballer and the match is wrong.
            if not 1960 <= year <= 2012:
                continue
            for r in by_title[title]:
                r["birth_year"] = year
                filled += 1

        time.sleep(delay)

    print(f"filled {filled} birth years", file=sys.stderr)
    return filled


def _check_user_agent():
    """HTTP headers are latin-1. A non-ASCII character in UA breaks every
    request with an error that points nowhere near the cause."""
    try:
        UA.encode("latin-1")
    except UnicodeEncodeError as e:
        raise SystemExit(f"UA contains a non-latin-1 character: {e}")


def die(*lines):
    """Fail with an explanation rather than a traceback.

    A stack trace is the right output for a bug and the wrong one for a path
    that does not exist.
    """
    for line in lines:
        print(line, file=sys.stderr)
    sys.exit(1)


def main():
    _check_user_agent()
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", default="players.csv")
    ap.add_argument("--clubs", help="JSON file of {club_id: wikipedia title}, overrides search")
    ap.add_argument("--clubs-csv", help="clubs.csv to read club ids and names from")
    ap.add_argument("--delay", type=float, default=2.0, help="seconds between requests")
    ap.add_argument("--year", type=int, default=2026, help="season year, used by title search")
    ap.add_argument("--no-ages", action="store_true",
                    help="skip the Wikidata birth-date lookup")
    ap.add_argument("--dry-run", action="store_true", help="resolve titles and report, write nothing")
    ap.add_argument("--version", action="version", version=f"scrape_squads {VERSION}")
    ap.add_argument("--dump-failed", metavar="DIR", nargs="?", const="wikidumps",
                    help="save raw wikitext for every club that fails to parse, "
                         "so all remaining layouts can be diagnosed in one pass")
    ap.add_argument("--dump-wikitext", metavar="CLUB_ID",
                    help="save one club's raw wikitext to <club_id>.wiki and exit; "
                         "use this if parsing fails so the markup can be inspected")
    ap.add_argument("--cache", default=None,
                    help="JSON file of resolved titles; written on success, reused on re-runs")
    args = ap.parse_args()

    # Prefer clubs.csv: it is the list the game actually uses, so the scrape can
    # never drift out of step with the dataset.
    clubs = None
    csv_path = args.clubs_csv
    if not csv_path and not args.clubs:
        guess = os.path.join(os.path.dirname(args.out) or ".", "clubs.csv")
        if os.path.exists(guess):
            csv_path = guess

    if args.clubs:
        with open(args.clubs, encoding="utf-8") as f:
            clubs = {k: ("title", v) for k, v in json.load(f).items()}
    elif csv_path:
        if not os.path.exists(csv_path):
            die(
                f"clubs.csv not found at {csv_path!r}",
                "That path is relative to where you are now:",
                f"  {os.getcwd()}",
                "",
                "Find your dataset:",
                "  find ~ -name clubs.csv -maxdepth 5 2>/dev/null",
                "",
                "Then pass its directory, or use --clubs-csv with the full path.",
            )
        with open(csv_path, encoding="utf-8") as f:
            reader = csv.DictReader(r for r in f if not r.startswith("#"))
            # Keep the short name as an alternate search term: "Santos FC"
            # finds an article that "Santos Futebol Clube" does not.
            clubs = {}
            for r in reader:
                if not r.get("id"):
                    continue
                names = [r["name"]]
                short = (r.get("short_name") or "").strip()
                if short and short.lower() != r["name"].lower():
                    names.append(short)
                clubs[r["id"]] = ("search", names)
        if not clubs:
            die(f"{csv_path} has no rows with an `id` column")
        print(f"reading {len(clubs)} clubs from {csv_path}", file=sys.stderr)
    else:
        clubs = {k: ("title", v) for k, v in DEFAULT_CLUBS.items()}

    # Create the output directory rather than failing after all the network work.
    if not args.dry_run:
        os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)

    cache_path = args.cache or os.path.join(
        os.path.dirname(os.path.abspath(args.out)), "wiki_titles.json"
    )
    title_cache = {}
    if os.path.exists(cache_path):
        try:
            with open(cache_path, encoding="utf-8") as f:
                title_cache = json.load(f)
            print(f"reusing {len(title_cache)} cached titles from {cache_path}", file=sys.stderr)
        except Exception:
            title_cache = {}

    if args.dump_wikitext:
        entry = clubs.get(args.dump_wikitext)
        if not entry:
            die(f"{args.dump_wikitext!r} is not in the club list")
        mode, value = entry
        title = value if mode == "title" else search_title(value, args.year)
        if not title:
            die(f"no season article found for {value!r}")
        text = fetch_wikitext(title)
        if text is None:
            die(f"no such page: {title}")
        path = f"{args.dump_wikitext}.wiki"
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)
        print(f"wrote {len(text)} chars of {title!r} to {path}", file=sys.stderr)
        return

    rows, failures, resolved, dumped = [], [], [], []
    for club_id, (mode, value) in clubs.items():
        title, text = value, None
        try:
            if mode == "search":
                # A cached title halves the requests, and search is the endpoint
                # Wikipedia throttles hardest. Re-runs should not re-search.
                title = title_cache.get(club_id)
                if not title:
                    names = value if isinstance(value, list) else [value]
                    for candidate in names:
                        title = search_title(candidate, args.year)
                        if title:
                            break
                        time.sleep(args.delay)
                    if not title:
                        failures.append(
                            (club_id, f"no season article found for {names!r}"))
                        continue
                    title_cache[club_id] = title
                    time.sleep(args.delay)
            text = fetch_wikitext(title)
        except Exception as e:  # network, rate limit, anything
            failures.append((club_id, f"{title}: {e}"))
            continue

        if text is None:
            failures.append((club_id, f"no such page: {title}"))
            continue

        squad = extract_squad(text)

        if not squad and mode == "search":
            # Many season articles carry only transfers, fixtures and results —
            # no squad section at all. The club's own article is where Wikipedia
            # keeps "Current squad" in that case.
            names = value if isinstance(value, list) else [value]
            for candidate in names:
                try:
                    club_text = fetch_wikitext(candidate)
                except Exception:
                    club_text = None
                if club_text:
                    fallback = extract_squad(club_text)
                    if fallback:
                        squad = fallback
                        title = f"{candidate} (club article)"
                        break
                time.sleep(args.delay)

        if not squad:
            failures.append((club_id, f"no squad found in {title!r} or the club article"))
            # Keep the source of anything we could not parse. Diagnosing a
            # layout needs the markup, and re-fetching later costs another
            # round of rate limiting.
            if args.dump_failed:
                os.makedirs(args.dump_failed, exist_ok=True)
                path = os.path.join(args.dump_failed, f"{club_id}.wiki")
                with open(path, "w", encoding="utf-8") as fh:
                    fh.write(text)
                dumped.append(path)
            continue
        resolved.append((club_id, title))

        for p in squad:
            rows.append({"club_id": club_id, **p})
        warn = "  <-- suspiciously small" if len(squad) < 15 else ""
        print(f"  {club_id:<16} {len(squad):>3} players   {title}{warn}", file=sys.stderr)
        time.sleep(args.delay)

    # Persist resolved titles even on partial failure: the next run then costs
    # one request per club instead of two.
    if title_cache:
        try:
            with open(cache_path, "w", encoding="utf-8") as f:
                json.dump(title_cache, f, ensure_ascii=False, indent=2, sort_keys=True)
            print(f"cached {len(title_cache)} titles to {cache_path}", file=sys.stderr)
        except Exception as e:
            print(f"could not write title cache: {e}", file=sys.stderr)

    # Summary FIRST. A write failure must never swallow the reason clubs
    # produced nothing — which is exactly what happened the first time this ran.
    ok = len(clubs) - len(failures)
    print(f"\n{ok}/{len(clubs)} clubs resolved, {len(rows)} players", file=sys.stderr)

    if failures:
        # Loud on purpose: a club that silently scraped zero players falls back
        # to generated names, and you would not notice until the squad looked
        # wrong weeks later.
        print(f"\n{len(failures)} club(s) FAILED — these fall back to generated players:", file=sys.stderr)
        for club_id, reason in failures:
            print(f"  {club_id}: {reason}", file=sys.stderr)
        if dumped:
            print(f"\nwrote {len(dumped)} wikitext dumps to {args.dump_failed}/", file=sys.stderr)
        else:
            print("\nRe-run with --dump-failed to save the markup of everything", file=sys.stderr)
            print("that failed, so the layouts can be diagnosed in one pass.", file=sys.stderr)

    if args.dry_run:
        print("\ndry run — nothing written", file=sys.stderr)
        return

    if not rows:
        die("\nNo players scraped; refusing to overwrite a good players.csv with an empty one.")

    if not args.no_ages:
        fill_birth_years(rows, delay=args.delay / 2)
        missing = sum(1 for r in rows if not r["birth_year"])
        if missing:
            print(f"{missing} players still have no birth year", file=sys.stderr)

    for r in rows:
        r.pop("_title", None)

    with open(args.out, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(
            f, fieldnames=["club_id", "name", "position", "nationality", "birth_year", "number"]
        )
        w.writeheader()
        w.writerows(rows)
    print(f"\nwrote {len(rows)} players to {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
