# Jira Workspace for Herdr

A native, keyboard-first Jira workspace **inside Herdr**. Real Kanban columns,
calendar-based worklogs, Jira-style ticket activity, user mentions, and Herdr
notifications—no web server or browser companion.

**Release: 0.2.0.** Activity, tagging, and notifications were verified in actual
Herdr panes, including read-only Jira Cloud checks. The **84 automated tests**
use localhost mocks and offline rendering. Live Jira writes still need an
acceptance check on a disposable ticket. See [what changed](CHANGELOG.md).

## Features

- Choose an actual Jira Software board; load its configured columns, status
  mappings, WIP limits, and every issue page. Cloud uses the enhanced,
  token-paginated board endpoint; Server/DC uses offset pagination.
- Navigate cards, move with workflow confirmation, drag between columns,
  and rank cards within a column. Jira permissions and rank configuration apply.
- Filter loaded tickets by key, summary, assignee, priority, or labels.
- Open a ticket directly into **Activity**: comments, field history, and worklogs
  in one newest-first feed, with All/Comments/History/Worklogs filters. Keep
  dedicated Details, Comments, Worklogs, and Attachments panels too.
- Edit summary, add/remove labels, find an assignee, and choose a priority.
- Log work with a **clickable month-grid calendar**, start time, duration,
  multiline work description, and remaining-estimate controls.
- Browse personal/team week or month worklog calendars, daily/monthly totals,
  and worklog edit/delete actions. **Calendar scope is tickets currently
  returned by the chosen board**, not every issue on your Jira site.
- Read paginated comments, add/edit/delete, and quote-reply.
- Type **`@`** in comments or work descriptions to browse active visible users.
  Type a name to search; Enter/Tab inserts a real Jira mention. Tagged names
  render without account IDs; caret movement/deletion treats tags atomically.
  Editing existing Cloud mentions preserves their identities.
- Background selected-board monitoring, an unread badge, **`N` activity inbox**,
  and native Herdr toasts. Quiet startup, batched alerts, error backoff, and
  cross-pane delivery locking prevent notification spam.
- Secure attachment downloads. PNG previews use Herdr's experimental pane
  graphics if available; other files or unsupported Herdr versions use your
  system viewer. `o` opens the Jira ticket in your browser.
- Unicode-safe caret editing, multiline paste, explicit save/cancel, and retained
  drafts after failures. In-flight writes cannot be double-submitted.
- Offline demo mode: no credentials, network requests, or Jira mutations.

## Requirements

- Linux or macOS
- Herdr >= 0.7.0 (developed with 0.9.0)
- Rust >= 1.89 (a current stable toolchain is recommended) for installation
- Jira Software board access; Cloud email/API token or Server/DC PAT
- Jira time tracking enabled and appropriate permissions
- `xdg-open` (Linux) or `open` (macOS) for browser/external viewer actions

Use a full tab or zoomed pane. Recommended size: **100 × 35** terminal cells.
The board adapts down to 40 × 14; the worklog dialog needs at least 62 × 24.

## Install / develop

From this checkout:

```sh
cargo build --release --locked
herdr plugin link "$PWD"
herdr plugin action invoke herdr-jira-workspace.open
# Safe preview without Jira credentials:
herdr plugin action invoke herdr-jira-workspace.demo
```

Install from the public repository:

```sh
herdr plugin install khanal-samir/herdr-jira-workspace --yes
```

A private repository requires GitHub access on the installing machine. Marketplace
discovery requires a public repository with the `herdr-plugin` GitHub topic.

### Configuration

If this plugin has no config, it reads the existing `herdr-jira` plugin's
`[jira]` configuration. It does **not** copy credentials, change that plugin,
or overwrite its configuration.

For independent configuration:

```sh
mkdir -p "$(herdr plugin config-dir herdr-jira-workspace)"
cp config.example.toml "$(herdr plugin config-dir herdr-jira-workspace)/config.toml"
chmod 600 "$(herdr plugin config-dir herdr-jira-workspace)/config.toml"
```

Edit that file outside the repository. Prefer `api_token_cmd` with a credential
store. Set `board_id` to skip the board picker, or leave it at zero.

```sh
./target/release/herdr-jira-workspace --check
./target/release/herdr-jira-workspace --demo
./target/release/herdr-jira-workspace --dump-demo
```

`--check` validates configuration without executing the token command.
Configuration is loaded at launch; restart the plugin after changing it.

### Notifications

Monitoring is enabled by default. Optional configuration, alongside `[jira]`:

```toml
[notifications]
enabled = true
interval_seconds = 60 # clamped to 30–3600
```

Choose a board and keep its workspace running. The initial snapshot is silent;
subsequent issue changes, additions, and departures enter the bounded,
session-local inbox (200 entries). Changes are detected at poll time, not in
real time. A departure means the ticket left the board—not that it was deleted.
Opening an alert loads that ticket's activity.

**This is not Jira's private notification inbox.** It watches the selected board,
not every mention, assigned issue, or project on the site. It cannot recover
events while the plugin is closed, and the inbox resets on exit. Confirmed
writes in the same pane are suppressed; another pane's changes may still alert.
Native toasts respect Herdr's delivery policy, and alerts remain in the plugin
inbox when a toast cannot be shown. Only one pane per board delivers toasts;
each pane maintains its own inbox. `m` mutes toasts, not collection.

### Optional Herdr keybinding

Add a **free** key to your Herdr config (do not overwrite existing Jira keys):

```toml
[[keys.command]]
key = "prefix+shift+k"
type = "plugin_action"
command = "herdr-jira-workspace.open"
description = "Open Jira Workspace"
```

Reload Herdr configuration using its normal reload command.

## Shortcuts

| Context | Keys |
| --- | --- |
| Board | Arrows navigate; Enter opens; double-click opens |
| Move ticket | `m` picks a workflow transition; Shift+Left/Right targets a column; mouse drag also stages a move |
| Reorder | Shift+Up/Down; Jira ranking permissions required |
| Common | `a` activity; `N` inbox; `w` log work; `g` calendar; `c` comments; `i` attachments; `l` labels; `e` edit; `o` browser |
| Board controls | `b` choose board; `/` filter; `r` refresh; `?` help; `q` quit |
| Detail panels | Tab / Shift+Tab switches; Up/Down selects; Page Up/Down scrolls |
| Activity | `f` cycles filters; Up/Down selects; `n` comment; `e/p/D` act on the selected comment/worklog; history is read-only |
| Comments | `n` new; `e` edit; `p` quoted reply; `D` confirm deletion |
| Mention picker | `@` browse; type a name; arrows select; Enter/Tab tags; Page Down loads more; Esc dismisses without losing the draft |
| Inbox | `N` opens; Up/Down selects; Enter opens activity/marks read; `a` marks all read; `m` mutes/unmutes toasts |
| Worklogs | `w` new; `e` edit; `D` confirm deletion |
| Attachments | Enter downloads and previews |
| Calendar | Arrows choose day/week; Page Up/Down month; `j/k` selects entry; `a` mine/team; `v` week/month; `t` today |
| Form | Tab / Shift+Tab field; Ctrl+S or Ctrl+Enter save; Esc cancel; Ctrl+U clear field |
| Calendar field | Arrows choose date; Page Up/Down month; Enter advances |
| Estimate field | Left/Right or Space cycles leave/auto/new |
| Text editing | Left/Right caret; Home/End line; Up/Down multiline; Backspace/Delete; Ctrl+A/E start/end |

While a read is loading, Esc dismisses its result and `q` exits. Requests already
running may finish in the background. While a write is pending, controls are
locked until its outcome is known.

## Safety and current limits

- No automatic retries of writes. A network failure can mean the save succeeded;
  inspect Jira before retrying to avoid duplicate worklogs/comments.
- Deletes require confirmation. Permissions are enforced by Jira.
- Label changes use incremental add/remove operations to preserve unrelated labels.
- Failed forms keep their contents in memory. Drafts are **not** persisted across
  process exit or crash.
- Partial worklog reads are marked **INCOMPLETE**; totals are not presented as complete.
- Cloud field history is paginated. Data Center's changelog expansion can be
  truncated; an **INCOMPLETE** warning makes this explicit. Permission failures
  warn rather than masquerading as an empty history.
- User lookup respects Jira permissions and privacy; it cannot list hidden users.
  Cloud name search has a documented first-1000-user limitation. Bare `@`
  directory browsing is separately paginated; it requires Browse users/groups.
- Local timestamps include an explicit offset. Ambiguous/nonexistent daylight
  saving times are rejected.
- Complex workflow transitions requiring extra fields must be completed in Jira.
- Replies are new comments containing a quoted original and a mention, **not native
  threaded replies**. Threaded-comment parity needs site/API capability verification.
- Reading rich descriptions/comments is supported; editing a comment replaces its
  rich formatting with paragraphs and mentions. No rich-text editor or inline upload.
- Image references appear in text; attachment images are previewed separately.
  Experimental inline graphics are capability-detected, not required.
- Attachments have a 20 MiB download limit. External-viewer cache files are stored in
  a private plugin state directory; clear the attachment cache when no longer needed.
- Board creation, workflow administration, sprint administration, issue creation,
  custom-field editors, swimlanes, web quick-filter parity, and Jira's private
  notification inbox are not implemented. Background polling refreshes board
  snapshots, but open ticket details still require `r` to refresh.
- Agent delegation is not implemented; the old plugin can coexist if retained.

## Development and tests

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

See [0.2 verification](docs/release-0.2-verification.md), [the original verification report](docs/verification.md), [the test matrix](docs/testing.md),
and [the implementation plan](docs/plan.md).
CI runs formatting, Clippy, tests, release build, and demo output on Linux/macOS.
No CI credentials or real Jira instance are needed.

## Credits

Authentication and ADF handling were informed by
[a2u/herdr-jira](https://github.com/a2u/herdr-jira), which declares the MIT license.
The UI and board/calendar workflows in this plugin are separate. See [NOTICE](NOTICE).

MIT license.
