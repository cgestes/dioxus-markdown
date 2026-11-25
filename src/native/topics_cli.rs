//! Commands that run without a window: setup, pairing, a text summary of
//! the topics, and one real comment turn.
use super::topics_live::{Live, load};
use super::worker;
use margin_editor_core::comments as file;
use margin_editor_core::hub::{self, Ask, Endpoints, Space};

/// Run a command from `args`. Returns an exit code when one ran.
pub(super) fn cli(args: &[String]) -> Option<i32> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match words.as_slice() {
        ["setup", space, zeroclaw, agent] => setup(space, zeroclaw, agent),
        ["pair", code] => pair(code),
        ["topics", "--print"] | ["--topics", "--print"] => print(),
        ["comment-test", page, quote, request] => comment_test(page, quote, request),
        ["setup", ..] | ["pair", ..] | ["comment-test", ..] => Err("usage:
  margin setup <silverbullet-url> <zeroclaw-url> <agent>
  margin pair <code from `zeroclaw gateway get-paircode --new`>
  margin comment-test <page> <quote> <request>"
            .into()),
        _ => return None,
    };
    Some(match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("margin: {e}");
            1
        }
    })
}

fn setup(space: &str, zeroclaw: &str, agent: &str) -> Result<(), String> {
    Endpoints::new(space, zeroclaw, agent).save()?;
    println!("Saved {}", Endpoints::path().display());
    Ok(())
}

fn pair(code: &str) -> Result<(), String> {
    let e = Endpoints::load()?;
    let sessions = hub::pair(&e, code)?.sessions()?;
    println!(
        "Paired with {}. The token is in {}; {} sessions are visible.",
        e.zeroclaw,
        hub::token::location(e.account()),
        sessions.len()
    );
    Ok(())
}

/// Ask the comment agent about `quote` in `page`, exactly as the editor does
/// on a new thread, on a throwaway session; print what the editor would show.
fn comment_test(page: &str, quote: &str, request: &str) -> Result<(), String> {
    let e = Endpoints::load()?;
    let zc = e.zeroclaw();
    let md = Space::new(&e.space).read(page)?.text;
    let at = md.find(quote).ok_or_else(|| format!("`{quote}` is not in {page}"))?;
    let t = file::turn("connection-test", page, &md, at..at + quote.len(), request, None);
    let ask = Ask {
        agent: e.comment_agent.clone(),
        session: t.session.clone(),
        name: t.name,
        content: t.content,
        preamble: Some(t.preamble),
    };
    let answer = worker::answer(&zc, &ask);
    let cleanup = zc.delete_session(&t.session);
    for tool in &answer.tools {
        println!("[{} used {tool}]", e.comment_agent);
    }
    if let Some(err) = &answer.error {
        println!("[error] {err}");
    }
    println!("── reply ──\n{}", file::visible(&answer.text));
    match file::edit(&answer.text) {
        Some(edit) => println!("── proposal: {} ──\n- {quote}\n+ {}", edit.label, edit.replacement),
        None => println!("── no proposal ──"),
    }
    println!("session deleted: {}", cleanup.is_ok());
    answer.error.map_or(Ok(()), Err)
}

fn print() -> Result<(), String> {
    let live = Live::new(Endpoints::load()?);
    let loaded = load(&live)?;
    for t in loaded.ws.all() {
        println!(
            "{}{} ({})  reminders {} · discussions {} · questions {} · docs {} · lists {}",
            "  ".repeat(t.id.matches('/').count()),
            t.title,
            t.id,
            t.agenda.len(),
            t.threads.len(),
            t.questions.iter().filter(|q| q.open).count(),
            t.docs.len(),
            t.lists.len(),
        );
    }
    println!("Unfiled chats: {}", loaded.ws.inbox.len());
    if let Some(w) = loaded.warning {
        println!("{w}");
    }
    if !live.zc.paired() {
        println!("Not paired: run `margin pair <code>` with the code from `zeroclaw gateway get-paircode --new`.");
    }
    Ok(())
}
