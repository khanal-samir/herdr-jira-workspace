//! One chronological timeline for Jira comments, field history, and logged work.
use crate::{
    calendar::{duration_text, parse_started},
    jira::{Comment, Worklog},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Comment,
    History,
    Worklog,
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Comment => "Comment",
            Self::History => "History",
            Self::Worklog => "Worklog",
        }
    }
}
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub resource_id: String,
    pub kind: Kind,
    pub at: String,
    pub author: String,
    pub title: String,
    pub body: String,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Comments,
    History,
    Worklogs,
}
impl Filter {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Comments,
            Self::Comments => Self::History,
            Self::History => Self::Worklogs,
            Self::Worklogs => Self::All,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Comments => "Comments",
            Self::History => "History",
            Self::Worklogs => "Worklogs",
        }
    }
    pub fn accepts(self, kind: Kind) -> bool {
        match self {
            Self::All => true,
            Self::Comments => kind == Kind::Comment,
            Self::History => kind == Kind::History,
            Self::Worklogs => kind == Kind::Worklog,
        }
    }
}
pub fn timeline(comments: &[Comment], worklogs: &[Worklog], history: Vec<Entry>) -> Vec<Entry> {
    let mut entries = history;
    entries.extend(comments.iter().map(|c| Entry {
        id: format!("comment:{}", c.id),
        resource_id: c.id.clone(),
        kind: Kind::Comment,
        at: c.created.clone(),
        author: c.author.clone(),
        title: if !c.updated.is_empty() && c.updated != c.created {
            "Commented · edited".into()
        } else {
            "Commented".into()
        },
        body: if !c.updated.is_empty() && c.updated != c.created {
            format!("{}\n\nEdited: {}", c.text, c.updated)
        } else {
            c.text.clone()
        },
    }));
    entries.extend(worklogs.iter().map(|l| Entry {
        id: format!("worklog:{}", l.id),
        resource_id: l.id.clone(),
        kind: Kind::Worklog,
        at: if l.created.is_empty() {
            l.started.clone()
        } else {
            l.created.clone()
        },
        author: l.author.clone(),
        title: format!("Logged {}", duration_text(l.seconds)),
        body: format!(
            "Started: {}\n{}{}",
            l.started,
            l.text,
            if !l.updated.is_empty() && l.updated != l.created {
                format!("\n\nEdited: {}", l.updated)
            } else {
                String::new()
            }
        ),
    }));
    entries.sort_by(|a, b| {
        parse_started(&b.at)
            .cmp(&parse_started(&a.at))
            .then_with(|| a.id.cmp(&b.id))
    });
    entries
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_activity_is_sorted_by_actual_time_not_offset_text() {
        let comments = [Comment {
            id: "1".into(),
            created: "2026-10-10T12:00:00+0545".into(),
            text: "Earlier".into(),
            ..Default::default()
        }];
        let logs = [Worklog {
            id: "1".into(),
            created: "2026-10-10T07:00:00Z".into(),
            started: "2026-10-01T08:00:00Z".into(),
            seconds: 3600,
            ..Default::default()
        }];
        let entries = timeline(&comments, &logs, Vec::new());
        assert_eq!(entries[0].kind, Kind::Worklog);
        assert_eq!(entries[1].kind, Kind::Comment);
        assert_ne!(entries[0].id, entries[1].id);
    }
    #[test]
    fn invalid_timestamps_are_visible_and_sort_after_valid_entries() {
        let comments = [
            Comment {
                id: "a".into(),
                created: "broken".into(),
                ..Default::default()
            },
            Comment {
                id: "b".into(),
                created: "2026-10-10T00:00:00Z".into(),
                updated: "2026-10-11T00:00:00Z".into(),
                ..Default::default()
            },
        ];
        let entries = timeline(&comments, &[], Vec::new());
        assert_eq!(entries[0].resource_id, "b");
        assert!(entries[0].body.contains("Edited:"));
        assert_eq!(entries[1].resource_id, "a");
        assert!(Filter::Comments.accepts(entries[0].kind));
        assert!(!Filter::History.accepts(entries[0].kind));
    }
}
