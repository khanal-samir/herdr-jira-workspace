# Testing

## Commands

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

The suite is offline. All Jira HTTP tests use an ephemeral loopback server with
synthetic credentials. It checks complete JSON mutation bodies, HTTP verbs,
endpoint paths, query parameters, and authorization headers.

## Coverage matrix

| Area | Cases |
| --- | --- |
| Calendar | Leap-year day count, Jan 31 → February clamping, year rollover, Monday-first grid, complete month, explicit-offset timestamps, invalid time |
| Text editing | Unicode insertion/deletion in the middle, caret boundaries, multiline up/down, clear-field shortcut, malformed caret normalization |
| Board UI/state | Actual columns, complete paging, local filter, keyboard movement with confirmation, mouse drag, no premature mutation |
| Asynchronous state | Duplicate-save prevention, cancel vs in-flight write, stale read dismissal, exit during reads |
| Worklog requests | Cloud ADF, Server/DC plain text, create vs PUT edit, duration/start time, remaining estimate modes, leave estimate on deletion |
| Comments | New/edit endpoints, multiline ADF, mentions, quoted reply composition, links and image references when reading |
| Labels/rank | Incremental operations preserving unrelated labels, no-op labels, rank field and neighbor payload |
| Pagination | Offset pages, Cloud tokens, final page, repeated offsets/tokens, missing arrays/tokens |
| Permissions | HTTP 403 diagnostics, restricted comments or disabled time tracking still show ticket details |
| Security | No external authenticated redirects, attachment origin/ID/size validation, non-HTTPS browser URLs, secret-free config errors |
| CLI | Help/invalid flags, config validation without token command, offline demo never resolves credentials |
| Rendering | Board and log dialog contents, tiny/normal/large panes, empty board, small-dialog size hint |

## Manual acceptance (use a disposable issue)

Automated HTTP checks do not prove the user's Jira workflow/permissions match
the fixtures. Before daily use:

1. Select a real board and compare its columns/card membership to Jira web.
2. Move a disposable card and verify its actual Jira status; cancel a second move.
3. Reorder two disposable cards and verify rank; test a user without rank permission.
4. Add/remove a label while another user adds a different label; verify it remains.
5. Log a small duration on a selected date/time and compare timestamp and estimate in Jira.
6. Edit/delete that worklog; inspect calendar totals and try restricted worklog permissions.
7. Add a multiline comment with a mention; quote reply; confirm and cancel deletions.
8. Simulate a failed save; verify the draft remains and no automatic retries occur.
9. Open a PNG attachment with graphics disabled/enabled. Check external viewer fallback.
10. Resize the pane, test keyboard-only operation, mouse drag, and bracketed paste.

Never use a real active ticket merely to exercise write tests.
