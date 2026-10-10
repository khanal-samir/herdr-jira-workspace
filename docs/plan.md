# Plan: Native Jira Workspace

Source: user requirements in this conversation: a new plugin, entirely inside
Herdr, with a Jira-like Kanban board, editable labels, calendar-based time entry,
comments/replies, images, browser shortcuts, and proper tests.

## Durable decisions

- Separate plugin ID: `herdr-jira-workspace`; existing plugin remains untouched.
- Native Rust/Ratatui UI with keyboard equivalents for every core action.
- Jira remains authoritative. No local replica/database, web companion, or API proxy.
- Credentials live outside the repository; reuse existing configuration when desired.
- Real Jira Software board metadata and paginated REST results, not hardcoded statuses.
- Shared Jira module owns authentication, Cloud/DC formatting, pagination, and writes.
- Calendar module owns date selection and timezone-safe Jira timestamps.
- Drafts remain in memory until successful save or explicit cancellation.
- Every vertical slice includes behavior/API/UI checks.

## Implemented first-release slices

1. Board selection, real columns, card navigation, workflow-confirmed movement,
   ranking, and filters. Mock HTTP and keyboard/mouse checks.
2. Ticket details and summary/labels/assignee/priority editing. Incremental label writes.
3. Calendar-based worklog entry, duration/start time/comment/estimate controls.
   Calendar, validation, request-body, and failed-save checks.
4. Board-scoped personal/team worklog calendar, totals, editing, confirmed deletion.
   Missing permissions produce incomplete-read warnings.
5. Comment reading, new/edit/delete actions, ADF mentions, quoted replies.
   No promise of native threaded comments until the specific site/API supports them.
6. Attachment browsing, secure download, experimental PNG graphics with a viewer fallback.
7. Offline demo, CLI tests, Linux/macOS CI, and documentation.

## Before daily use

- Validate real board discovery/columns against the user's Jira web board.
- Use a disposable issue to verify movement, rank, labels, comments, and worklogs.
- Verify remaining-estimate changes, permission errors, and timezone expectations.
- Test mouse forwarding and experimental PNG graphics in the installed Herdr client.
- Do not call this full Jira web parity: current limitations are documented in README.

## Follow-up slices, not scaffolding

- Native required-field workflow dialogs and custom-field editors.
- Rich comment editing, native threaded replies if supported, uploads.
- Agent delegation integration, persisted drafts, and incremental refresh.
- Board quick filters/swimlanes and broader worklog scope.

These should each be implemented as a complete UI → Jira → refreshed UI path,
with failure and permission cases, rather than building speculative infrastructure.
