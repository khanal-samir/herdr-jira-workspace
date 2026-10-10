//! Mention completion never owns or replaces the enclosing draft.
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub account_id: String,
    pub name: String,
}
#[derive(Debug, Clone, Default)]
pub struct UserPage {
    pub users: Vec<User>,
    pub more: bool,
    pub next_offset: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    pub start: usize,
    pub end: usize,
    pub query: String,
}
fn name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '-')
}
pub fn context(value: &str, cursor: usize) -> Option<Context> {
    if !value.is_char_boundary(cursor) {
        return None;
    }
    let before = &value[..cursor];
    let start = before.rfind('@')?;
    if start > 0
        && !before[..start]
            .chars()
            .last()
            .is_some_and(|c| c.is_whitespace() || matches!(c, '(' | '[' | '{'))
    {
        return None;
    }
    let query = &before[start + 1..];
    if !query.chars().all(name_char) {
        return None;
    }
    let end = cursor
        + value[cursor..]
            .chars()
            .take_while(|&c| name_char(c))
            .map(char::len_utf8)
            .sum::<usize>();
    Some(Context {
        start,
        end,
        query: query.into(),
    })
}
pub fn insert(value: &mut String, cursor: &mut usize, ctx: &Context, user: &User) -> bool {
    if context(value, *cursor).as_ref() != Some(ctx)
        || user.account_id.is_empty()
        || user
            .account_id
            .chars()
            .any(|c| c.is_control() || matches!(c, '{' | '}' | '|' | '[' | ']'))
    {
        return false;
    }
    let name: String = user
        .name
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '{' | '}' | '|'))
        .collect();
    let token = format!(
        "@{{{}|{}}}{}",
        user.account_id,
        name,
        if ctx.end == value.len() { " " } else { "" }
    );
    value.replace_range(ctx.start..ctx.end, &token);
    *cursor = ctx.start + token.len();
    true
}
pub struct Picker {
    pub context: Context,
    pub version: u64,
    pub users: Vec<User>,
    pub index: usize,
    pub offset: usize,
    pub more: bool,
    pub loading: bool,
    pub needs_fetch: bool,
    pub error: Option<String>,
    ready: Instant,
}
impl Picker {
    pub fn new(context: Context, version: u64, now: Instant) -> Self {
        Self {
            context,
            version,
            users: Vec::new(),
            index: 0,
            offset: 0,
            more: false,
            loading: false,
            needs_fetch: true,
            error: None,
            ready: now + Duration::from_millis(250),
        }
    }
    pub fn due(&self, now: Instant) -> bool {
        self.needs_fetch && !self.loading && now >= self.ready
    }
    pub fn more(&mut self, now: Instant) {
        if self.more && !self.loading {
            self.needs_fetch = true;
            self.ready = now;
        }
    }
    pub fn finish(&mut self, result: Result<UserPage, String>) {
        self.loading = false;
        match result {
            Ok(page) => {
                let mut ids: HashSet<_> = self.users.iter().map(|u| u.account_id.clone()).collect();
                let had_users = !page.users.is_empty();
                let before = self.users.len();
                self.users.extend(
                    page.users
                        .into_iter()
                        .filter(|u| ids.insert(u.account_id.clone())),
                );
                self.more = page.more;
                if self.offset > 0 && had_users && before == self.users.len() {
                    self.more = false;
                    self.error = Some("Jira repeated this user page; narrow the search.".into());
                }
                self.offset = page.next_offset;
            }
            Err(error) => {
                self.more = false;
                self.error=Some(format!("User lookup unavailable: {error}. Browse users and groups permission may be required."));
            }
        }
    }
}
fn token_ranges(value: &str) -> Vec<std::ops::Range<usize>> {
    value
        .match_indices("@{")
        .filter_map(|(start, _)| {
            let rest = &value[start + 2..];
            let end = rest.find('}')?;
            let (id, name) = rest[..end].split_once('|')?;
            (!id.is_empty() && !name.is_empty()).then_some(start..start + end + 3)
        })
        .collect()
}
pub fn display(value: &str, cursor: usize) -> String {
    crate::jira::mention_preview(&crate::input::display(value, cursor))
}
pub fn edit(value: &mut String, cursor: &mut usize, key: crossterm::event::KeyEvent) {
    use crossterm::event::{KeyCode, KeyModifiers};
    *cursor = (*cursor).min(value.len());
    while !value.is_char_boundary(*cursor) {
        *cursor -= 1;
    }
    if !key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        for span in token_ranges(value) {
            if *cursor > span.start && *cursor < span.end {
                *cursor = span.end;
            }
            match key.code {
                KeyCode::Left if *cursor == span.end => {
                    *cursor = span.start;
                    return;
                }
                KeyCode::Right if *cursor == span.start => {
                    *cursor = span.end;
                    return;
                }
                KeyCode::Backspace if *cursor == span.end => {
                    value.replace_range(span.clone(), "");
                    *cursor = span.start;
                    return;
                }
                KeyCode::Delete if *cursor == span.start => {
                    value.replace_range(span, "");
                    return;
                }
                _ => {}
            }
        }
    }
    crate::input::edit(value, cursor, key);
    for span in token_ranges(value) {
        if *cursor > span.start && *cursor < span.end {
            *cursor = span.end;
            break;
        }
    }
}

pub fn demo_users(query: &str) -> UserPage {
    UserPage {
        users: [
            ("demo-alex", "Alex Chen"),
            ("demo-user", "Samir Khanal"),
            ("demo-maya", "Maya Patel"),
            ("demo-taylor", "Taylor Smith"),
        ]
        .into_iter()
        .filter(|(_, name)| name.to_lowercase().contains(&query.to_lowercase()))
        .map(|(id, name)| User {
            account_id: id.into(),
            name: name.into(),
        })
        .collect(),
        ..Default::default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inserted_tags_render_as_names_and_are_deleted_atomically() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut text = "Hi @{id:42|Samir Khanal}".to_string();
        let mut cursor = text.len();
        assert_eq!(display(&text, cursor), "Hi @Samir Khanal▌");
        edit(
            &mut text,
            &mut cursor,
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
        );
        assert_eq!(cursor, 3);
        edit(
            &mut text,
            &mut cursor,
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
        );
        assert_eq!(cursor, text.len());
        edit(
            &mut text,
            &mut cursor,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        assert_eq!(text, "Hi ");
    }
    #[test]
    fn at_triggers_names_not_emails_or_completed_mentions() {
        assert_eq!(context("Hi @Sa", 6).unwrap().query, "Sa");
        assert!(context("a@example.com", 13).is_none());
        assert!(context("@{id|Samir}", 11).is_none());
        assert!(context("@Samir ", 7).is_none());
        assert_eq!(context("界 @", 5).unwrap().start, 4);
    }
    #[test]
    fn selection_replaces_only_the_handle_and_preserves_unicode_and_suffix() {
        let mut value = "界 @Samir and team".to_string();
        let mut cursor = 7;
        let ctx = context(&value, cursor).unwrap();
        assert!(insert(
            &mut value,
            &mut cursor,
            &ctx,
            &User {
                account_id: "id:42".into(),
                name: "Samir | Khanal".into()
            }
        ));
        assert_eq!(value, "界 @{id:42|Samir  Khanal} and team");
        assert!(value.is_char_boundary(cursor));
        assert!(!insert(
            &mut value,
            &mut cursor,
            &ctx,
            &User {
                account_id: "x".into(),
                name: "X".into()
            }
        ));
    }
    #[test]
    fn paging_is_debounced_deduplicated_and_retains_existing_results_on_error() {
        let now = Instant::now();
        let mut picker = Picker::new(context("@", 1).unwrap(), 1, now);
        assert!(!picker.due(now));
        assert!(picker.due(now + Duration::from_millis(251)));
        let user = User {
            account_id: "1".into(),
            name: "A".into(),
        };
        picker.finish(Ok(UserPage {
            users: vec![user.clone()],
            more: true,
            next_offset: 50,
        }));
        picker.finish(Ok(UserPage {
            users: vec![user],
            more: true,
            next_offset: 100,
        }));
        assert_eq!(picker.users.len(), 1);
        assert!(!picker.more);
        picker.finish(Err("HTTP 403".into()));
        assert_eq!(picker.users.len(), 1);
        assert!(picker.error.unwrap().contains("permission"));
    }
}
