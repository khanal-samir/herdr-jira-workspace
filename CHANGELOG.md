# Changelog

## 0.2.0

- Ticket activity combines comments, field history, and worklogs, newest first.
  Filters and resource-specific edit/reply/delete controls preserve dedicated tabs.
- Paginated Cloud changelogs; explicit warnings for restricted or truncated
  Data Center history.
- `@` user picker in comments/work descriptions: browse, name search, pagination,
  keyboard/mouse selection, Unicode-safe insertion, and stale-response rejection.
- Real Cloud ADF/Data Center wiki mentions; existing Cloud mention identities
  survive edits. Tags display as names and move/delete atomically.
- Selected-board change alerts: quiet baseline, configurable polling, unread
  badge, session inbox, batched Herdr toasts, failed-poll backoff, own-save
  suppression, and cross-pane delivery locks.
- Background reads do not interrupt forms or unlock pending writes.
- Herdr socket handling accepts native `id/result` envelopes and normal
  notification delivery-policy decisions.
- 84 tests; native Herdr and read-only Jira Cloud verification.
- Requires Rust 1.89 or newer to build. Existing Jira configuration remains valid.

Notifications cover the selected board while the workspace runs. They are not
Jira's private notification inbox or a site-wide mention subscription.

## 0.1.0

Initial native Herdr release: actual Kanban boards, workflow moves, ranking,
ticket editing, calendar worklogs, comments, attachment previews, and offline demo.
