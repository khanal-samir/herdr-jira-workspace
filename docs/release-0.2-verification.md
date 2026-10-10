# 0.2.0 verification

## Automated

Passed locally:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

**79 unit tests + 5 CLI integration tests = 84 passing tests.**

New coverage includes activity timestamp ordering and filters, all Cloud history
pages, Data Center truncation/permission warnings, safe plain-text worklog edits,
directory browsing/name search/pagination/privacy, real mention bodies and
identity preservation, atomic tag editing, stale suggestions, draft retention,
notification baselines/deduplication/backoff/batching, cross-pane locks, own-save
suppression, Herdr delivery-policy envelopes, background/foreground isolation,
and rendering at small and normal pane sizes.

HTTP tests use synthetic credentials and ephemeral localhost servers.
No test requires access to a real Jira site.

## Actual Herdr panes

A temporary, separately registered smoke plugin ran the release binary, without
replacing the installed plugin during testing.

Offline demo checks passed inside native panes:

- Open ticket activity; switch to Comments filter.
- Compose `Native @Sa`, select a person, and save with Ctrl+S.
- Confirm the saved text renders the person's name.
- Open the activity inbox, acknowledge an alert, and return to ticket activity.

Read-only Jira Cloud checks passed using the independent private configuration:

- Discover and select an actual board.
- Load a real ticket's combined activity without a history warning.
- Browse the real user directory with bare `@`.
- Search names in the picker.
- Cancel suggestions and the draft without saving.

A native Herdr notification was accepted with:

```json
{"reason":"shown","shown":true,"type":"notification_show"}
```

**No live Jira comments, worklogs, transitions, edits, or deletes were performed.**

## Remaining acceptance scope

Live mention delivery to a recipient, real mutation permissions, and end-to-end
notifications triggered by a disposable issue's external update still need
explicit acceptance testing. Data Center behavior is covered by mock APIs, not
a live Data Center site. The notification inbox is selected-board change
detection, not Jira's private notification stream.

Use the disposable-ticket checks in [testing.md](testing.md); never mutate an
active issue just to verify the release.
