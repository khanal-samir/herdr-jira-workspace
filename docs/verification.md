# First-release verification

Code tested: `0f8c9ad` (0.1.0). This report contains no Jira credentials,
tenant addresses, project names, issue identifiers, or comment/worklog contents.

## Automated checks

- **49 unit tests + 5 CLI integration tests**, zero failures or ignored tests.
- `cargo fmt --check`, strict Clippy, locked tests, locked release build, and
  offline demo rendering all pass.
- [GitHub CI](https://github.com/khanal-samir/herdr-jira-workspace/actions/runs/38023072150)
  passes on **Ubuntu and macOS**.
- Jira mutation tests use an ephemeral loopback server and synthetic credentials.

## Native Herdr checks

Installed from the public GitHub repository, not just a local checkout.

Offline demo inside an actual Herdr plugin-owned tab:

- Kanban board and calendar rendering
- Keyboard start time/duration entry and `Ctrl+S` worklog validation
- Multiline comment input and save
- Month calendar and daily entries
- No live Jira requests or changes in demo mode

Read-only checks using the existing Jira configuration:

- Authentication and board discovery succeed.
- An actual board loads configured columns and all returned issue pages.
- Ticket details, comments, and worklogs load.
- The board-scoped worklog calendar loads.
- The existing Jira plugin and its configuration remain untouched.

**No live Jira write was performed.** Transitions, ranking, edits, worklogs,
comments, and deletion still need the disposable-ticket manual acceptance
checks in [testing.md](testing.md) before daily use.

## Publication

- Public repository: https://github.com/khanal-samir/herdr-jira-workspace
- Root `herdr-plugin.toml` manifest
- GitHub topic: `herdr-plugin`
- The Herdr marketplace indexes this topic automatically every 30 minutes.
  Tagging makes the plugin eligible; it is not a reviewed/endorsed listing.
