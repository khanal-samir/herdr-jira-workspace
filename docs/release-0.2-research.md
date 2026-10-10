# 0.2: activity, mentions, and Herdr alerts

Primary sources checked for this release:

- [Cloud issue changelogs](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issues/):
  GET /rest/api/3/issue/{key}/changelog returns oldest-first paginated values.
  Read every page, then combine with comments/worklogs and sort by actual timestamps.
- [Data Center issue expansion](https://developer.atlassian.com/server/jira/platform/rest/v10005/api-group-issue/):
  expand=changelog exposes histories. If total exceeds the returned expansion,
  explicitly mark history incomplete; do not invent unsupported paging parameters.
- [Cloud users](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-users/):
  GET /users/search lists users with startAt/maxResults. Filter inactive, deleted,
  and app accounts; do not display private email addresses.
- [Cloud user search](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-user-search/):
  GET /user/search?query=... is permission/privacy constrained and documented as
  limited to the first 1000 users. Browse the paginated directory with an empty
  query when a search result is missing. No promise to bypass Jira permissions.
- [Data Center user search](https://developer.atlassian.com/server/jira/platform/rest/v10002/api-group-user/):
  GET /user/search uses username, startAt, maxResults, includeActive/includeInactive.
  Bare-query support depends on the server; errors remain visible without losing drafts.
- [Herdr notification API](https://herdr.ink/docs/socket-api):
  notification.show accepts title/body and optional sound/position. Success is an
  id/result envelope (no required ok flag); shown=false can be a normal delivery policy.
- Jira's issue notify endpoint creates outbound email. It is not an inbox reader.
  This release therefore detects selected-board changes through read-only polling
  and maintains its own local inbox while the plugin runs, rather than claiming
  to reproduce Jira's private notification inbox.

Plan: activity feed first, then debounced/paginated @ selection, then safe board
polling/native toasts. Preserve all drafts and foreground write safety. Test HTTP
payloads, pagination, timestamp ordering, stale async responses, initial notification
baselines, deduplication, and rendering before publishing 0.2.0.
