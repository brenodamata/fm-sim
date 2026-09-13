#!/usr/bin/env python3
"""Parser regression tests.

The fixture is fictional but structurally identical to the real articles. Every
assertion here corresponds to a bug that shipped, and each one needed the real
markup to find — three rounds of testing against invented fixtures found none of
them.

    python3 scripts/tests/test_parser.py
"""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from scrape_squads import (
    extract_squad,
    find_tables,
    header_columns,
    map_columns,
    parse_birth_year,
    parse_nationality,
)

HERE = os.path.dirname(os.path.abspath(__file__))


def load(name):
    with open(os.path.join(HERE, name), encoding="utf-8") as f:
        return f.read()


def test_sort_template_does_not_truncate_the_table():
    # {{sort|0.01|}} ends with |}} — a raw scan reads `|}` and closes the table
    # two rows in, which looks like a squad with two players in it.
    squad = extract_squad(load("fixture_roster.wiki"))
    assert len(squad) == 5, f"expected 5 players, got {len(squad)}"


def test_birth_year_uses_age2_fourth_parameter():
    # {{birth date and age2|2026|12|31|1995|8|21}} leads with the "as of" date.
    # Taking the first four-digit number gives the season year for everyone.
    squad = extract_squad(load("fixture_roster.wiki"))
    by_name = {p["name"]: p for p in squad}
    assert by_name["Fulano Primeiro"]["birth_year"] == 1995
    assert by_name["Sicrano Segundo"]["birth_year"] == 2004
    assert all(p["birth_year"] != 2026 for p in squad), "season year leaked in"


def test_nationality_survives_flagicon():
    # The flag template *is* the nationality; stripping templates loses it.
    squad = extract_squad(load("fixture_roster.wiki"))
    by_name = {p["name"]: p for p in squad}
    assert by_name["Fulano Primeiro"]["nationality"] == "ARG"
    assert by_name["Beltrano Terceiro"]["nationality"] == "URU"
    assert by_name["Quarto Jogador"]["nationality"] == "BRA"


def test_positions_are_specific_not_broad():
    squad = extract_squad(load("fixture_roster.wiki"))
    assert {p["position"] for p in squad} == {"GK", "RB", "CB", "CF"}


def test_piped_links_resolve_to_display_name():
    squad = extract_squad(load("fixture_roster.wiki"))
    names = {p["name"] for p in squad}
    assert "Quinto Atacante" in names, "piped link kept the disambiguator"
    assert not any("(" in n for n in names), f"unresolved link in {names}"


def test_transfers_table_is_not_mistaken_for_the_squad():
    # A transfers table has Pos. and Player columns and appears earlier in the
    # article. Taking the first table that parses returns a squad made entirely
    # of January signings — which is how Palmeiras came back with nine players.
    squad = extract_squad(load("fixture_roster.wiki"))
    names = {p["name"] for p in squad}
    assert "Sexto Zagueiro" not in names, "picked up the transfers table"
    assert len(squad) == 5, f"expected the 5-man roster, got {len(squad)}"


def test_appearances_table_is_ignored():
    # Same columns, different meaning. Taking it would double the squad.
    squad = extract_squad(load("fixture_roster.wiki"))
    assert sum(1 for p in squad if p["name"] == "Fulano Primeiro") == 1


def test_abbr_wrapped_headers_are_detected():
    tables = find_tables(load("fixture_roster.wiki"))
    assert len(tables) == 3, f"expected 3 tables, found {len(tables)}"


def test_birth_year_handles_the_plain_template():
    assert parse_birth_year("{{birth date and age|1999|4|7|df=y}}") == 1999


def test_nationality_falls_back_to_bare_code():
    assert parse_nationality("BRA") == "BRA"
    assert parse_nationality("{{flagicon|COL}}") == "COL"


def test_fs_player_layout_with_capital_f():
    # Articles use {{Fs start}} and {{fs start}} interchangeably. A
    # case-sensitive search silently skips every club that capitalises it.
    squad = extract_squad(load("fixture_fsplayer.wiki"))
    assert len(squad) == 5, f"expected 5, got {len(squad)} (out-on-loan excluded)"


def test_full_country_names_map_to_codes():
    # nat=Brazil, not nat=BRA. Truncating to three letters happens to work for
    # Brazil and fails for Netherlands.
    squad = extract_squad(load("fixture_fsplayer.wiki"))
    by_name = {p["name"]: p for p in squad}
    assert by_name["Primeiro Goleiro"]["nationality"] == "BRA"
    assert by_name["Terceiro Atacante"]["nationality"] == "COL"
    assert by_name["Quinto Volante"]["nationality"] == "NED", "Netherlands -> NET"


def test_one_header_cell_per_line_is_read_fully():
    # The roster layout puts each header cell on its own line. Reading only the
    # first gives a single column, so the table never matches and the club falls
    # back to generated players.
    cols = header_columns(
        '{| class="wikitable"\n|-\n'
        '! style="width:5%;"|{{abbr|No.|Squad number}}\n'
        '! style="width:5%;"|{{abbr|Pos.|Primary position}}\n'
        '! style="width:1%;"|{{abbr|Nat.|Nationality}}\n'
        '! | Name\n! | Date of birth (age)\n|-\n| 1\n|}'
    )
    assert cols[:5] == ["no", "pos", "nat", "name", "date of birth (age)"], cols


def test_columns_are_mapped_by_name_not_position():
    # A table with a different column order must still read correctly. Fixed
    # indices are what put appearance counts in the name column.
    idx = map_columns(["name", "date of birth", "pos", "nat"])
    assert idx == {"name": 0, "dob": 1, "pos": 2, "nat": 3}, idx


def test_table_without_a_birth_column_is_rejected():
    # Transfer and appearance tables never carry a date of birth. That single
    # requirement is what stops them being read as rosters.
    assert map_columns(["pos", "player", "transferred from", "fee"]) is None


def test_wikidata_results_match_by_sitelink_not_order():
    """Entities must be paired to the article that was requested.

    Without props=sitelinks the response cannot be matched back, and pairing by
    dict order silently mismatches — which is why the first version filled 49 of
    221 and looked like a coverage problem rather than a bug.
    """
    import scrape_squads as ss

    fake = {
        "entities": {
            "Q1": {
                "id": "Q1",
                "sitelinks": {"enwiki": {"title": "Player One (footballer, born 1987)"}},
                "claims": {"P569": [{"mainsnak": {"datavalue": {"value": {"time": "+1987-06-06T00:00:00Z"}}}}]},
            },
            "Q2": {
                "id": "Q2",
                "sitelinks": {"enwiki": {"title": "Ancient Manager"}},
                "claims": {"P569": [{"mainsnak": {"datavalue": {"value": {"time": "+1948-01-01T00:00:00Z"}}}}]},
            },
            "Q3": {"id": "Q3", "sitelinks": {}, "claims": {}},
        }
    }
    original = ss.api_get
    ss.api_get = lambda params, api=None, attempts=6: fake
    try:
        rows = [
            {"name": "Player One", "birth_year": "", "_title": "Player One (footballer, born 1987)"},
            {"name": "Coach", "birth_year": "", "_title": "Ancient Manager"},
            {"name": "Nobody", "birth_year": "", "_title": "Missing Page"},
        ]
        ss.fill_birth_years(rows, delay=0)
    finally:
        ss.api_get = original

    assert rows[0]["birth_year"] == 1987, rows[0]
    assert rows[1]["birth_year"] == "", "a 1948 birth year is not a current player"
    assert rows[2]["birth_year"] == "", "unmatched titles must stay blank"


def test_birth_year_from_disambiguated_title():
    from scrape_squads import year_from_title

    assert year_from_title("Cassio (footballer, born 1987)") == 1987
    assert year_from_title("Matheus Henrique") is None


def test_pipe_marked_headers_and_bang_marked_name_cells():
    """The third layout inverts both conventions.

    Headers are `|` cells with bold text, and the player name is a `!` cell with
    scope="row". Parsing only `!` headers finds no columns, and skipping `!`
    data lines discards the name column — so this layout produced nothing at all
    for eight clubs.
    """
    squad = extract_squad(load("fixture_pipeheader.wiki"))
    assert len(squad) == 3, f"expected 3 players, got {len(squad)}"
    by_name = {p["name"]: p for p in squad}
    assert "Primeiro Lateral" in by_name, sorted(by_name)
    assert by_name["Primeiro Lateral"]["position"] == "DF"
    assert by_name["Segundo Zagueiro"]["nationality"] == "URU"
    assert by_name["Terceiro Meia"]["birth_year"] == 2000


def test_appearance_columns_do_not_disqualify_a_roster():
    # Some genuine squad tables carry Apps and Goals next to the roster. A
    # date-of-birth column is the reliable signal; rejecting on "apps" throws
    # away real rosters.
    squad = extract_squad(load("fixture_pipeheader.wiki"))
    assert squad, "roster rejected because it also lists appearances"


def test_club_article_squad_parses():
    """Seven of twenty season articles carry no squad section at all.

    Their sections are Transfers, Friendlies, Competitions, References — there
    is nothing to parse, which is not a parser bug. The club's own article is
    where "Current squad" lives in that case, and it uses the {{Fs player}}
    layout the parser already handles.
    """
    squad = extract_squad(load("fixture_club_article.wiki"))
    assert len(squad) == 4, f"expected 4 (out-on-loan excluded), got {len(squad)}"
    by_name = {p["name"]: p for p in squad}
    assert by_name["Goleiro Titular"]["birth_year"] == 1995
    assert by_name["Atacante Principal"]["nationality"] == "COL"


if __name__ == "__main__":
    failures = 0
    for name, fn in sorted(globals().items()):
        if not name.startswith("test_"):
            continue
        try:
            fn()
            print(f"  ok    {name}")
        except AssertionError as e:
            failures += 1
            print(f"  FAIL  {name}: {e}")
    print(f"\n{'FAILED' if failures else 'all passed'} ({failures} failures)")
    sys.exit(1 if failures else 0)
