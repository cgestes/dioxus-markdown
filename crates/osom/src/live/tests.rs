use super::*;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::TcpListener;
use std::time::Duration;

fn load_with_failed_page(failed: &str, status: &str) -> Loaded {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let live = Live {
        endpoints: Endpoints::new(&url, &url, "test"),
        space: Space::new(&url),
        zc: ZeroClaw::new(&url, None),
    };
    std::thread::scope(|scope| {
        let loading = scope.spawn(|| load(&live));
        while !loading.is_finished() {
            let mut stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            stream.set_nonblocking(false).unwrap();
            let mut headers = BufReader::new(&stream).lines().map(Result::unwrap);
            let request = headers.next().unwrap();
            for line in headers {
                if line.is_empty() {
                    break;
                }
            }
            let path = request.split_whitespace().nth(1).unwrap();
            let (status, body) = match path {
                "/.fs" => (
                    "200 OK",
                    r#"[{"name":"house/README.md"},{"name":"planning/README.md"},{"name":"reminders.md"}]"#,
                ),
                path if path.strip_prefix("/.fs/") == Some(failed) => (status, "unavailable"),
                "/.fs/house/README.md" => ("200 OK", "# House"),
                "/.fs/planning/README.md" => ("200 OK", "## Key dates\n- 2031-04-08 Visit"),
                "/.fs/reminders.md" => (
                    "200 OK",
                    "| ID | What | Deadline |\n|---|---|---|\n| house-1 | Visit | 2031-04-08 |",
                ),
                _ => ("404 Not Found", ""),
            };
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        loading.join().unwrap().unwrap()
    })
}

#[test]
fn one_unreadable_agenda_page_keeps_the_other_source_and_topics_available() {
    let missing_plan = load_with_failed_page(model::PLANNING_PAGE, "503 Service Unavailable");
    assert!(missing_plan.warning.unwrap().contains(model::PLANNING_PAGE));
    assert!(!missing_plan.ws.reminders_unavailable);
    assert_eq!(missing_plan.ws.find("house").unwrap().agenda.len(), 1);

    let missing_reminders = load_with_failed_page(REMINDERS_PAGE, "503 Service Unavailable");
    assert!(missing_reminders.warning.unwrap().contains(REMINDERS_PAGE));
    assert!(missing_reminders.ws.reminders_unavailable);
    assert_eq!(missing_reminders.ws.key_dates.len(), 1);
    assert!(missing_reminders.pages.contains_key("house/README.md"));
}

#[test]
fn an_agenda_page_deleted_after_listing_is_absent() {
    let loaded = load_with_failed_page(REMINDERS_PAGE, "404 Not Found");
    assert!(loaded.warning.is_none());
    assert!(!loaded.ws.reminders_unavailable);
    assert_eq!(loaded.ws.key_dates.len(), 1);
}
