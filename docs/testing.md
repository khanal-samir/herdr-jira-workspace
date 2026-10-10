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
| Asynchronous state | Duplicate-save prevention, cancel vs in-flight write, stale read dismissal, exit during reads, background responses preserve forms and write locks |
| Worklog requests | Cloud ADF, Server/DC plain text, create vs PUT edit, duration/start time, remaining estimate modes, leave estimate on deletion |
| Comments | New/edit endpoints, multiline ADF, mentions, quoted reply composition, links and image references when reading |
| Activity | True timestamp order, edited metadata, kind filters, Cloud history pages, Data Center truncation and permission warnings |
| Mention picker | Browse/search API variants, raw-page offsets, inactive account filtering, debounce, stale results, Unicode insertion, identity preservation, atomic editing |
| Notifications | Quiet baseline, change detection, joining/leaving, batching, deduplication, stale responses, own-save suppression, failed-poll backoff, delivery locks, native envelopes |
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

### 0.2 activity, mentions, and alerts

Read-only checks can use an ordinary ticket:

- Compare the All/Comments/History/Worklogs feed to Jira. Check chronological
  order and restricted/truncated-history warnings.
- Type bare `@`, search a name, page/scroll results, and pick by keyboard/mouse.
  Cancel the draft afterward. A forbidden lookup must show a useful error and
  retain the composer.
- Edit an existing mention in a draft and cancel: its account identity must
  remain intact, and tags must navigate/delete atomically.

For live acceptance, use an **explicitly approved disposable issue**:

- Save a mention and verify both the Jira mention and recipient delivery,
  subject to Jira permissions and notification settings. Edit it without
  changing the tagged account; repeat in a work description.
- Start the workspace with notifications enabled; the initial snapshot must
  produce no historical alerts. Change the disposable ticket from another
  session. After a poll, verify the badge, `N` inbox, and Herdr toast.
- Enter the alert and verify ticket activity; mark all read with `a`.
- Mute with `m`: collection continues without toasts. Open a second pane on
  the same board and confirm at most one pane delivers each toast.
- Confirm a successful save in the same pane is quiet, but the next external
  change is not suppressed.
- Disconnect/reconnect networking: polling backs off, preserves its baseline
  and inbox, and resumes without interrupting a comment/worklog draft.

Alerts monitor the selected board while the process runs. They are not Jira's
private notification inbox; multiple changes between polls may coalesce.

Never use a real active ticket merely to exercise write tests.
