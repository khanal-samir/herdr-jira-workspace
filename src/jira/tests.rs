use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    time::Instant,
};

struct Expected {
    method: &'static str,
    path: &'static str,
    status: u16,
    response: Value,
    body: Option<Value>,
}

fn get(path: &'static str, response: Value) -> Expected {
    Expected {
        method: "GET",
        path,
        status: 200,
        response,
        body: None,
    }
}
fn mutation(method: &'static str, path: &'static str, body: Value) -> Expected {
    Expected {
        method,
        path,
        status: 204,
        response: Value::Null,
        body: Some(body),
    }
}
fn client(base: String, cloud: bool) -> JiraClient {
    let cfg = crate::config::Config {
        jira: crate::config::JiraConfig {
            base_url: base,
            api_token: "test-only-token".into(),
            email: "test@example.com".into(),
            max_results: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut client = JiraClient::new(&cfg).unwrap();
    client.cloud = cloud;
    client
}

/// A real local HTTP server: checks method, URL, authorization, and the complete
/// JSON write payload. No Jira credentials or external network are involved.
fn server(expectations: Vec<Expected>, cloud: bool) -> (JiraClient, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let handle = std::thread::spawn(move || {
        for expected in expectations {
            let deadline = Instant::now() + Duration::from_secs(4);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "Expected {} {} was never requested",
                            expected.method,
                            expected.path
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.starts_with(&format!("{} ", expected.method)), "{line}");
            assert!(line.contains(expected.path), "{line}");
            let mut length = 0;
            let mut authorized = false;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header == "\r\n" {
                    break;
                }
                if let Some((name, value)) = header.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap();
                    }
                    if name.eq_ignore_ascii_case("authorization") {
                        authorized = value.trim().starts_with("Basic ");
                    }
                }
            }
            assert!(authorized, "Missing Jira authorization header");
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let actual = if body.is_empty() {
                None
            } else {
                Some(serde_json::from_slice::<Value>(&body).unwrap())
            };
            assert_eq!(
                actual, expected.body,
                "Unexpected mutation payload for {}",
                expected.path
            );
            let response = if expected.status == 204 {
                String::new()
            } else {
                expected.response.to_string()
            };
            let extra = if expected.status == 302 {
                "Location: https://external.invalid/steal\r\n"
            } else {
                ""
            };
            write!(socket,"HTTP/1.1 {} Mock\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: application/json\r\n{extra}\r\n{response}",expected.status,response.len()).unwrap();
        }
    });
    (client(format!("http://{address}"), cloud), handle)
}

#[test]
fn all_offset_pages_are_fetched() {
    let (c, h) = server(
        vec![
            get(
                "startAt=0",
                json!({"values":[{"id":1,"name":"One"}],"total":2}),
            ),
            get(
                "startAt=1",
                json!({"values":[{"id":2,"name":"Two"}],"total":2}),
            ),
        ],
        false,
    );
    assert_eq!(c.boards().unwrap().len(), 2);
    h.join().unwrap();
}
#[test]
fn token_pages_are_fetched_until_is_last() {
    let (c, h) = server(
        vec![
            get(
                "/issues",
                json!({"issues":[1],"isLast":false,"nextPageToken":"abc"}),
            ),
            get("nextPageToken=abc", json!({"issues":[2],"isLast":true})),
        ],
        true,
    );
    assert_eq!(
        c.pages("/issues", "issues", true, &[]).unwrap(),
        vec![json!(1), json!(2)]
    );
    h.join().unwrap();
}
#[test]
fn repeating_tokens_are_rejected() {
    let (c, h) = server(
        vec![
            get("/issues", json!({"issues":[1],"nextPageToken":"abc"})),
            get(
                "nextPageToken=abc",
                json!({"issues":[2],"nextPageToken":"abc"}),
            ),
        ],
        true,
    );
    assert!(c
        .pages("/issues", "issues", true, &[])
        .unwrap_err()
        .contains("repeated"));
    h.join().unwrap();
}
#[test]
fn repeating_offset_pages_are_rejected() {
    let (c, h) = server(
        vec![
            get("startAt=0", json!({"values":[{"id":1}],"isLast":false})),
            get("startAt=1", json!({"values":[{"id":1}],"isLast":false})),
        ],
        false,
    );
    assert!(c.boards().unwrap_err().contains("repeated"));
    h.join().unwrap();
}
#[test]
fn malformed_pages_are_not_treated_as_an_empty_board() {
    let (c, h) = server(vec![get("/board", json!({"unexpected":[]}))], false);
    assert!(c.boards().is_err());
    h.join().unwrap();
}
#[test]
fn missing_next_token_is_reported() {
    let (c, h) = server(
        vec![get("/issues", json!({"issues":[1],"isLast":false}))],
        true,
    );
    assert!(c.pages("/issues", "issues", true, &[]).is_err());
    h.join().unwrap();
}
#[test]
fn cloud_board_uses_actual_columns_and_enhanced_issue_endpoint() {
    let (c, h) = server(
        vec![
            get(
                "/rest/agile/1.0/board/42/configuration",
                json!({"name":"Delivery","columnConfig":{"columns":[{"name":"Quality assurance","statuses":[{"id":"11"},{"id":"12"}],"max":3}]},"ranking":{"rankCustomFieldId":10020}}),
            ),
            get(
                "/rest/software/1.0/board/42/issue",
                json!({"issues":[{"key":"X-1","fields":{"summary":"Ship","status":{"id":"12","name":"Testing"},"timetracking":{"timeSpentSeconds":600}}}],"isLast":true}),
            ),
        ],
        true,
    );
    let board = c.board(42).unwrap();
    assert_eq!(board.columns[0].name, "Quality assurance");
    assert_eq!(board.columns[0].statuses, vec!["11", "12"]);
    assert_eq!(board.columns[0].max, Some(3));
    assert_eq!(board.issues[0].time_spent, 600);
    h.join().unwrap();
}
#[test]
fn transitions_send_a_transition_id_not_a_status_assignment() {
    let (c, h) = server(
        vec![mutation(
            "POST",
            "/rest/api/3/issue/X-1/transitions",
            json!({"transition":{"id":"31"}}),
        )],
        true,
    );
    c.transition(
        "X-1",
        &Transition {
            id: "31".into(),
            ..Default::default()
        },
    )
    .unwrap();
    h.join().unwrap();
}
#[test]
fn required_workflow_fields_are_reported_without_sending_a_write() {
    let c = client("http://127.0.0.1:1".into(), true);
    let error = c
        .transition(
            "X-1",
            &Transition {
                required_fields: vec!["resolution".into()],
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(error.contains("resolution"));
}
#[test]
fn transition_metadata_detects_required_fields() {
    let (c, h) = server(
        vec![get(
            "expand=transitions.fields",
            json!({"transitions":[{"id":"31","name":"Complete","to":{"id":"9"},"fields":{"resolution":{"required":true,"hasDefaultValue":false},"summary":{"required":true,"hasDefaultValue":true}}}]}),
        )],
        true,
    );
    let result = c.transitions("X-1").unwrap();
    assert_eq!(result[0].required_fields, vec!["resolution"]);
    h.join().unwrap();
}
#[test]
fn cloud_worklog_sends_adf_started_duration_and_estimate_controls() {
    let (c, h) = server(
        vec![mutation(
            "POST",
            "/rest/api/3/issue/X-1/worklog?adjustEstimate=new&newEstimate=2h",
            json!({"timeSpent":"1h 30m","started":"2026-07-26T09:30:00.000+0545","comment":{"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":"Built calendar"}]}]}}),
        )],
        true,
    );
    c.save_worklog(
        "X-1",
        None,
        WorklogInput {
            duration: "1h 30m",
            started: "2026-07-26T09:30:00.000+0545",
            comment: "Built calendar",
            estimate_mode: "new",
            remaining: "2h",
        },
    )
    .unwrap();
    h.join().unwrap();
}
#[test]
fn data_center_worklog_edit_uses_put_and_plain_text() {
    let (c, h) = server(
        vec![mutation(
            "PUT",
            "/rest/api/2/issue/X-1/worklog/9?adjustEstimate=leave",
            json!({"timeSpent":"30m","started":"2026-07-26T09:30:00.000+0000","comment":"Revised entry"}),
        )],
        false,
    );
    c.save_worklog(
        "X-1",
        Some("9"),
        WorklogInput {
            duration: "30m",
            started: "2026-07-26T09:30:00.000+0000",
            comment: "Revised entry",
            estimate_mode: "leave",
            remaining: "",
        },
    )
    .unwrap();
    h.join().unwrap();
}
#[test]
fn invalid_worklog_input_does_not_send_requests() {
    let c = client("http://127.0.0.1:1".into(), true);
    for (duration, mode, remaining) in [("", "leave", ""), ("1h", "invalid", ""), ("1h", "new", "")]
    {
        assert!(c
            .save_worklog(
                "X-1",
                None,
                WorklogInput {
                    duration,
                    started: "x",
                    comment: "",
                    estimate_mode: mode,
                    remaining
                }
            )
            .is_err());
    }
}
#[test]
fn worklog_deletion_leaves_remaining_estimate_unchanged() {
    let (c, h) = server(
        vec![Expected {
            method: "DELETE",
            path: "/worklog/9?adjustEstimate=leave",
            status: 204,
            response: Value::Null,
            body: None,
        }],
        true,
    );
    c.delete("X-1", "worklog", "9").unwrap();
    h.join().unwrap();
}
#[test]
fn labels_use_incremental_operations_preserving_unrelated_labels() {
    let (c, h) = server(
        vec![mutation(
            "PUT",
            "/issue/X-1",
            json!({"update":{"labels":[{"remove":"old"},{"add":"new"}]}}),
        )],
        true,
    );
    c.labels(
        "X-1",
        &["keep".into(), "old".into()],
        &["keep".into(), "new".into()],
    )
    .unwrap();
    h.join().unwrap();
}
#[test]
fn unchanged_labels_send_no_request() {
    let c = client("http://127.0.0.1:1".into(), true);
    c.labels("X-1", &["keep".into()], &["keep".into()]).unwrap();
}
#[test]
fn comment_mentions_and_newlines_are_real_adf_nodes() {
    let (c, h) = server(
        vec![Expected {
            method: "POST",
            path: "/issue/X-1/comment",
            status: 201,
            response: json!({"id":"3"}),
            body: Some(
                json!({"body":{"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":"Hi "},{"type":"mention","attrs":{"id":"abc","text":"@Alex"}}]},{"type":"paragraph","content":[]},{"type":"paragraph","content":[{"type":"text","text":"Thanks"}]}]}}),
            ),
        }],
        true,
    );
    c.save_comment("X-1", None, "Hi @{abc|Alex}\n\nThanks")
        .unwrap();
    h.join().unwrap();
}
#[test]
fn data_center_comment_edit_uses_plain_text() {
    let (c, h) = server(
        vec![mutation(
            "PUT",
            "/rest/api/2/issue/X-1/comment/8",
            json!({"body":"Updated"}),
        )],
        false,
    );
    c.save_comment("X-1", Some("8"), "Updated").unwrap();
    h.join().unwrap();
}
#[test]
fn comment_reading_keeps_mentions_links_and_image_references() {
    let value = json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Hi "},{"type":"mention","attrs":{"text":"@Samir"}}]},{"type":"inlineCard","attrs":{"url":"https://example.com"}},{"type":"media","attrs":{"alt":"Screenshot"}}]});
    let text = document_text(&value);
    assert!(text.contains("@Samir"));
    assert!(text.contains("https://example.com"));
    assert!(text.contains("Screenshot"));
}
#[test]
fn restricted_comments_or_disabled_worklogs_do_not_hide_ticket_details() {
    let (c, h) = server(
        vec![
            get(
                "/issue/X-1?fields=",
                json!({"key":"X-1","fields":{"summary":"Visible ticket","attachment":[]}}),
            ),
            Expected {
                status: 403,
                ..get(
                    "/comment",
                    json!({"errorMessages":["No comment permission"]}),
                )
            },
            Expected {
                status: 400,
                ..get(
                    "/worklog",
                    json!({"errorMessages":["Time tracking disabled"]}),
                )
            },
        ],
        true,
    );
    let details = c.details("X-1").unwrap();
    assert_eq!(details.issue.summary, "Visible ticket");
    assert_eq!(details.warnings.len(), 2);
    assert!(details.comments.is_empty());
    h.join().unwrap();
}
#[test]
fn permission_errors_include_actionable_jira_messages() {
    let (c, h) = server(
        vec![Expected {
            status: 403,
            ..get(
                "/board",
                json!({"errorMessages":["No project permission"],"errors":{"assignee":"User not assignable"}}),
            )
        }],
        true,
    );
    let error = c.boards().unwrap_err();
    assert!(error.contains("HTTP 403"));
    assert!(error.contains("User not assignable"));
    assert!(!error.contains("test-only-token"));
    h.join().unwrap();
}
#[test]
fn authenticated_requests_do_not_follow_external_redirects() {
    let (c, h) = server(
        vec![Expected {
            status: 302,
            ..get("/board", json!({}))
        }],
        false,
    );
    assert!(c.boards().unwrap_err().contains("redirect"));
    h.join().unwrap();
}
#[test]
fn attachment_size_ids_and_origins_are_checked_before_download() {
    let c = client("http://127.0.0.1:1".into(), false);
    for attachment in [
        Attachment {
            id: "../secret".into(),
            ..Default::default()
        },
        Attachment {
            id: "1".into(),
            size: 21 * 1024 * 1024,
            ..Default::default()
        },
        Attachment {
            id: "1".into(),
            content_url: "https://external.invalid/image.png".into(),
            ..Default::default()
        },
    ] {
        assert!(c.attachment(&attachment).is_err());
    }
}
#[test]
fn network_failures_on_writes_warn_against_duplicate_retries() {
    let c = client("http://127.0.0.1:1".into(), true);
    assert!(c
        .save_comment("X-1", None, "Hello")
        .unwrap_err()
        .contains("outcome unknown"));
}
#[test]
fn ranking_sends_the_board_rank_field_and_neighbor() {
    let (c, h) = server(
        vec![mutation(
            "PUT",
            "/rest/agile/1.0/issue/rank",
            json!({"issues":["X-1"],"rankBeforeIssue":"X-2","rankCustomFieldId":10020}),
        )],
        true,
    );
    c.rank(
        &Board {
            rank_field: Some(10020),
            ..Default::default()
        },
        "X-1",
        "X-2",
        true,
    )
    .unwrap();
    h.join().unwrap();
}
