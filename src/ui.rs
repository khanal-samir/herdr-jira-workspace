use crate::{
    app::{App, Dialog, Hits, TextKind, View, WorklogForm},
    calendar::{duration_text, parse_started, Calendar},
};
use chrono::{Datelike, Local};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

const BG: Color = Color::Rgb(19, 27, 40);
const PANEL: Color = Color::Rgb(27, 38, 54);
const INK: Color = Color::Rgb(220, 231, 244);
const MUTED: Color = Color::Rgb(143, 163, 188);
const ACCENT: Color = Color::Rgb(92, 171, 255);
const GOOD: Color = Color::Rgb(100, 200, 165);
const WARN: Color = Color::Rgb(241, 189, 97);
const BAD: Color = Color::Rgb(247, 129, 139);

fn base() -> Style {
    Style::default().fg(INK).bg(BG)
}
fn selected() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(ACCENT)
        .add_modifier(Modifier::BOLD)
}
fn block(title: impl Into<String>, focus: bool) -> Block<'static> {
    Block::default()
        .title(title.into())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focus { ACCENT } else { MUTED }))
        .style(base())
}
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

pub fn draw(frame: &mut Frame, app: &App) -> Hits {
    let area = frame.area();
    let mut hits = Hits::default();
    frame.render_widget(Block::default().style(base()), area);
    if area.width < 40 || area.height < 14 {
        frame.render_widget(Paragraph::new("Jira Workspace\nResize the pane to at least 40 × 14 cells.\nUse Herdr's pane zoom for the full board.").wrap(Wrap{trim:false}).style(base()),area);
        return hits;
    }
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(2),
            Constraint::Length(2),
        ])
        .split(area);
    let heading = format!(
        " JIRA WORKSPACE{}  /  {} ",
        if app.demo { " · DEMO" } else { "" },
        if app.board.name.is_empty() {
            "Choose a board"
        } else {
            &app.board.name
        }
    );
    frame.render_widget(
        Paragraph::new(heading)
            .block(block(" Herdr · native workspace ", false))
            .style(
                Style::default()
                    .fg(ACCENT)
                    .bg(PANEL)
                    .add_modifier(Modifier::BOLD),
            ),
        regions[0],
    );
    let tab = match app.view {
        View::Board => "BOARD",
        View::Details => "TICKET",
        View::Calendar => "WORKLOG CALENDAR",
        View::Image => "IMAGE",
    };
    frame.render_widget(
        Paragraph::new(format!(
            " {tab}   {} issues   {}",
            app.board.issues.len(),
            if app.search.is_empty() {
                String::new()
            } else {
                format!("filter: {}", app.search)
            }
        ))
        .style(Style::default().fg(MUTED)),
        regions[1],
    );
    match app.view{
        View::Board=>draw_board(frame,regions[2],app,&mut hits),
        View::Details=>draw_details(frame,regions[2],app,&mut hits),
        View::Calendar=>draw_calendar_view(frame,regions[2],app,&mut hits),
        View::Image=>frame.render_widget(Paragraph::new(format!("{}\n\nImage is rendered by Herdr's experimental pane graphics.\nEsc returns to the attachment list.",app.image_name)).block(block("Image preview",true)),regions[2]),
    }
    let keys=match app.view{
        View::Board=>"↑↓ card  ←→ column  Enter ticket  Shift+←→ move  Shift+↑↓ rank\nw log work  g calendar  c comments  l labels  e edit  b boards  / filter  ? help  q quit",
        View::Details=>"Tab panel  ↑↓ entry  PgUp/PgDn scroll  n new  e edit  p quote reply  D delete\nw log work  g calendar  l labels  i attachments  o browser  Esc board  ? help",
        View::Calendar=>"←→ day  ↑↓ week  PgUp/PgDn month  j/k entry  a mine/team  v week/month\nw log work  e edit  D delete  Enter ticket  t today  r reload  Esc board",
        View::Image=>"Esc return to attachments",
    };
    frame.render_widget(
        Paragraph::new(keys).style(Style::default().fg(MUTED)),
        regions[3],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{}{}",
            if app.busy { "● " } else { "" },
            app.notice
        ))
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(if app.error {
            BAD
        } else if app.busy {
            WARN
        } else {
            GOOD
        })),
        regions[4],
    );
    if let Some(dialog) = &app.dialog {
        draw_dialog(frame, area, app, dialog, &mut hits);
    }
    hits
}

fn draw_board(frame: &mut Frame, area: Rect, app: &App, hits: &mut Hits) {
    if app.board.columns.is_empty() {
        frame.render_widget(Paragraph::new(if app.busy{"Loading your Jira workspace…"}else{"Press b to choose a Jira board.\nCheck configuration if connection failed.\nRun --demo to explore without credentials."}).block(block("Board",true)).wrap(Wrap{trim:false}),area);
        return;
    }
    let groups = app.groups();
    let visible = (area.width / 25).max(1) as usize;
    let count = visible.min(groups.len());
    let start = if groups.len() <= count {
        0
    } else {
        app.column
            .saturating_sub(count - 1)
            .min(groups.len() - count)
    };
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, count as u32); count])
        .split(area);
    for (offset, rect) in cols.iter().enumerate() {
        let column = start + offset;
        let group = &groups[column];
        let config = &app.board.columns[column];
        let limit = config
            .max
            .map(|m| format!(" / WIP {m}"))
            .unwrap_or_default();
        let title = format!(" {} ({}){} ", config.name, group.len(), limit);
        let mut border = block(title, column == app.column);
        if config.max.is_some_and(|max| group.len() as u64 > max) {
            border = border.border_style(Style::default().fg(WARN));
        }
        let inner = border.inner(*rect);
        frame.render_widget(border, *rect);
        hits.columns.push((*rect, column));
        if group.is_empty() {
            frame.render_widget(
                Paragraph::new("No matching tickets")
                    .style(Style::default().fg(MUTED))
                    .wrap(Wrap { trim: false }),
                inner,
            );
            continue;
        }
        let capacity = (inner.height / 6).max(1) as usize;
        let first = if column == app.column {
            app.row.saturating_sub(capacity - 1)
        } else {
            0
        };
        for (position, index) in group.iter().enumerate().skip(first).take(capacity) {
            let y = inner.y + (position - first) as u16 * 6;
            if y >= inner.bottom() {
                break;
            }
            let rect = Rect::new(inner.x, y, inner.width, 6.min(inner.bottom() - y));
            let issue = &app.board.issues[*index];
            let focus = column == app.column && position == app.row;
            let color = if focus { ACCENT } else { MUTED };
            let lines = vec![
                Line::from(vec![
                    Span::styled(
                        format!("{}  ", issue.key),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        issue.priority.clone(),
                        Style::default().fg(if issue.priority.to_lowercase().contains("high") {
                            WARN
                        } else {
                            MUTED
                        }),
                    ),
                ]),
                Line::from(issue.summary.clone()),
                Line::from(Span::styled(
                    format!("{} · {}", issue.assignee, issue.status),
                    Style::default().fg(MUTED),
                )),
                Line::from(Span::styled(
                    format!(
                        "{}  [{}]",
                        duration_text(issue.time_spent),
                        issue.labels.join(", ")
                    ),
                    Style::default().fg(GOOD),
                )),
            ];
            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .borders(Borders::BOTTOM)
                            .border_style(Style::default().fg(color)),
                    )
                    .style(Style::default().bg(if focus { PANEL } else { BG }))
                    .wrap(Wrap { trim: false }),
                rect,
            );
            hits.cards.push((rect, column, position));
        }
    }
}

fn draw_details(frame: &mut Frame, area: Rect, app: &App, hits: &mut Hits) {
    let Some(details) = &app.details else {
        return;
    };
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(1),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(format!(
            "{} · {}\n{}  /  {}  /  {}",
            details.issue.key,
            details.issue.summary,
            details.issue.status,
            details.issue.assignee,
            details.issue.priority
        ))
        .style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
        .wrap(Wrap { trim: false }),
        parts[0],
    );
    let tabs = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25); 4])
        .split(parts[1]);
    for (index, label) in ["Details", "Comments", "Worklogs", "Attachments"]
        .iter()
        .enumerate()
    {
        frame.render_widget(
            Paragraph::new(*label).style(if app.detail_tab == index {
                selected()
            } else {
                base()
            }),
            tabs[index],
        );
        hits.tabs.push((tabs[index], index));
    }
    if app.detail_tab == 0 {
        frame.render_widget(
            Paragraph::new(format!(
                "Type: {}\nLabels: {}\nTime logged: {}\n\n{}",
                details.issue.issue_type,
                details.issue.labels.join(", "),
                duration_text(details.issue.time_spent),
                details.issue.description
            ))
            .block(block("Description · e edit fields", false))
            .wrap(Wrap { trim: false })
            .scroll((app.scroll, 0)),
            parts[2],
        );
        return;
    }
    let rows = if parts[2].width >= 70 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(36), Constraint::Percentage(64)])
            .split(parts[2])
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(parts[2])
    };
    let labels: Vec<String> = match app.detail_tab {
        1 => details
            .comments
            .iter()
            .map(|c| {
                format!(
                    "{} · {}",
                    c.author,
                    c.created.chars().take(10).collect::<String>()
                )
            })
            .collect(),
        2 => details
            .worklogs
            .iter()
            .map(|l| {
                format!(
                    "{} · {}",
                    duration_text(l.seconds),
                    parse_started(&l.started)
                        .map(|t| t.format("%m-%d %H:%M").to_string())
                        .unwrap_or_else(|| l.started.clone())
                )
            })
            .collect(),
        _ => details
            .attachments
            .iter()
            .map(|a| a.filename.clone())
            .collect(),
    };
    list(frame, rows[0], &labels, app.item, "Entries");
    let body=match app.detail_tab{
        1=>details.comments.get(app.item).map(|c|format!("{}\n{}\n\n{}\n\np quote reply · e replaces formatting",c.author,c.created,c.text)),
        2=>details.worklogs.get(app.item).map(|l|format!("{} · {}\n{}\n\n{}\n\ne edit · D delete",l.author,duration_text(l.seconds),l.started,l.text)),
        _=>details.attachments.get(app.item).map(|a|format!("{}\n{} · {} KiB\n\nEnter downloads securely and previews the file.\nPNG images use Herdr pane graphics when enabled; otherwise your system viewer opens.\n\no opens the ticket in Jira.",a.filename,a.mime,a.size/1024)),
    }.unwrap_or_else(||"No entries yet.\n\nn adds a comment/worklog; attachments are managed in Jira.".into());
    frame.render_widget(
        Paragraph::new(body)
            .block(block("Selected entry", false))
            .wrap(Wrap { trim: false })
            .scroll((app.scroll, 0)),
        rows[1],
    );
}

fn draw_calendar_view(frame: &mut Frame, area: Rect, app: &App, hits: &mut Hits) {
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(5)])
        .split(area);
    let total: i64 = app
        .calendar_logs
        .iter()
        .filter(|l| {
            (app.calendar_all || l.account_id == app.calendar_owner)
                && parse_started(&l.started).is_some_and(|t| {
                    t.year() == app.calendar.selected.year()
                        && t.month() == app.calendar.selected.month()
                })
        })
        .map(|l| l.seconds)
        .sum();
    frame.render_widget(
        Paragraph::new(format!(
            "{} · {} total: {} · current board only{}",
            if app.calendar_all {
                "Team"
            } else {
                "My worklogs"
            },
            app.calendar.selected.format("%B"),
            duration_text(total),
            if app.calendar_warnings.is_empty() {
                ""
            } else {
                " · INCOMPLETE"
            }
        ))
        .style(Style::default().fg(if app.calendar_warnings.is_empty() {
            GOOD
        } else {
            WARN
        }))
        .wrap(Wrap { trim: false }),
        parts[0],
    );
    let regions = Layout::default()
        .direction(if area.width >= 75 {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(parts[1]);
    calendar_grid(
        frame,
        regions[0],
        &app.calendar,
        true,
        Some(app),
        app.calendar_week,
        hits,
    );
    let logs = app.day_logs();
    let labels: Vec<_> = logs
        .iter()
        .map(|l| {
            format!(
                "{}  {}  {}\n{}",
                l.key,
                duration_text(l.seconds),
                l.author,
                l.text
            )
        })
        .collect();
    list(
        frame,
        regions[1],
        &labels,
        app.calendar_item,
        &format!(
            "{} · {}",
            app.calendar.selected,
            duration_text(app.total_on(app.calendar.selected))
        ),
    );
}

fn calendar_grid(
    frame: &mut Frame,
    area: Rect,
    calendar: &Calendar,
    focus: bool,
    app: Option<&App>,
    week: bool,
    hits: &mut Hits,
) {
    let border = block(
        format!(" {} · local timezone ", calendar.selected.format("%B %Y")),
        focus,
    );
    let inner = border.inner(area);
    frame.render_widget(border, area);
    if inner.height < 2 || inner.width < 7 {
        return;
    }
    let names = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
    let header = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 7); 7])
        .split(Rect::new(inner.x, inner.y, inner.width, 1));
    for (index, name) in names.iter().enumerate() {
        frame.render_widget(
            Paragraph::new(*name).style(Style::default().fg(MUTED)),
            header[index],
        );
    }
    let dates = if week {
        let start = calendar.selected
            - chrono::Duration::days(calendar.selected.weekday().num_days_from_monday() as i64);
        (0..7)
            .map(|day| Some(start + chrono::Duration::days(day)))
            .collect()
    } else {
        calendar.cells()
    };
    let row_count = if week { 1 } else { 6 };
    let grid = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Ratio(1, row_count); row_count as usize])
        .split(grid);
    for (row, chunk) in dates.chunks(7).enumerate() {
        let cells = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Ratio(1, 7); 7])
            .split(rows[row]);
        for (column, date) in chunk.iter().enumerate() {
            if let Some(date) = date {
                let rect = cells[column];
                let total = app.map_or(0, |a| a.total_on(*date));
                let label = if app.is_some() && total > 0 {
                    format!("{:02}\n{}", date.day(), duration_text(total))
                } else {
                    format!("{:02}", date.day())
                };
                let style = if *date == calendar.selected {
                    selected()
                } else if *date == Local::now().date_naive() {
                    Style::default().fg(GOOD).bg(PANEL)
                } else {
                    base()
                };
                frame.render_widget(Paragraph::new(label).style(style), rect);
                hits.days.push((rect, *date));
            }
        }
    }
}

fn list(frame: &mut Frame, area: Rect, labels: &[String], index: usize, title: &str) {
    if labels.is_empty() {
        frame.render_widget(
            Paragraph::new("No entries")
                .block(block(title, false))
                .style(Style::default().fg(MUTED)),
            area,
        );
        return;
    }
    let items: Vec<_> = labels.iter().map(|s| ListItem::new(s.clone())).collect();
    let mut state = ListState::default().with_selected(Some(index));
    frame.render_stateful_widget(
        List::new(items)
            .block(block(title, true))
            .highlight_style(selected())
            .highlight_symbol("› "),
        area,
        &mut state,
    );
}

fn draw_dialog(frame: &mut Frame, area: Rect, app: &App, dialog: &Dialog, hits: &mut Hits) {
    hits.cards.clear();
    hits.columns.clear();
    hits.days.clear();
    hits.tabs.clear();
    if let Dialog::Worklog(form) = dialog {
        draw_worklog(frame, area, app, form, hits);
        return;
    }
    let rect = centered(area, 80, area.height.saturating_sub(4).min(26));
    frame.render_widget(Clear, rect);
    let outer = block(
        match dialog {
            Dialog::Text(f) => f.title.clone(),
            Dialog::Boards { .. } => "Choose Jira board".into(),
            Dialog::Transitions { .. } => "Move ticket · confirm workflow transition".into(),
            Dialog::Choices { field, .. } => format!("Choose {field}"),
            Dialog::EditMenu { .. } => "Edit ticket".into(),
            Dialog::Confirm { .. } => "Confirm deletion".into(),
            _ => "Keyboard guide".into(),
        },
        true,
    );
    let inner = outer.inner(rect);
    frame.render_widget(outer, rect);
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(inner);
    match dialog {
        Dialog::Text(form) => {
            let lines = form.value.lines().count().max(1);
            let scroll = lines.saturating_sub(regions[0].height as usize) as u16;
            frame.render_widget(
                Paragraph::new(crate::input::display(&form.value, form.cursor))
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0)),
                regions[0],
            );
            let footer = if matches!(form.kind, TextKind::Comment(_)) {
                "Enter newline · Ctrl+S / Ctrl+Enter saves · Esc cancels\nMentions: @{accountId|Display name}. Editing replaces rich formatting."
            } else {
                "Enter saves · Esc cancels · Backspace edits"
            };
            frame.render_widget(
                Paragraph::new(footer)
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(MUTED)),
                regions[1],
            );
        }
        Dialog::Confirm { kind, id, key } => {
            frame.render_widget(Paragraph::new(format!("Delete {kind} {id} on {key}?\n\nThis cannot be undone. Jira permissions still apply.\nEnter / y confirms · Esc cancels")).wrap(Wrap{trim:false}),regions[0]);
        }
        Dialog::Help => {
            let mut help=String::from("BOARD\nArrows navigate · Enter opens · Shift+Left/Right moves\nShift+Up/Down ranks · m transition picker · drag cards to move\nDouble-click opens · / filters loaded tickets · b board picker\n\nWORKLOGS\nw opens date calendar + time + duration + description\nTab changes field · arrows select date · PgUp/PgDn month\nCtrl+S / Ctrl+Enter saves · Esc cancels · mouse selects dates/fields\ng worklog calendar · a mine/team · v week/month · j/k entry\n\nTICKETS\nTab changes panel · n adds · e edits · p quoted reply + mention\nD confirms deletion · l labels · i attachments · o Jira browser\nPgUp/PgDn scroll text · r refresh · Esc returns · q quits\n\nOnly supported Jira operations are native; complex workflow forms\nand rich-text/image uploads remain available through o browser.");
            if !app.calendar_warnings.is_empty() {
                help.push_str("\n\nIncomplete calendar: ");
                help.push_str(&app.calendar_warnings.join("; "));
            }
            frame.render_widget(Paragraph::new(help).wrap(Wrap { trim: false }), regions[0]);
            frame.render_widget(
                Paragraph::new("Esc closes this guide").style(Style::default().fg(MUTED)),
                regions[1],
            );
        }
        _ => {
            let (labels, index) = match dialog {
                Dialog::Boards { boards, index } => (
                    boards
                        .iter()
                        .map(|b| format!("{} · {}", b.id, b.name))
                        .collect::<Vec<_>>(),
                    *index,
                ),
                Dialog::Transitions { list, index, .. } => (
                    list.iter()
                        .map(|t| {
                            format!(
                                "{}{}",
                                t.name,
                                if t.required_fields.is_empty() {
                                    String::new()
                                } else {
                                    format!(
                                        " · requires {} (browser)",
                                        t.required_fields.join(", ")
                                    )
                                }
                            )
                        })
                        .collect(),
                    *index,
                ),
                Dialog::Choices { items, index, .. } => (
                    items.iter().map(|(_, label)| label.clone()).collect(),
                    *index,
                ),
                Dialog::EditMenu { index } => (
                    vec![
                        "Summary".into(),
                        "Labels".into(),
                        "Assignee".into(),
                        "Priority".into(),
                    ],
                    *index,
                ),
                _ => unreachable!(),
            };
            let capacity = regions[0].height as usize;
            let first = index.saturating_sub(capacity.saturating_sub(1));
            for (position, label) in labels.iter().enumerate().skip(first).take(capacity) {
                let row = Rect::new(
                    regions[0].x,
                    regions[0].y + (position - first) as u16,
                    regions[0].width,
                    1,
                );
                frame.render_widget(
                    Paragraph::new(label.clone()).style(if position == index {
                        selected()
                    } else {
                        base()
                    }),
                    row,
                );
                hits.picker.push((row, position));
            }
            if labels.is_empty() {
                frame.render_widget(
                    Paragraph::new("No available choices").style(Style::default().fg(WARN)),
                    regions[0],
                );
            }
            frame.render_widget(
                Paragraph::new("↑↓ selects · Enter confirms · Esc cancels")
                    .style(Style::default().fg(MUTED)),
                regions[1],
            );
        }
    }
}

fn draw_worklog(frame: &mut Frame, area: Rect, app: &App, form: &WorklogForm, hits: &mut Hits) {
    let rect = centered(area, 94, 28);
    frame.render_widget(Clear, rect);
    let outer = block(
        format!(
            " {} work · {} ",
            if form.id.is_some() { "Edit" } else { "Log" },
            form.key
        ),
        true,
    );
    let inner = outer.inner(rect);
    frame.render_widget(outer, rect);
    if inner.width < 60 || inner.height < 22 {
        frame.render_widget(Paragraph::new("Log work needs a pane of at least 62 × 24 cells.\n\nZoom this pane using Herdr, or resize the terminal.\nYour input is preserved. Esc cancels.").wrap(Wrap{trim:false}),inner);
        return;
    }
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(18),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(sections[0]);
    let date_parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(11),
            Constraint::Min(1),
        ])
        .split(columns[0]);
    frame.render_widget(
        Paragraph::new(format!(
            "Date: {}\n←→ day · ↑↓ week · PgUp/PgDn month",
            form.calendar.selected
        ))
        .style(Style::default().fg(if form.field == 0 { ACCENT } else { MUTED })),
        date_parts[0],
    );
    calendar_grid(
        frame,
        date_parts[1],
        &form.calendar,
        form.field == 0,
        None,
        false,
        hits,
    );
    frame.render_widget(Paragraph::new("Calendar dates are clickable.\n\nLocal timezone; Jira receives an explicit UTC offset.\n\nTab moves to start time.").style(Style::default().fg(MUTED)).wrap(Wrap{trim:false}),date_parts[2]);
    let fields = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(columns[1]);
    for (index, title, value) in [
        (1, "Start time · HH:MM", form.time.as_str()),
        (2, "Time spent · e.g. 1h 30m", form.duration.as_str()),
        (3, "Work description · Enter newline", form.comment.as_str()),
        (4, "Remaining estimate · ←→ cycles", form.estimate.as_str()),
        (
            5,
            "New remaining estimate · if mode=new",
            form.remaining.as_str(),
        ),
    ] {
        let rect = fields[index - 1];
        let text = if form.field == index {
            let cursor = match index {
                1 => form.cursors[0],
                2 => form.cursors[1],
                3 => form.cursors[2],
                5 => form.cursors[3],
                _ => value.len(),
            };
            crate::input::display(value, cursor)
        } else {
            value.into()
        };
        frame.render_widget(
            Paragraph::new(text)
                .block(block(title, form.field == index))
                .wrap(Wrap { trim: false }),
            rect,
        );
        hits.fields.push((rect, index));
    }
    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(sections[1]);
    for (index, label, rect) in [(6, "Save worklog", buttons[0]), (7, "Cancel", buttons[1])] {
        frame.render_widget(
            Paragraph::new(label)
                .block(block(label, form.field == index))
                .style(if form.field == index {
                    selected()
                } else {
                    base()
                }),
            rect,
        );
        hits.fields.push((rect, index));
    }
    frame.render_widget(
        Paragraph::new(if app.error {
            format!("{} · input preserved", app.notice)
        } else if app.busy {
            "Saving… do not close this pane".into()
        } else {
            "Tab field · Ctrl+S / Ctrl+Enter saves · Esc cancels · t today".into()
        })
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(if app.error { BAD } else { MUTED })),
        sections[2],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::{backend::TestBackend, Terminal};
    fn render(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| {
                draw(f, app);
            })
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .chunks(width as usize)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn board_and_calendar_dialog_render_at_supported_sizes() {
        let (tx, _) = std::sync::mpsc::channel();
        let mut app = App::new(tx, true);
        let board = render(&app, 120, 40);
        assert!(board.contains("JIRA WORKSPACE"));
        assert!(board.contains("DEMO-1"));
        assert!(board.contains("In review"));
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('w'),
            crossterm::event::KeyModifiers::NONE,
        ));
        let dialog = render(&app, 120, 40);
        assert!(dialog.contains("Save worklog"));
        assert!(dialog.contains("Time spent"));
        assert!(dialog.contains("Mo"));
        assert!(dialog.contains("Remaining estimate"));
    }
    #[test]
    fn ticket_panels_and_both_calendar_views_render() {
        let (tx, _) = std::sync::mpsc::channel();
        let mut app = App::new(tx, true);
        let key =
            |code| crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE);
        app.on_key(key(crossterm::event::KeyCode::Enter));
        for tab in 0..4 {
            app.detail_tab = tab;
            let text = render(&app, 120, 40);
            assert!(text.contains("DEMO-1"));
            assert!(text.contains("Attachments"));
        }
        app.on_key(key(crossterm::event::KeyCode::Char('g')));
        assert!(render(&app, 120, 40).contains("My worklogs"));
        app.calendar_week = true;
        assert!(render(&app, 120, 40).contains("DEMO-1"));
        for (width, height) in [(40, 14), (62, 24), (80, 24), (120, 40)] {
            render(&app, width, height);
        }
    }

    #[test]
    fn worklog_date_hitboxes_select_days_with_the_mouse() {
        let (tx, _) = std::sync::mpsc::channel();
        let mut app = App::new(tx, true);
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('w'),
            crossterm::event::KeyModifiers::NONE,
        ));
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|f| app.hits = draw(f, &app)).unwrap();
        let (rect, date) = app.hits.days[0];
        app.on_mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        if let Some(Dialog::Worklog(form)) = &app.dialog {
            assert_eq!(form.calendar.selected, date);
        } else {
            panic!("Form unexpectedly closed");
        }
    }

    #[test]
    fn resize_and_empty_boards_never_panic() {
        let (tx, _) = std::sync::mpsc::channel();
        let mut app = App::new(tx, true);
        for (width, height) in [
            (1, 1),
            (20, 8),
            (40, 14),
            (62, 24),
            (80, 24),
            (120, 40),
            (240, 60),
        ] {
            render(&app, width, height);
        }
        app.board = Default::default();
        assert!(render(&app, 80, 24).contains("choose a Jira board"));
        app.dialog = Some(Dialog::Worklog(WorklogForm::new("X-1".into(), None, None)));
        assert!(render(&app, 40, 14).contains("Log work needs"));
    }
}
