//! Small Unicode-safe caret editor shared by text and worklog forms.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn normalize(value: &str, cursor: &mut usize) {
    *cursor = (*cursor).min(value.len());
    while !value.is_char_boundary(*cursor) {
        *cursor -= 1;
    }
}

pub fn insert(value: &mut String, cursor: &mut usize, text: &str) {
    normalize(value, cursor);
    value.insert_str(*cursor, text);
    *cursor += text.len();
}

pub fn edit(value: &mut String, cursor: &mut usize, key: KeyEvent) {
    normalize(value, cursor);
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('u') => {
                value.clear();
                *cursor = 0;
            }
            KeyCode::Char('a') => *cursor = 0,
            KeyCode::Char('e') => *cursor = value.len(),
            _ => {}
        }
        return;
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        return;
    }
    match key.code {
        KeyCode::Char(c) => insert(value, cursor, &c.to_string()),
        KeyCode::Left => *cursor = value[..*cursor].char_indices().last().map_or(0, |(i, _)| i),
        KeyCode::Right => *cursor += value[*cursor..].chars().next().map_or(0, char::len_utf8),
        KeyCode::Home => *cursor = value[..*cursor].rfind('\n').map_or(0, |i| i + 1),
        KeyCode::End => *cursor += value[*cursor..].find('\n').unwrap_or(value.len() - *cursor),
        KeyCode::Backspace => {
            let previous = value[..*cursor].char_indices().last().map_or(0, |(i, _)| i);
            value.replace_range(previous..*cursor, "");
            *cursor = previous;
        }
        KeyCode::Delete => {
            let next = *cursor + value[*cursor..].chars().next().map_or(0, char::len_utf8);
            value.replace_range(*cursor..next, "");
        }
        KeyCode::Up | KeyCode::Down => {
            let start = value[..*cursor].rfind('\n').map_or(0, |i| i + 1);
            let column = value[start..*cursor].chars().count();
            let target = if key.code == KeyCode::Up {
                if start == 0 {
                    return;
                }
                let previous = value[..start - 1].rfind('\n').map_or(0, |i| i + 1);
                (previous, start - 1)
            } else {
                let Some(end) = value[*cursor..].find('\n').map(|i| *cursor + i) else {
                    return;
                };
                let next = end + 1;
                let end = value[next..].find('\n').map_or(value.len(), |i| next + i);
                (next, end)
            };
            *cursor = target.0
                + value[target.0..target.1]
                    .char_indices()
                    .nth(column)
                    .map_or(target.1 - target.0, |(i, _)| i);
        }
        _ => {}
    }
}

pub fn display(value: &str, mut cursor: usize) -> String {
    normalize(value, &mut cursor);
    format!("{}▌{}", &value[..cursor], &value[cursor..])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    #[test]
    fn unicode_can_be_inserted_deleted_and_navigated_in_the_middle() {
        let mut value = "A界B".to_string();
        let mut cursor = value.len();
        edit(&mut value, &mut cursor, key(KeyCode::Left));
        edit(&mut value, &mut cursor, key(KeyCode::Left));
        insert(&mut value, &mut cursor, "🙂");
        assert_eq!(value, "A🙂界B");
        edit(&mut value, &mut cursor, key(KeyCode::Delete));
        assert_eq!(value, "A🙂B");
        edit(&mut value, &mut cursor, key(KeyCode::Backspace));
        assert_eq!(value, "AB");
        assert_eq!(cursor, 1);
    }
    #[test]
    fn multiline_navigation_preserves_character_column() {
        let mut value = "ab界\nxyz\nlonger".to_string();
        let mut cursor = 2;
        edit(&mut value, &mut cursor, key(KeyCode::Down));
        assert_eq!(cursor, 8);
        edit(&mut value, &mut cursor, key(KeyCode::Up));
        assert_eq!(cursor, 2);
        edit(&mut value, &mut cursor, key(KeyCode::End));
        assert_eq!(cursor, 5);
    }
    #[test]
    fn clear_and_boundary_navigation_are_safe() {
        let mut value = "界".to_string();
        let mut cursor = 1;
        assert_eq!(display(&value, cursor), "▌界");
        edit(&mut value, &mut cursor, key(KeyCode::Left));
        assert_eq!(cursor, 0);
        edit(
            &mut value,
            &mut cursor,
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        );
        assert!(value.is_empty());
        assert_eq!(cursor, 0);
        edit(&mut value, &mut cursor, key(KeyCode::Delete));
        assert!(value.is_empty());
    }
}
