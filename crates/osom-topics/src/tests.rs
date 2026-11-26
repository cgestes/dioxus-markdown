use crate::*;
use margin_editor_core::{dates, heading::split_heading};

const README: &str = "# Procedures\n\n## Channel → folder map\n\n| Slack channel | Channel ID   | Folder |\n|---------------|--------------|--------|\n| DM with luc   | D0DM0000001  | `dm/`  |\n| #car          | C0CAR000001  | `car/` |\n| Telegram car group | -1001000001 | `car/` |\n| Telegram Nikki group | -1001000002 | `people/nikki-sample.md` — everything about Nikki. |\n| Telegram Nikki group (wedding topics) | -1001000002 | `wedding/` — its own project |\n| _(all channels)_ | —         | `people/` — global people registry |\n";

const REMINDERS: &str = "# Reminders\n\n## Active items\n| ID | What | Recurrence | Deadline | Start | Status | Next ping | Last ping | Deliver to | Source file | Notes |\n|----|------|------------|----------|-------|--------|-----------|-----------|------------|-------------|-------|\n| car-ct-2027 | Book the **CT** | every 2 years | 2027-06-25 | 2027-03-25 | pending | 2027-03-25 | — | Telegram car group | `car/maintenance.md` | Last CT 2025 |\n| nikki-nz-wedding-confirm | Borland: check payment | one-off | 2026-12-01 | 2026-11-26 | pending | 2026-11-26 | — | Telegram DM | `wedding/questions.md` | [Borland thread](https://example.com) |\n| car-sensor | Fix the parking sensor | — | 2027-01-10 | — | pending | — | — | Telegram DM | `people/dana-anghel.md` | |\n\n## Done / cancelled\n| ID | What | Closed on | Outcome |\n|----|------|-----------|---------|\n";

fn files(names: &[&str]) -> Vec<SpaceFile> {
    names
        .iter()
        .map(|n| SpaceFile {
            name: (*n).into(),
            ..Default::default()
        })
        .collect()
}

fn space() -> Vec<Topic> {
    let mut t = topics(&files(&[
        "README.md",
        "reminders.md",
        "Library/Std/Thing.md",
        "_margin/comments/car/log.md.json",
        "Repositories/Std.md",
        "car/log.md",
        "car/maintenance.md",
        "car/invoice.pdf",
        "people/README.md",
        "people/_template.md",
        "people/nikki-sample.md",
        "people/dana-anghel.md",
        "wedding/README.md",
        "wedding/questions.md",
        "house/hot-water/notes.md",
    ]));
    promote_routed_pages(&mut t, &routes(README));
    t
}

#[test]
fn folders_become_topics_and_people_become_page_topics() {
    let t = space();
    let ids: Vec<_> = t.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["car", "house", "people", "wedding"]);
    let people = find(&t, "people").unwrap();
    assert_eq!(people.home(), Some("people/README.md"));
    let kids: Vec<_> = people.children.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(kids, ["Dana Anghel", "Nikki Sample"]);
    let nikki = find(&t, "people/nikki-sample").unwrap();
    assert_eq!(nikki.kind, Kind::Page);
    assert_eq!(nikki.home(), Some("people/nikki-sample.md"));
    assert_eq!(people.docs().count(), 0, "templates and README are not docs");
    let car = find(&t, "car").unwrap();
    assert_eq!(car.home(), Some("car/maintenance.md"), "no README: the main page");
    assert_eq!(car.docs().collect::<Vec<_>>(), ["car/invoice.pdf", "car/log.md"]);
    assert_eq!(find(&t, "wedding").unwrap().file("questions.md"), Some("wedding/questions.md"));
    assert!(find(&t, "house/hot-water").is_some(), "sub-folders are sub-topics");
    let order: Vec<_> = all(&t).iter().map(|t| t.id.as_str()).collect();
    assert_eq!(&order[..4], ["car", "house", "house/hot-water", "people"]);
}

#[test]
fn titles_come_from_the_first_heading() {
    let mut t = Topic::folder("wedding");
    assert_eq!(t.title, "Wedding");
    t.retitle("# Weddings — Alex & Sam (2028)\n\nText");
    assert_eq!((t.title.as_str(), t.subtitle.as_str()), ("Weddings", "Alex & Sam (2028)"));
    t.retitle("# DM: general");
    assert_eq!((t.title.as_str(), t.subtitle.as_str()), ("DM", "general"));
    assert_eq!(split_heading("Car (Mazda 3)"), ("Car", "(Mazda 3)"));
}

#[test]
fn readme_routes_chats_to_folders_and_pages() {
    let r = routes(README);
    assert_eq!(r.len(), 5, "the all-channels row has no id");
    assert_eq!(r[1].target, "car");
    assert_eq!(r[3].target, "people/nikki-sample");
    assert_eq!(r[4].target, "wedding");
    assert_eq!(r[0].label, "DM with luc");
    assert!(r[2].matches("telegram_telegram_-1001000001_cgestes"));
    assert!(!r[2].matches("telegram_telegram_-10010000010_cgestes"), "whole parts only");
}

#[test]
fn reminders_belong_to_the_most_specific_topic() {
    let all = reminders(REMINDERS);
    assert_eq!(all.len(), 3, "the done table has no Deadline column");
    assert_eq!(all[0].what, "Book the CT");
    assert_eq!(all[0].source, "car/maintenance.md");
    assert_eq!(all[1].notes, "Borland thread");
    assert!(all[0].repeats() && !all[1].repeats() && !all[2].repeats());
    let t = space();
    let of = |id: &str| -> Vec<String> {
        let topic = find(&t, id).unwrap();
        all.iter().filter(|r| r.concerns(topic)).map(|r| r.id.clone()).collect()
    };
    assert_eq!(of("car"), ["car-ct-2027", "car-sensor"]);
    assert_eq!(of("wedding"), ["nikki-nz-wedding-confirm"]);
    assert_eq!(of("people/nikki-sample"), ["nikki-nz-wedding-confirm"]);
    let owners: Vec<_> = all.iter().map(|r| r.owner(&t)).collect();
    assert_eq!(owners, [Some("car"), Some("wedding"), Some("people/dana-anghel")]);
    assert_eq!(all[1].due(), "2026-11-26");
}

#[test]
fn planning_key_dates_keep_the_whole_entry_and_only_read_the_named_section() {
    let md = "# Planning\n\n- 2031-04-01 outside the section\n\n## Key dates ⭐\n\n\
              A note written on 2031-04-02.\n\n\
              - **Tue 2031-04-08: important**. Meet at 10:30.\n  Bring the forms.\n\
              - 2031-04-05 — [Workshop](details.md), heads-up 2031-04-04.\n\
              - 2031-02-30 invalid date\n\n\
              ```md\n- 2031-04-06 example only\n```\n\n\
              ## Day brief\n\n- 2031-04-07 outside the section\n";
    let entries = key_dates(md);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].day, dates::parse_date("2031-04-05").unwrap());
    assert_eq!(entries[1].day, dates::parse_date("2031-04-08").unwrap());
    assert_eq!(entries[1].markdown, "**Tue 2031-04-08: important**. Meet at 10:30.\n  Bring the forms.");
}

#[test]
fn headings_in_quotes_and_lists_do_not_change_the_planning_section() {
    let md = "\
> ## Key dates

- 2031-04-01 Outside

- ## Key dates

- 2031-04-02 Also outside

## Key dates

- 2031-04-08 Appointment
  - Bring the forms

> ## Example
> - 2031-04-09 Quoted example

- ## Checklist

- 2031-04-10 Visit";
    let entries = key_dates(md);
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.markdown.as_str())
            .collect::<Vec<_>>(),
        [
            "2031-04-08 Appointment\n  - Bring the forms",
            "2031-04-10 Visit"
        ]
    );
}

#[test]
fn questions_split_open_and_answered() {
    let md = "# Wedding — open questions\n\n## Open\n| ID | Question | Why it matters | Ask when… | Ask by |\n|----|----------|----------------|-----------|--------|\n| Q1 | Which wedding is **legal**? | Paperwork | mairie | 2027-03 |\n\n## Answered\n| ID | Question | Answer | Date | Source |\n|----|----------|--------|------|--------|\n| Q0 | Year? | **2028** | 2026-11-24 | TG |\n";
    let q = questions(md);
    assert_eq!(q.len(), 2);
    assert!(q[0].open && !q[1].open);
    assert_eq!(q[0].question, "Which wedding is legal?");
    assert_eq!(q[0].ask_by, "2027-03");
    assert_eq!(q[1].answer, "2028");
}

#[test]
fn session_keys_parse_to_their_origin() {
    assert_eq!(
        origin("telegram_telegram_-1001000001_cgestes"),
        Origin::Telegram { chat: "-1001000001".into() }
    );
    assert_eq!(
        origin("slack_main_C0CAR000001_1795508185_314769_U0CKVEP9VQW"),
        Origin::Slack { channel: "C0CAR000001".into(), thread: "1795508185.314769".into() }
    );
    assert_eq!(origin("gw_topic-people-nikki-sample"), Origin::Topic("people-nikki-sample".into()));
    assert_eq!(origin("gw_margin-general"), Origin::General);
    assert_eq!(origin("gw_08aac21c"), Origin::Gateway);
    assert_eq!(origin("rpc_280e"), Origin::Other);
    assert_eq!(chat_session("people/nikki-sample"), "topic-people-nikki-sample");
}

#[test]
fn sessions_are_filed_by_routed_chat_id() {
    let session = |key: &str, at: &str| Session {
        session_id: key.into(),
        session_key: key.into(),
        last_activity: at.into(),
        message_count: 3,
        ..Default::default()
    };
    let sessions = [
        session("telegram_telegram_-1001000001_cgestes", "2026-11-24T10:00:00Z"),
        session("slack_main_C0CAR000001_1732400000_1_U0", "2026-11-24T12:00:00Z"),
        session("telegram_telegram_-1001000002_cgestes", "2026-11-23T09:00:00Z"),
        session("gw_topic-car", "2026-11-20T09:00:00Z"),
    ];
    let t = space();
    let r = routes(README);
    let car = threads(find(&t, "car").unwrap(), &r, &sessions);
    let labels: Vec<_> = car.iter().map(|t| (t.label.as_str(), t.origin.source())).collect();
    assert_eq!(
        labels,
        [
            ("#car", Source::Slack),
            ("Telegram car group", Source::Telegram),
            ("Margin chat", Source::Chat)
        ]
    );
    assert_eq!(threads(find(&t, "people/nikki-sample").unwrap(), &r, &sessions).len(), 1);
    assert_eq!(threads(find(&t, "wedding").unwrap(), &r, &sessions).len(), 1);
}

#[test]
fn unrouted_chats_are_unfiled_but_margins_own_are_not() {
    let s = |key: &str, n: u64| Session {
        session_id: key.into(),
        session_key: key.into(),
        message_count: n,
        ..Default::default()
    };
    let u = unfiled(
        &routes(README),
        &[
            s("telegram_telegram_1000000001_cgestes", 69),
            s("telegram_telegram_-1001000001_cgestes", 50),
            s("gw_08aac21c", 35),
            s("gw_topic-car", 4),
            s("gw_margin-general", 2),
            s("rpc_280e", 0),
        ],
    );
    let labels: Vec<_> = u.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, ["Telegram DM", "Dashboard chat"]);
}

#[test]
fn transcripts_keep_what_people_said() {
    let m = |role: &str, content: &str| Message {
        role: role.into(),
        content: content.into(),
        created_at: Some("2026-11-24T13:56:20.1+00:00".into()),
    };
    let first = format!("{}\n\nIs the CT booked?", preamble(&chat_context("Car", "car/", 1, 0)));
    let t = transcript(&[
        m("user", "[2026-11-24 14:56:19 +01:00] Done for gmail"),
        m("assistant", r#"{"content":null,"tool_calls":[{"id":"1","name":"file_edit","arguments":"{}"}]}"#),
        m("tool", r#"{"content":"Edited","tool_call_id":"1"}"#),
        m("assistant", "Corrected **everywhere**."),
        m("assistant", "NO_REPLY"),
        m("user", &first),
    ]);
    assert_eq!(t.len(), 3);
    assert!(t[0].from_user);
    assert_eq!(t[0].text, "Done for gmail");
    assert_eq!(t[0].at, "2026-11-24 14:56");
    assert_eq!(t[1].text, "Corrected **everywhere**.");
    assert_eq!(t[1].tools, ["file_edit"]);
    assert_eq!(t[1].at, "2026-11-24 13:56");
    assert_eq!(t[2].text, "Is the CT booked?", "the context preamble is hidden");
}

#[test]
fn registries_relate_pages_to_people() {
    let readme = "## Index\n| Person | Relationship to me | Circle | File |\n|---|---|---|---|\n| Nikki Sample | Wife | Family | [nikki-sample.md](nikki-sample.md) |\n| Dr A | Doctor (pregnancy) | Professional | [a.md](a.md) |\n";
    let r = relations(readme, "people");
    assert_eq!(r.get("people/nikki-sample.md").map(String::as_str), Some("Wife"));
    assert_eq!(r.get("people/a.md").map(String::as_str), Some("Doctor (pregnancy)"));
}

#[test]
fn task_items_are_read_like_the_notebook() {
    let md = "# Groceries ideas\n- [ ] Mango\n- [x] **Basil**\n1. [ ] Bread\n- plain\n\n```\n- [ ] not a task\n```\n";
    assert_eq!(
        tasks(md),
        [("Mango".to_string(), false), ("Basil".to_string(), true), ("Bread".to_string(), false)]
    );
}

#[test]
fn plain_text_drops_inline_markup() {
    assert_eq!(plain("**2027-04** or `x` [a](b) c"), "2027-04 or x a c");
    assert_eq!(plain("[ ] not a link"), "[ ] not a link");
    assert_eq!(plain("- not a list"), "- not a list");
    assert!(blank("_TBD_") && blank("—") && !blank("2027-03"));
}
