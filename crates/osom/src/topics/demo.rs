//! Sample data for `osom --demo`: the topics of the design mockup
//! (`mockup/topics.html`), so the window can be shown without a tailnet.
use crate::topics::view::*;

fn s(x: &str) -> String {
    x.to_string()
}

fn topic(id: &str, title: &str, icon: Icon, hue: Hue) -> TopicView {
    TopicView {
        id: s(id),
        title: s(title),
        icon,
        hue,
        vault_path: format!("topics/{id}"),
        ..Default::default()
    }
}

/// `sort` is the ISO date; `to` is where the ping goes.
fn agenda(sort: &str, title: &str, detail: &str, to: &str, tag: &str) -> Agenda {
    Agenda {
        title: s(title),
        deliver_to: s(to),
        detail: s(detail),
        tag: s(tag),
        sort: s(sort),
        primary: true,
        ..Default::default()
    }
}

/// A sub-topic repeats its parent's entries; Today shows the parent's.
fn echo(mut t: TopicView) -> TopicView {
    for a in &mut t.agenda {
        a.primary = false;
    }
    t
}

fn repeating(mut a: Agenda) -> Agenda {
    a.repeats = true;
    a
}

/// `when` is (label, ISO time) relative to 2026-11-24T22:00:00Z.
fn thread(
    key: String,
    source: Source,
    place: &str,
    title: &str,
    snippet: &str,
    when: (&str, &str),
    messages: u64,
) -> ThreadRow {
    ThreadRow {
        key,
        source,
        title: s(title),
        snippet: s(snippet),
        place: s(place),
        when: s(when.0),
        sort: s(when.1),
        messages,
        ..Default::default()
    }
}

fn unread(mut t: ThreadRow) -> ThreadRow {
    t.unread = true;
    t
}

fn with_turns(mut t: ThreadRow, turns: &[(&str, &str)]) -> ThreadRow {
    t.turns = turns.iter().map(|(who, text)| (s(who), s(text))).collect();
    t
}

fn doc(kind: &str, name: &str, meta: &str) -> DocRow {
    DocRow {
        kind: s(kind),
        name: s(name),
        meta: s(meta),
        ..Default::default()
    }
}

fn checklist(title: &str, meta: &str, items: &[(&str, bool)]) -> Checklist {
    Checklist {
        title: s(title),
        meta: s(meta),
        items: items.iter().map(|(text, done)| (s(text), *done)).collect(),
        ..Default::default()
    }
}

fn shopping(mut c: Checklist) -> Checklist {
    c.shopping = true;
    c
}

fn pairs(facts: &[(&str, &str)]) -> Vec<(String, String)> {
    facts.iter().map(|(k, v)| (s(k), s(v))).collect()
}

fn me(text: &str) -> ChatLine {
    ChatLine {
        from_user: true,
        text: s(text),
        ..Default::default()
    }
}

fn agent(text: &str) -> ChatLine {
    ChatLine {
        text: s(text),
        ..Default::default()
    }
}

fn question(id: &str, question: &str, why: &str, ask_when: &str, ask_by: &str) -> QuestionRow {
    QuestionRow {
        id: s(id),
        question: s(question),
        why: s(why),
        ask_when: s(ask_when),
        ask_by: s(ask_by),
        open: true,
        ..Default::default()
    }
}

const HOT_WATER_YAML: &str = r#"# Boost the cylinder before morning showers if it fell below 55 °C
alias: Hot water boost
trigger:
  - platform: time
    at: "05:30:00"
condition:
  - condition: numeric_state
    entity_id: sensor.cylinder_temp
    below: 55
action:
  - service: switch.turn_on
    target: { entity_id: switch.hwc_element }"#;

const POWER_BUDGET_PY: &str = r#"# Sum module draw from Collection.md and compare to the PSU
rows = parse_table("Collection.md")
used = sum(r["+12V mA"] for r in rows if r["status"] == "racked")
print(f"+12 V: {used} / 1200 mA")"#;

fn mazda() -> TopicView {
    let id = "mazda";
    let tag = "[mazda]";
    TopicView {
        next: s("WOF · 14 Dec"),
        unread: true,
        brief: s(
            "WOF is due Monday 14 December; the garage can take it Friday 9am. The last service flagged front brake pads at about 30%. Rego is paid until August 2027.",
        ),
        brief_by: s("from 23 messages · 2 h ago"),
        agenda: vec![
            agenda(
                "2026-12-14",
                "WOF due",
                "cron 0 9 14 12 *",
                "Telegram › Car",
                tag,
            ),
            agenda(
                "2027-01-05",
                "Book 60,000 km service",
                "cron 0 9 5 1 *",
                "Telegram › Car",
                tag,
            ),
            agenda(
                "2027-08-01",
                "Rego renewal (2027)",
                "cron 0 9 1 8 *",
                "Telegram › Car",
                tag,
            ),
        ],
        threads: vec![
            unread(with_turns(
                thread(
                    format!("{id}-1"),
                    Source::Telegram,
                    "Home › Car",
                    "WOF booking",
                    "Garage can do Friday 9am — want me to add it to the calendar?",
                    ("2 h", "2026-11-24T20:00:00Z"),
                    5,
                ),
                &[
                    ("you", "When is the WOF due again?"),
                    ("agent", "Monday 14 Dec. The garage has Friday 9am free."),
                    ("you", "Book Friday."),
                ],
            )),
            thread(
                format!("{id}-2"),
                Source::Chat,
                "Margin chat",
                "Tyre pressure for the road trip",
                "36 psi front, 34 rear when loaded; check cold.",
                ("Mon", "2026-11-23T18:30:00Z"),
                4,
            ),
            thread(
                format!("{id}-3"),
                Source::Slack,
                "#facilities",
                "Car park permit renewal",
                "Form sent to facilities, permit valid to March.",
                ("12 Nov", "2026-11-12T10:00:00Z"),
                3,
            ),
        ],
        docs: vec![
            doc("PDF", "Service invoice — Oct 2026.pdf", "212 KB · 14 Oct"),
            doc("PDF", "Rego receipt 2026.pdf", "88 KB · 2 Aug"),
            doc("MD", "Car details.md", "note · edited 3 Nov"),
            doc("IMG", "Tyre placard.jpg", "1.2 MB · 9 Sep"),
        ],
        procedures: vec![
            checklist(
                "Before a long drive",
                "5 steps · last run 3 Oct",
                &[
                    ("Tyres: 36 front / 34 rear psi, cold", true),
                    ("Washer fluid topped up", true),
                    ("Oil level between marks", false),
                    ("Spare + jack in boot", false),
                    ("Download offline maps", false),
                ],
            ),
            checklist(
                "Replace cabin air filter",
                "4 steps · every 15,000 km",
                &[
                    ("Empty glovebox, unclip the damper", false),
                    ("Press sides, lower the glovebox", false),
                    ("Swap filter — arrow points down", false),
                    ("Reassemble, fan on max to check", false),
                ],
            ),
        ],
        lists: vec![checklist(
            "To buy",
            "lists/to-buy.md",
            &[
                ("Front wiper blades 600 / 400 mm", false),
                ("12 V tyre inflator", false),
                ("Microfibre cloths", true),
            ],
        )],
        facts: pairs(&[
            ("rego", "paid to Aug 2027"),
            ("odometer", "≈ 57,800 km"),
            ("tyres", "225/55 R19"),
            ("brakes", "front pads ~30% (Oct)"),
        ]),
        chat: vec![
            me("Can you book the WOF for Friday?"),
            agent(
                "I added **[mazda] WOF — garage Fri 9:00** and a reminder Thursday 18:00 in the Car thread.\n\nWant me to ask the garage to quote the brake pads while it’s in?",
            ),
        ],
        chat_context: s("Mazda · brief, 4 facts, 2 procedures"),
        ..topic(id, "Mazda", Icon::Car, Hue::Slate)
    }
}

fn wedding() -> TopicView {
    let id = "wedding";
    let tag = "[wedding]";
    TopicView {
        next: s("Deposit · 2 Dec"),
        unread: true,
        brief: s(
            "Venue confirmed for Saturday 20 March. The deposit is due Wednesday 2 December. 64 of 90 RSVPs are in; the catering tasting is Saturday 12 December at 11:00.",
        ),
        brief_by: s("from 41 messages · this morning"),
        agenda: vec![
            agenda(
                "2026-12-02",
                "Venue deposit due",
                "cron 0 9 2 12 *",
                "Telegram › Wedding",
                tag,
            ),
            agenda(
                "2026-12-12",
                "Catering tasting 11:00",
                "cron 0 8 12 12 *",
                "Telegram › Wedding",
                tag,
            ),
            agenda(
                "2027-01-15",
                "RSVP chase-up round 2",
                "cron 0 10 15 1 *",
                "Telegram › Wedding",
                tag,
            ),
            agenda(
                "2027-03-20",
                "Wedding day",
                "date in topic.md",
                "—",
                tag,
            ),
        ],
        threads: vec![
            unread(thread(
                format!("{id}-1"),
                Source::Telegram,
                "N&C wedding group",
                "Tasting time",
                "Can we move the tasting to 11? Caterer said yes.",
                ("1 h", "2026-11-24T21:00:00Z"),
                7,
            )),
            thread(
                format!("{id}-2"),
                Source::Chat,
                "Margin chat",
                "Seating plan draft",
                "Grouped by household; 3 tables still unbalanced.",
                ("Sun", "2026-11-22T16:00:00Z"),
                9,
            ),
            thread(
                format!("{id}-3"),
                Source::Telegram,
                "N&C wedding group",
                "Playlist ideas",
                "12 songs added to the shared list.",
                ("Thu", "2026-11-19T19:00:00Z"),
                15,
            ),
        ],
        docs: vec![
            doc("MD", "Guest list.md", "table · 90 rows"),
            doc("MD", "Budget.md", "table · edited today"),
            doc("PDF", "Venue contract.pdf", "540 KB · 30 Oct"),
            doc("IMG", "Mood board.jpg", "3.4 MB · 18 Oct"),
        ],
        procedures: vec![checklist(
            "RSVP follow-up",
            "4 steps · run before each deadline",
            &[
                ("Export missing RSVPs from Guest list", true),
                ("Draft reminder message", true),
                ("Send via Telegram / SMS", false),
                ("Update counts in Budget", false),
            ],
        )],
        lists: vec![checklist(
            "Catering ideas",
            "lists/catering-ideas.md",
            &[
                ("Grazing table instead of entrée", false),
                ("Vegetarian main — mushroom wellington", false),
                ("Late-night snack: sliders", true),
            ],
        )],
        questions: vec![
            question(
                "Q1",
                "Which wording for the RSVP chase-up: a friendly nudge or a firm deadline?",
                "Round 2 goes out on 15 Jan",
                "guest list, RSVPs",
                "2027-01-10",
            ),
            question(
                "Q2",
                "Final catering head count for the tasting?",
                "The caterer needs numbers by the tasting",
                "catering, tasting, budget",
                "2026-12-12",
            ),
            QuestionRow {
                answer: s("Sat 20 Mar 2027"),
                open: false,
                ..question(
                    "Q0",
                    "Which date for the wedding?",
                    "Venue booking and save-the-dates",
                    "venue, date",
                    "—",
                )
            },
        ],
        facts: pairs(&[
            ("date", "Sat 20 Mar 2027"),
            ("guests", "90 invited · 64 RSVP"),
            ("venue", "deposit 30%, due 2 Dec"),
        ]),
        chat: vec![
            me("Who hasn’t replied yet?"),
            agent(
                "26 guests across 14 households. The biggest gaps are the cousins’ table (7) and work friends (5). Shall I draft the reminder?",
            ),
        ],
        chat_context: s("Wedding N&C · brief, 3 facts, guest list"),
        ..topic(id, "Wedding N&C", Icon::Rings, Hue::Rose)
    }
}

fn plumber(tag: &str) -> Agenda {
    agenda(
        "2026-11-26",
        "Plumber — hot water cylinder, 10:00",
        "cron 0 8 26 11 *",
        "Telegram › House",
        tag,
    )
}

fn cylinder_check(tag: &str) -> Agenda {
    repeating(agenda(
        "2026-11-29",
        "Check cylinder ≥ 60 °C",
        "cron 0 18 * * 0",
        "Telegram › House",
        tag,
    ))
}

fn clean_gutters(tag: &str) -> Agenda {
    agenda(
        "2027-03-27",
        "Clean gutters before winter",
        "cron 0 9 27 3 *",
        "Telegram › House",
        tag,
    )
}

fn hot_water_cold(key: String) -> ThreadRow {
    with_turns(
        thread(
            key,
            Source::Telegram,
            "Home › House",
            "Hot water cold again",
            "Element tripped at 6am; reset the breaker, back to 62 °C by 8.",
            ("Yesterday", "2026-11-23T05:30:00Z"),
            6,
        ),
        &[
            ("you", "No hot water again 😩"),
            (
                "agent",
                "Third trip this month. Reset steps are in “Reset the cylinder”. I’ve added it to the plumber notes.",
            ),
            ("you", "Reset, it’s heating."),
        ],
    )
}

fn heat_pump(key: String) -> ThreadRow {
    thread(
        key,
        Source::Chat,
        "Margin chat",
        "Heat-pump cylinder vs. new element",
        "Payback ≈ 4.5 years at current tariff; quotes compared.",
        ("Fri", "2026-11-20T20:00:00Z"),
        11,
    )
}

fn gutter_guards(key: String) -> ThreadRow {
    thread(
        key,
        Source::Slack,
        "#house",
        "Gutter guard options",
        "Mesh vs. brush — mesh for the south side.",
        ("9 Nov", "2026-11-09T09:00:00Z"),
        4,
    )
}

fn reset_cylinder() -> Checklist {
    checklist(
        "Reset the hot water cylinder",
        "5 steps · used 3× this month",
        &[
            ("Switch off “HWC” breaker (board, row 2)", true),
            ("Wait 30 s, switch back on", true),
            ("Check thermostat light after 5 min", true),
            ("If it trips again: leave off, message plumber", false),
            ("Log the trip time in the timeline", false),
        ],
    )
}

fn flush_gutters() -> Checklist {
    checklist(
        "Clean & flush gutters",
        "4 steps · yearly, March",
        &[
            ("Ladder + stand-off on the south side", false),
            ("Scoop debris, bag it", false),
            ("Flush downpipes with the hose", false),
            ("Fit mesh guards", false),
        ],
    )
}

fn hardware_store() -> Checklist {
    checklist(
        "Hardware store",
        "lists/hardware-store.md",
        &[
            ("Gutter guard mesh, 10 m", false),
            ("Downpipe elbow 80 mm", false),
            ("Hose fitting for flushing", true),
        ],
    )
}

fn hot_water_automation() -> Snippet {
    Snippet {
        file: s("home-assistant/hot_water_boost.yaml"),
        code: s(HOT_WATER_YAML),
    }
}

fn hot_water() -> TopicView {
    let id = "house/hot-water";
    let tag = "[house/hot-water]";
    TopicView {
        subtitle: s("chauffe-eau"),
        next: s("Plumber · Thu"),
        brief: s(
            "The cylinder trips its element about once a week; a plumber is booked Thursday 10:00. Keep it at 60 °C or above.",
        ),
        brief_by: s("from 17 messages · yesterday"),
        agenda: vec![plumber(tag), cylinder_check(tag)],
        threads: vec![
            hot_water_cold(format!("{id}-1")),
            heat_pump(format!("{id}-2")),
        ],
        docs: vec![
            doc("PDF", "Cylinder manual.pdf", "2.1 MB"),
            doc("PDF", "Plumber quote.pdf", "96 KB · 20 Nov"),
        ],
        procedures: vec![reset_cylinder()],
        code: vec![hot_water_automation()],
        facts: pairs(&[
            ("cylinder", "180 L, 3 kW element"),
            ("breaker", "board row 2 “HWC”"),
            ("plumber", "Thu 26 Nov 10:00"),
        ]),
        chat: vec![
            me("How often has it tripped?"),
            agent(
                "Three times this month: 3, 12 and 23 November, each time around 6am. I’ve put the dates in the plumber notes.",
            ),
        ],
        chat_context: s("Hot water · brief, 3 facts, 1 procedure, 1 automation"),
        ..topic(id, "Hot water", Icon::Drop, Hue::Clay)
    }
}

fn gutters() -> TopicView {
    let id = "house/gutters";
    let tag = "[house/gutters]";
    TopicView {
        subtitle: s("gouttières"),
        next: s("Clean · 27 Mar"),
        brief: s(
            "The south gutters overflow in heavy rain. Clean them and fit mesh guards before winter.",
        ),
        brief_by: s("from 4 messages · 9 Nov"),
        agenda: vec![clean_gutters(tag)],
        threads: vec![gutter_guards(format!("{id}-1"))],
        docs: vec![doc("IMG", "South gutters (4 photos)", "8 Nov")],
        procedures: vec![flush_gutters()],
        lists: vec![hardware_store()],
        facts: pairs(&[("gutters", "south side overflows")]),
        chat: vec![
            me("Mesh or brush guards?"),
            agent(
                "Mesh for the south side: the overflow comes from leaves, and mesh sheds them in wind. Brush guards hold leaf litter.",
            ),
        ],
        chat_context: s("Gutters · brief, 1 fact, 1 procedure"),
        ..topic(id, "Gutters", Icon::Gutter, Hue::Clay)
    }
}

fn house() -> TopicView {
    let id = "house";
    let tag = "[house]";
    TopicView {
        next: s("Plumber · Thu"),
        brief: s(
            "The hot water cylinder trips its element about once a week; a plumber is booked Thursday 10:00. The south gutters overflow in heavy rain — clean and add guards before winter.",
        ),
        brief_by: s("from 31 messages · yesterday"),
        agenda: vec![plumber(tag), cylinder_check(tag), clean_gutters(tag)],
        threads: vec![
            hot_water_cold(format!("{id}-1")),
            heat_pump(format!("{id}-2")),
            gutter_guards(format!("{id}-3")),
        ],
        docs: vec![
            doc("PDF", "Cylinder manual.pdf", "2.1 MB"),
            doc("PDF", "Plumber quote.pdf", "96 KB · 20 Nov"),
            doc("IMG", "South gutters (4 photos)", "8 Nov"),
            doc("MD", "House systems.md", "note · breakers, valves, filters"),
        ],
        procedures: vec![reset_cylinder(), flush_gutters()],
        lists: vec![shopping(hardware_store())],
        code: vec![hot_water_automation()],
        facts: pairs(&[
            ("cylinder", "180 L, 3 kW element"),
            ("breaker", "board row 2 “HWC”"),
            ("plumber", "Thu 26 Nov 10:00"),
            ("gutters", "south side overflows"),
        ]),
        chat: vec![
            me("Is 60 °C the right setpoint?"),
            agent(
                "Yes — keep it at **60 °C or above** to stop Legionella growing; use a tempering valve so taps stay at 55 °C. Your automation only boosts below 55 °C, so that’s consistent.",
            ),
        ],
        chat_context: s("Noisy Marae · brief, 4 facts, 2 procedures, 1 automation"),
        children: vec![echo(hot_water()), echo(gutters())],
        ..topic(id, "Noisy Marae", Icon::Home, Hue::Clay)
    }
}

fn modular() -> TopicView {
    let id = "modular";
    let tag = "[modular]";
    TopicView {
        next: s("Wishlist · Fri"),
        brief: s(
            "The rack is 2 × 104 HP with 23 HP free and about 400 mA of +12 V headroom. The shortlist for the next module is a dual VCA or a sequencer.",
        ),
        brief_by: s("from 12 messages · 3 days ago"),
        agenda: vec![agenda(
            "2026-11-27",
            "Black Friday — check wishlist prices",
            "cron 0 8 27 11 *",
            "Telegram › Modular",
            tag,
        )],
        threads: vec![
            thread(
                format!("{id}-1"),
                Source::Chat,
                "Margin chat",
                "Which VCA fits in 8 HP?",
                "Three candidates under 8 HP, all < 60 mA on +12 V.",
                ("Sat", "2026-11-21T15:00:00Z"),
                6,
            ),
            thread(
                format!("{id}-2"),
                Source::Telegram,
                "Home › Modular",
                "Patch idea: generative bells",
                "Voice note transcribed; patch saved to Patches.md.",
                ("15 Nov", "2026-11-15T21:00:00Z"),
                2,
            ),
        ],
        docs: vec![
            doc("MD", "Collection.md", "table · 31 modules"),
            doc("IMG", "Rack layout 2×104HP.png", "640 KB"),
            doc("MD", "Patches.md", "note · 14 patches"),
            doc("PDF", "Manuals (9)", "folder"),
        ],
        procedures: vec![checklist(
            "Before buying a module",
            "4 steps",
            &[
                ("Fits free HP (23 HP)", true),
                ("Power: +12 V / −12 V within headroom", false),
                ("Depth OK for the case", false),
                ("Second-hand price check", false),
            ],
        )],
        lists: vec![checklist(
            "Wishlist",
            "lists/wishlist.md",
            &[
                ("Dual VCA (≤ 8 HP)", false),
                ("Trigger sequencer", false),
                ("Spare ribbon cables", false),
            ],
        )],
        code: vec![Snippet {
            file: s("scripts/power_budget.py"),
            code: s(POWER_BUDGET_PY),
        }],
        facts: pairs(&[
            ("case", "2 × 104 HP"),
            ("free", "23 HP"),
            ("+12 V", "≈ 400 mA headroom"),
            ("budget", "≈ $450"),
        ]),
        chat: vec![
            me("Does a 12 HP sequencer fit?"),
            agent(
                "Yes: 23 HP free, so 11 HP would be left. It draws 110 mA on +12 V, which leaves about 290 mA of headroom.",
            ),
        ],
        chat_context: s("Modular · brief, collection table, wishlist"),
        ..topic(id, "Modular", Icon::Sliders, Hue::Ochre)
    }
}

fn people() -> TopicView {
    let id = "people";
    let tag = "[people]";
    TopicView {
        next: s("Alex · 6 Dec"),
        brief: s(
            "18 people. Two birthdays in December (Alex on the 6th, Mum on the 17th). You last called Mum 12 days ago.",
        ),
        brief_by: s("from person notes · today"),
        agenda: vec![
            repeating(agenda(
                "2026-12-06",
                "Alex’s birthday",
                "cron 0 9 6 12 *",
                "Telegram › People",
                tag,
            )),
            repeating(agenda(
                "2026-12-17",
                "Mum’s birthday — call",
                "cron 0 9 17 12 *",
                "Telegram › People",
                tag,
            )),
        ],
        threads: vec![thread(
            format!("{id}-1"),
            Source::Chat,
            "Margin chat",
            "Gift for Mum",
            "Shortlist: a garden voucher or a framed print.",
            ("Wed", "2026-11-18T19:00:00Z"),
            5,
        )],
        docs: vec![
            doc("MD", "Mum.md", "person · birthday, sizes, likes"),
            doc("MD", "Alex.md", "person · kids, last seen"),
            doc("MD", "Neighbours.md", "person · 3 entries"),
        ],
        lists: vec![checklist(
            "Gift ideas",
            "lists/gift-ideas.md",
            &[
                ("Mum — garden voucher", false),
                ("Alex — climbing gym pass", false),
            ],
        )],
        facts: pairs(&[("people", "18 notes"), ("next", "Alex · 6 Dec")]),
        chat: vec![
            me("What did Alex’s kids want last year?"),
            agent("From the note: Lego sets for both, and the youngest asked for a scooter."),
        ],
        chat_context: s("People · 18 person notes"),
        ..topic(id, "People", Icon::Users, Hue::Pine)
    }
}

fn nikki() -> TopicView {
    let id = "nikki";
    let tag = "[nikki]";
    TopicView {
        subtitle: s("Osom wife"),
        next: s("Anniversary · 12 Jan"),
        brief: s(
            "Anniversary is Tuesday 12 January — book dinner by the 2nd. Groceries ideas and gift ideas are kept here. (Sample content.)",
        ),
        brief_by: s("from 9 messages · 4 days ago"),
        agenda: vec![
            repeating(agenda(
                "2026-11-27",
                "Flowers on the way home?",
                "cron 0 16 * * 5",
                "Telegram (DM)",
                tag,
            )),
            agenda(
                "2027-01-02",
                "Book anniversary dinner",
                "cron 0 10 2 1 *",
                "Telegram (DM)",
                tag,
            ),
            repeating(agenda(
                "2027-01-12",
                "Anniversary",
                "cron 0 8 12 1 *",
                "Telegram (DM)",
                tag,
            )),
        ],
        threads: vec![
            thread(
                format!("{id}-1"),
                Source::Chat,
                "Margin chat",
                "Anniversary dinner shortlist",
                "Three places with a 7:30 table; one has a garden.",
                ("Thu", "2026-11-19T20:00:00Z"),
                6,
            ),
            thread(
                format!("{id}-2"),
                Source::Telegram,
                "DM",
                "Groceries ideas",
                "Added: ripe mangoes, dark chocolate (70%).",
                ("Sun", "2026-11-22T11:00:00Z"),
                2,
            ),
        ],
        docs: vec![
            doc("MD", "Nikki.md", "person · sizes, likes, dates"),
            doc("MD", "Date ideas.md", "note · 11 ideas"),
        ],
        lists: vec![
            shopping(checklist(
                "Groceries ideas",
                "lists/groceries-ideas.md",
                &[
                    ("Ripe mangoes", false),
                    ("Dark chocolate 70%", false),
                    ("Fresh basil", true),
                    ("Sparkling water", false),
                ],
            )),
            checklist(
                "Gift ideas",
                "lists/gift-ideas.md",
                &[("Pottery class for two", false), ("Linen scarf", false)],
            ),
        ],
        facts: pairs(&[
            ("anniversary", "12 Jan"),
            ("likes", "mango, dark chocolate (sample)"),
        ]),
        chat: vec![
            me("Remind me to book dinner"),
            agent(
                "Done: **[nikki] Book anniversary dinner** on Sat 2 Jan, 10:00, sent to your Telegram DM.",
            ),
        ],
        chat_context: s("Nikki · brief, 2 facts, 2 lists"),
        ..topic(id, "Nikki", Icon::Heart, Hue::Plum)
    }
}

fn inbox_item(
    n: u32,
    source: Source,
    place: &str,
    text: &str,
    when: &str,
    suggestions: &[(&str, &str)],
) -> InboxItem {
    InboxItem {
        key: format!("inbox-{n}"),
        source,
        place: s(place),
        text: s(text),
        when: s(when),
        suggestions: suggestions.iter().map(|(id, p)| (s(id), s(p))).collect(),
    }
}

/// Pages the demo opens in the editor, by space path.
const PAGES: &[(&str, &str)] = &[
    ("topics/house/house-systems.md", "# House systems\n\nWhere things are and how to reset them. Sample content.\n\n## Electrical board\n\n| Row | Breaker | Circuit |\n| :--- | :--- | ---: |\n| 1 | Kitchen | 20 A |\n| 2 | HWC (hot water cylinder) | 16 A |\n| 3 | Lounge + office | 10 A |\n\n## Hot water cylinder\n\n- 180 L, 3 kW element, thermostat at **60 °C**.\n- Tempering valve under the laundry sink keeps the taps at 55 °C.\n- [ ] Ask the plumber about the element on Thursday.\n- [x] Log the last trip time.\n\n> If it trips twice in a week, leave the breaker off and message the plumber.\n\n## Gutters\n\nThe south side overflows in heavy rain; mesh guards go on before winter.\n"),
    ("topics/mazda/car-details.md", "# Car details\n\nThe facts the agent keeps about the car. Sample content.\n\n| Field | Value |\n| :--- | :--- |\n| Rego | paid to Aug 2027 |\n| Odometer | ≈ 57,800 km |\n| Tyres | 225/55 R19 |\n\n## Service\n\n- Oct 2026: service; front pads at about 30%.\n- [ ] Book the WOF before **14 Dec**.\n- [ ] Ask for a brake pad quote at the same visit.\n"),
    ("topics/wedding/guest-list.md", "# Guest list\n\n64 of 90 replies are in. Sample content.\n\n| Household | Guests | RSVP |\n| :--- | ---: | :--- |\n| Cousins | 7 | waiting |\n| Work friends | 5 | waiting |\n| Neighbours | 4 | yes |\n\n- [ ] Chase the remaining replies by **15 Jan**.\n"),
    ("topics/modular/collection.md", "# Collection\n\n2 × 104 HP, 23 HP free. Sample content.\n\n| Module | Maker | HP | +12 V mA |\n| :--- | :--- | ---: | ---: |\n| Oscillator | sample | 10 | 60 |\n| Envelope | sample | 8 | 40 |\n| Dual VCA | wishlist | 8 | 50 |\n"),
];

/// The text of a demo page.
pub fn page(path: &str) -> Option<&'static str> {
    PAGES.iter().find(|(p, _)| *p == path).map(|(_, text)| *text)
}

/// Give each demo doc that has a sample page its path, so it opens.
fn link_pages(t: &mut TopicView) {
    for d in &mut t.docs {
        let slug = d.name.to_lowercase().replace(' ', "-");
        let path = format!("topics/{}/{slug}", t.id);
        if page(&path).is_some() {
            d.path = Some(path);
        }
    }
    for c in &mut t.children {
        link_pages(c);
    }
}

pub fn workspace() -> Workspace {
    let mut topics = vec![mazda(), wedding(), house(), modular(), people(), nikki()];
    topics.iter_mut().for_each(link_pages);
    Workspace {
        topics,
        inbox: vec![
            inbox_item(
                1,
                Source::Telegram,
                "DM",
                "Remind me to call the insurer about the windscreen chip",
                "25 min",
                &[("mazda", "0.74"), ("house", "0.12")],
            ),
            inbox_item(
                2,
                Source::Slack,
                "#general",
                "Anyone selling a 6U case? Asking for a friend 😉",
                "3 h",
                &[("modular", "0.66")],
            ),
            inbox_item(
                3,
                Source::Chat,
                "Margin chat",
                "Pick up peonies on the way home Friday",
                "Yesterday",
                &[("nikki", "0.71"), ("wedding", "0.18")],
            ),
        ],
        inbox_title: s("Inbox"),
        inbox_note: s("Turns the hub couldn’t file with confidence ≥ 0.8. Your choices become examples for the classifier."),
        host: s("home.tailnet.ts.net"),
        agent: s("home"),
        today: s("2026-11-24"),
        general_chat: vec![
            me("What needs doing this week?"),
            agent(
                "Four things: plumber Thursday 10:00, Black Friday wishlist check, cylinder temperature on Sunday, and the venue deposit next Wednesday.",
            ),
        ],
    }
}
