//! Read-only selected-board change detection and a bounded, session-local inbox.
use crate::{
    config::NotificationsConfig,
    jira::{Board, Issue},
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{File, OpenOptions, TryLockError},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct Notice {
    pub key: String,
    pub title: String,
    pub body: String,
    pub at: String,
    pub unread: bool,
}
pub struct Monitor {
    pub config: NotificationsConfig,
    pub board_id: u64,
    pub inbox: VecDeque<Notice>,
    pub muted: bool,
    pub warning: Option<String>,
    pub delivery_warning: Option<String>,
    baseline: HashMap<String, Issue>,
    quiet: HashSet<String>,
    version: u64,
    inflight: Option<u64>,
    next: Instant,
    failures: u32,
}
impl Monitor {
    pub fn new(config: NotificationsConfig, now: Instant) -> Self {
        Self {
            config,
            board_id: 0,
            inbox: VecDeque::new(),
            muted: false,
            warning: None,
            delivery_warning: None,
            baseline: HashMap::new(),
            quiet: HashSet::new(),
            version: 0,
            inflight: None,
            next: now,
            failures: 0,
        }
    }
    fn period(&self) -> Duration {
        Duration::from_secs(self.config.interval_seconds.clamp(30, 3600))
    }
    pub fn observe(&mut self, board: &Board, now: Instant) {
        self.version += 1;
        self.board_id = board.id;
        self.baseline = board
            .issues
            .iter()
            .map(|i| (i.key.clone(), i.clone()))
            .collect();
        self.quiet.clear();
        self.next = now + self.period();
        self.failures = 0;
        self.warning = None;
    }
    pub fn sync_issue(&mut self, issue: &Issue) {
        if self.baseline.contains_key(&issue.key) {
            self.baseline.insert(issue.key.clone(), issue.clone());
            self.quiet.remove(&issue.key);
            self.version += 1;
        }
    }
    pub fn quiet(&mut self, key: String) {
        self.quiet.insert(key);
    }
    pub fn due(&self, now: Instant) -> bool {
        self.config.enabled && self.board_id > 0 && self.inflight.is_none() && now >= self.next
    }
    pub fn begin_poll(&mut self) -> u64 {
        self.inflight = Some(self.version);
        self.version
    }
    pub fn discard(&mut self, version: u64, now: Instant) {
        if self.inflight == Some(version) {
            self.inflight = None;
            self.next = now + self.period();
        }
    }
    pub fn finish(
        &mut self,
        version: u64,
        result: Result<Board, String>,
        now: Instant,
    ) -> Option<(Board, Vec<Notice>)> {
        self.discard(version, now);
        if version != self.version {
            return None;
        }
        let board = match result {
            Ok(board) if board.id == self.board_id => board,
            Ok(_) => return None,
            Err(error) => {
                self.failures = (self.failures + 1).min(4);
                self.next = now + self.period().saturating_mul(1 << self.failures);
                self.warning = Some(format!(
                    "Activity polling unavailable: {error}. Retrying with backoff."
                ));
                return None;
            }
        };
        let mut notices = Vec::new();
        let mut keys: Vec<_> = board.issues.iter().collect();
        keys.sort_by(|a, b| a.key.cmp(&b.key));
        for issue in keys {
            let body = match self.baseline.get(&issue.key) {
                None => Some("Added to this board".into()),
                Some(old) => difference(old, issue),
            };
            if self.quiet.remove(&issue.key) {
                continue;
            }
            if let Some(body) = body {
                notices.push(Notice {
                    key: issue.key.clone(),
                    title: format!("Jira · {}", issue.key),
                    body: format!("{body} · {}", issue.summary),
                    at: chrono::Local::now().to_rfc3339(),
                    unread: true,
                });
            }
        }
        let current: HashSet<_> = board.issues.iter().map(|i| i.key.as_str()).collect();
        let mut removed: Vec<_> = self
            .baseline
            .values()
            .filter(|i| !current.contains(i.key.as_str()))
            .collect();
        removed.sort_by(|a, b| a.key.cmp(&b.key));
        for issue in removed {
            if self.quiet.remove(&issue.key) {
                continue;
            }
            notices.push(Notice {
                key: issue.key.clone(),
                title: format!("Jira · {}", issue.key),
                body: "Left this board (not necessarily deleted)".into(),
                at: chrono::Local::now().to_rfc3339(),
                unread: true,
            });
        }
        self.baseline = board
            .issues
            .iter()
            .map(|i| (i.key.clone(), i.clone()))
            .collect();
        self.quiet.clear();
        self.failures = 0;
        self.warning = None;
        for notice in &notices {
            self.inbox.push_front(notice.clone());
        }
        self.inbox.truncate(200);
        Some((board, notices))
    }
    pub fn unread(&self) -> usize {
        self.inbox.iter().filter(|n| n.unread).count()
    }
    pub fn mark_all_read(&mut self) {
        for n in &mut self.inbox {
            n.unread = false;
        }
    }
}
// ponytail: snapshots coalesce changes between polls; use a webhook stream if every event is required.
fn difference(old: &Issue, new: &Issue) -> Option<String> {
    let mut changes = Vec::new();
    if old.status != new.status {
        changes.push(format!("Status: {} → {}", old.status, new.status));
    }
    if old.assignee != new.assignee {
        changes.push(format!("Assignee: {}", new.assignee));
    }
    if old.priority != new.priority {
        changes.push(format!("Priority: {}", new.priority));
    }
    if old.labels != new.labels {
        changes.push("Labels changed".into());
    }
    if old.summary != new.summary {
        changes.push("Summary changed".into());
    }
    if old.time_spent != new.time_spent {
        changes.push(format!(
            "Logged work: {}",
            crate::calendar::duration_text(new.time_spent)
        ));
    }
    if changes.is_empty() && (old.updated != new.updated || old.description != new.description) {
        changes.push("Activity updated (comments, history or other fields)".into());
    }
    (!changes.is_empty()).then(|| changes.join(" · "))
}

/// One long-held OS lock per board prevents duplicate toasts from multiple panes.
pub fn delivery_lock(board_id: u64) -> Result<Option<File>, String> {
    let root = std::env::var_os("HERDR_PLUGIN_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join(".local/state/herdr/plugins/herdr-jira-workspace")
        })
        .join("notifications");
    lock_at(&root, board_id)
}
fn lock_at(root: &Path, board_id: u64) -> Result<Option<File>, String> {
    std::fs::create_dir_all(root).map_err(|_| "Could not create notification state directory")?;
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "Could not protect notification state directory")?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(root.join(format!("board-{board_id}.lock")))
        .map_err(|_| "Could not open notification lock")?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(_) => Err("Could not lock notification delivery".into()),
    }
}
pub fn batch(notices: &[Notice]) -> Option<(String, String)> {
    let first = notices.first()?;
    if notices.len() == 1 {
        Some((first.title.clone(), first.body.clone()))
    } else {
        Some((
            format!("Jira · {} board updates", notices.len()),
            format!(
                "{} · N opens the activity inbox",
                notices
                    .iter()
                    .take(5)
                    .map(|n| n.key.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn monitor() -> (Monitor, Board, Instant) {
        let now = Instant::now();
        let mut m = Monitor::new(NotificationsConfig::default(), now);
        let b = crate::app::demo_board();
        m.observe(&b, now);
        (m, b, now)
    }
    #[test]
    fn startup_is_quiet_and_identical_polls_do_not_repeat_notifications() {
        let (mut m, mut b, now) = monitor();
        assert!(m.inbox.is_empty());
        assert!(!m.due(now));
        assert!(m.due(now + Duration::from_secs(60)));
        b.issues[0].status = "In progress".into();
        let v = m.begin_poll();
        let (_, notices) = m.finish(v, Ok(b.clone()), now).unwrap();
        assert_eq!(notices.len(), 1);
        assert!(notices[0].body.contains("In progress"));
        let v = m.begin_poll();
        assert!(m.finish(v, Ok(b), now).unwrap().1.is_empty());
        assert_eq!(m.unread(), 1);
        m.mark_all_read();
        assert_eq!(m.unread(), 0);
    }
    #[test]
    fn failed_polls_keep_the_baseline_and_back_off() {
        let (mut m, mut b, now) = monitor();
        let v = m.begin_poll();
        assert!(m.finish(v, Err("HTTP 429".into()), now).is_none());
        assert!(!m.due(now + Duration::from_secs(61)));
        assert!(m.due(now + Duration::from_secs(121)));
        assert!(m.warning.is_some());
        b.issues[0].updated = "changed".into();
        let v = m.begin_poll();
        assert_eq!(m.finish(v, Ok(b), now).unwrap().1.len(), 1);
        assert!(m.warning.is_none());
    }
    #[test]
    fn stale_polls_and_own_saves_are_quiet() {
        let (mut m, mut b, now) = monitor();
        let v = m.begin_poll();
        m.observe(&b, now);
        assert!(m.finish(v, Ok(b.clone()), now).is_none());
        m.quiet(b.issues[0].key.clone());
        b.issues[0].updated = "own save".into();
        let v = m.begin_poll();
        assert!(m.finish(v, Ok(b), now).unwrap().1.is_empty());
    }
    #[test]
    fn joining_leaving_and_batched_notifications_are_not_deletion_claims() {
        let (mut m, mut b, now) = monitor();
        b.issues.remove(0);
        b.issues.push(Issue {
            key: "NEW-1".into(),
            ..Default::default()
        });
        let v = m.begin_poll();
        let (_, notices) = m.finish(v, Ok(b), now).unwrap();
        assert_eq!(notices.len(), 2);
        assert!(notices
            .iter()
            .any(|n| n.body.contains("not necessarily deleted")));
        assert!(batch(&notices).unwrap().0.contains("2 board updates"));
    }
    #[test]
    fn seeing_own_save_does_not_suppress_the_next_external_update() {
        let (mut m, mut b, now) = monitor();
        m.quiet(b.issues[0].key.clone());
        b.issues[0].updated = "own save".into();
        m.sync_issue(&b.issues[0]);
        b.issues[0].updated = "external comment".into();
        let v = m.begin_poll();
        assert_eq!(m.finish(v, Ok(b), now).unwrap().1.len(), 1);
    }
    #[test]
    fn multiple_panes_cannot_own_the_same_board_delivery_lock() {
        let root = std::env::temp_dir().join(format!(
            "jira-lock-{}-{}",
            std::process::id(),
            chrono::Local::now().timestamp_nanos_opt().unwrap()
        ));
        let first = lock_at(&root, 1).unwrap().unwrap();
        assert!(lock_at(&root, 1).unwrap().is_none());
        assert!(lock_at(&root, 2).unwrap().is_some());
        drop(first);
        assert!(lock_at(&root, 1).unwrap().is_some());
        std::fs::remove_dir_all(root).unwrap();
    }
}
