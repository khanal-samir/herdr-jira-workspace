use crate::{
    calendar::{duration_text, parse_started, Calendar},
    config::Config,
    integrations,
    jira::*,
};
use chrono::{Local, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use serde_json::{json, Value};
use std::{
    sync::mpsc::Sender,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Board,
    Details,
    Calendar,
    Image,
}

#[derive(Debug, Clone)]
pub enum TextKind {
    Search,
    Labels(Vec<String>),
    Comment(Option<String>),
    Summary,
    AssigneeSearch,
}

#[derive(Debug, Clone)]
pub struct TextForm {
    pub title: String,
    pub key: String,
    pub value: String,
    pub cursor: usize,
    pub kind: TextKind,
}

#[derive(Debug, Clone)]
pub struct WorklogForm {
    pub key: String,
    pub id: Option<String>,
    pub calendar: Calendar,
    pub time: String,
    pub duration: String,
    pub comment: String,
    pub estimate: String,
    pub remaining: String,
    pub field: usize,
    pub cursors: [usize; 4],
}

impl WorklogForm {
    pub fn new(key: String, date: Option<NaiveDate>, log: Option<&Worklog>) -> Self {
        let started = log.and_then(|l| parse_started(&l.started));
        let duration = log.map(|l| duration_text(l.seconds)).unwrap_or_default();
        let comment = log.map(|l| l.text.clone()).unwrap_or_default();
        let cursors = [5, duration.len(), comment.len(), 0];
        Self {
            key,
            id: log.map(|l| l.id.clone()),
            calendar: Calendar {
                selected: started
                    .map(|t| t.date_naive())
                    .or(date)
                    .unwrap_or_else(|| Local::now().date_naive()),
            },
            time: started
                .unwrap_or_else(Local::now)
                .format("%H:%M")
                .to_string(),
            duration,
            comment,
            estimate: "leave".into(),
            remaining: String::new(),
            field: 0,
            cursors,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Dialog {
    Boards {
        boards: Vec<Board>,
        index: usize,
    },
    Transitions {
        key: String,
        list: Vec<Transition>,
        index: usize,
    },
    Choices {
        key: String,
        field: String,
        items: Vec<(String, String)>,
        index: usize,
    },
    EditMenu {
        index: usize,
    },
    Text(TextForm),
    Worklog(WorklogForm),
    Confirm {
        key: String,
        kind: String,
        id: String,
    },
    Help,
}

pub struct Response {
    pub generation: u64,
    pub result: Result<Data, String>,
}

pub enum Data {
    Connected(JiraClient, u64),
    Boards(Vec<Board>),
    Board(Board),
    Details(Box<Details>),
    Transitions(String, Vec<Transition>),
    Choices(String, String, Vec<(String, String)>),
    Calendar(Vec<Worklog>, String, Vec<String>),
    Saved(String),
    Attachment(Attachment, Vec<u8>),
}

#[derive(Default)]
pub struct Hits {
    pub cards: Vec<(Rect, usize, usize)>,
    pub columns: Vec<(Rect, usize)>,
    pub days: Vec<(Rect, NaiveDate)>,
    pub fields: Vec<(Rect, usize)>,
    pub tabs: Vec<(Rect, usize)>,
    pub picker: Vec<(Rect, usize)>,
}

pub struct App {
    pub board: Board,
    pub view: View,
    pub column: usize,
    pub row: usize,
    pub search: String,
    pub details: Option<Details>,
    pub detail_tab: usize,
    pub item: usize,
    pub scroll: u16,
    pub calendar: Calendar,
    pub calendar_logs: Vec<Worklog>,
    pub calendar_owner: String,
    pub calendar_all: bool,
    pub calendar_week: bool,
    pub calendar_item: usize,
    pub calendar_warnings: Vec<String>,
    pub dialog: Option<Dialog>,
    pub busy: bool,
    pending_write: bool,
    generation: u64,
    pub notice: String,
    pub error: bool,
    pub should_quit: bool,
    pub demo: bool,
    pub hits: Hits,
    pub image_name: String,
    pub dimensions: (u16, u16),
    client: Option<JiraClient>,
    tx: Sender<Response>,
    drag: Option<(String, usize)>,
    last_click: Option<(String, Instant)>,
}

impl App {
    pub fn new(tx: Sender<Response>, demo: bool) -> Self {
        let mut app = Self {
            board: Board::default(),
            view: View::Board,
            column: 0,
            row: 0,
            search: String::new(),
            details: None,
            detail_tab: 0,
            item: 0,
            scroll: 0,
            calendar: Calendar::default(),
            calendar_logs: Vec::new(),
            calendar_owner: String::new(),
            calendar_all: false,
            calendar_week: false,
            calendar_item: 0,
            calendar_warnings: Vec::new(),
            dialog: None,
            busy: false,
            pending_write: false,
            generation: 0,
            notice: String::new(),
            error: false,
            should_quit: false,
            demo,
            hits: Hits::default(),
            image_name: String::new(),
            dimensions: (80, 24),
            client: None,
            tx: tx.clone(),
            drag: None,
            last_click: None,
        };
        if demo {
            app.board = demo_board();
            app.notice = "DEMO · local-only interactions · no Jira requests".into();
        } else {
            app.busy = true;
            app.notice = "Connecting to Jira…".into();
            std::thread::spawn(move || {
                let result = Config::load().and_then(|cfg| {
                    JiraClient::new(&cfg).map(|client| Data::Connected(client, cfg.board_id))
                });
                let _ = tx.send(Response {
                    generation: 0,
                    result,
                });
            });
        }
        app
    }

    fn start(
        &mut self,
        label: &str,
        job: impl FnOnce(JiraClient) -> Result<Data, String> + Send + 'static,
    ) {
        let Some(client) = self.client.clone() else {
            self.fail("Not connected. Check config and restart, or use --demo.");
            return;
        };
        self.busy = true;
        self.pending_write = false;
        self.generation += 1;
        let generation = self.generation;
        self.error = false;
        self.notice = label.into();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Response {
                generation,
                result: job(client),
            });
        });
    }

    fn write(
        &mut self,
        label: &str,
        job: impl FnOnce(JiraClient) -> Result<Data, String> + Send + 'static,
    ) {
        self.start(label, job);
        self.pending_write = self.busy;
    }

    pub fn on_message(&mut self, response: Response) {
        if response.generation == self.generation {
            self.on_response(response.result);
        }
    }

    pub fn on_response(&mut self, result: Result<Data, String>) {
        self.busy = false;
        self.pending_write = false;
        match result {
            Err(error) => self.fail(&error),
            Ok(Data::Connected(client, board_id)) => {
                self.client = Some(client);
                if board_id == 0 {
                    self.choose_board();
                } else {
                    self.load_board(board_id);
                }
            }
            Ok(Data::Boards(boards)) => {
                self.notice = format!("{} available boards", boards.len());
                self.error = false;
                self.dialog = Some(Dialog::Boards { boards, index: 0 });
            }
            Ok(Data::Board(mut board)) => {
                let key = self.selected_issue().map(|i| i.key.clone());
                let mapped: Vec<String> = board
                    .columns
                    .iter()
                    .flat_map(|c| c.statuses.clone())
                    .collect();
                let unmapped: Vec<String> = board
                    .issues
                    .iter()
                    .filter(|i| !mapped.contains(&i.status_id))
                    .map(|i| i.status_id.clone())
                    .collect();
                if !unmapped.is_empty() {
                    board.columns.push(Column {
                        name: "Unmapped".into(),
                        statuses: unmapped,
                        max: None,
                    });
                }
                self.board = board;
                self.dialog = None;
                self.error = false;
                self.notice = format!(
                    "{} issues loaded · / filter · ? help",
                    self.board.issues.len()
                );
                self.clamp_selection();
                if let Some(key) = key {
                    self.select_key(&key);
                }
            }
            Ok(Data::Details(details)) => {
                let details = *details;
                self.error = !details.warnings.is_empty();
                self.notice = if self.error {
                    details.warnings.join("; ")
                } else {
                    "Tab switches panels · n new · e edit · D delete · Esc board".into()
                };
                if let Some(issue) = self
                    .board
                    .issues
                    .iter_mut()
                    .find(|i| i.key == details.issue.key)
                {
                    *issue = details.issue.clone();
                }
                self.details = Some(details);
                self.view = View::Details;
                self.clamp_item();
            }
            Ok(Data::Transitions(key, list)) => {
                if list.is_empty() {
                    self.fail("No available transition to this column. The card has not moved.");
                } else {
                    self.dialog = Some(Dialog::Transitions {
                        key,
                        list,
                        index: 0,
                    });
                    self.notice = "Select a transition · Enter confirms · Esc cancels".into();
                }
            }
            Ok(Data::Choices(key, field, items)) => {
                self.dialog = Some(Dialog::Choices {
                    key,
                    field,
                    items,
                    index: 0,
                });
                self.notice = "Enter selects · Esc cancels".into();
            }
            Ok(Data::Calendar(logs, owner, warnings)) => {
                self.calendar_logs = logs;
                self.calendar_owner = owner;
                self.calendar_warnings = warnings;
                self.view = View::Calendar;
                self.dialog = None;
                self.calendar_item = 0;
                self.error = !self.calendar_warnings.is_empty();
                self.notice = if self.error {
                    format!(
                        "INCOMPLETE TOTALS: {} tickets could not be read. ? for details",
                        self.calendar_warnings.len()
                    )
                } else {
                    "Calendar covers current board tickets · arrows day · j/k entry · a mine/team · v week/month".into()
                };
            }
            Ok(Data::Saved(key)) => {
                self.dialog = None;
                self.error = false;
                self.notice = format!("Saved {key} to Jira");
                self.refresh();
            }
            Ok(Data::Attachment(attachment, bytes)) => {
                match integrations::show_attachment(
                    &attachment,
                    &bytes,
                    self.dimensions.0,
                    self.dimensions.1,
                ) {
                    Ok(inline) => {
                        self.error = false;
                        if inline {
                            self.image_name = attachment.filename;
                            self.view = View::Image;
                            self.notice = "Image preview · Esc returns to attachments".into();
                        } else {
                            self.notice = "Opened attachment in your system viewer (inline graphics unavailable)".into();
                        }
                    }
                    Err(error) => self.fail(&error),
                }
            }
        }
    }

    fn fail(&mut self, error: &str) {
        self.error = true;
        self.notice = error.into();
    }

    pub fn groups(&self) -> Vec<Vec<usize>> {
        let query = self.search.to_lowercase();
        self.board
            .columns
            .iter()
            .map(|c| {
                self.board
                    .issues
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| {
                        c.statuses.contains(&i.status_id)
                            && (query.is_empty()
                                || format!(
                                    "{} {} {} {} {}",
                                    i.key,
                                    i.summary,
                                    i.assignee,
                                    i.priority,
                                    i.labels.join(" ")
                                )
                                .to_lowercase()
                                .contains(&query))
                    })
                    .map(|(index, _)| index)
                    .collect()
            })
            .collect()
    }

    pub fn selected_issue(&self) -> Option<&Issue> {
        self.groups()
            .get(self.column)
            .and_then(|g| g.get(self.row))
            .and_then(|i| self.board.issues.get(*i))
    }

    pub fn current_key(&self) -> Option<String> {
        if self.view == View::Details {
            self.details.as_ref().map(|d| d.issue.key.clone())
        } else if self.view == View::Calendar {
            self.day_logs()
                .get(self.calendar_item)
                .map(|l| l.key.clone())
                .or_else(|| self.selected_issue().map(|i| i.key.clone()))
        } else {
            self.selected_issue().map(|i| i.key.clone())
        }
    }

    fn clamp_selection(&mut self) {
        let groups = self.groups();
        self.column = self.column.min(groups.len().saturating_sub(1));
        self.row = self.row.min(
            groups
                .get(self.column)
                .map_or(0, |g| g.len().saturating_sub(1)),
        );
    }

    fn select_key(&mut self, key: &str) {
        for (column, group) in self.groups().iter().enumerate() {
            if let Some(row) = group.iter().position(|i| self.board.issues[*i].key == key) {
                self.column = column;
                self.row = row;
                break;
            }
        }
    }

    pub fn day_logs(&self) -> Vec<&Worklog> {
        let mut logs: Vec<_> = self
            .calendar_logs
            .iter()
            .filter(|l| {
                (self.calendar_all || l.account_id == self.calendar_owner)
                    && parse_started(&l.started)
                        .is_some_and(|t| t.date_naive() == self.calendar.selected)
            })
            .collect();
        logs.sort_by(|a, b| a.started.cmp(&b.started));
        logs
    }

    pub fn total_on(&self, date: NaiveDate) -> i64 {
        self.calendar_logs
            .iter()
            .filter(|l| {
                (self.calendar_all || l.account_id == self.calendar_owner)
                    && parse_started(&l.started).is_some_and(|t| t.date_naive() == date)
            })
            .map(|l| l.seconds)
            .sum()
    }

    fn choose_board(&mut self) {
        if self.demo {
            self.dialog = Some(Dialog::Boards {
                boards: vec![demo_board()],
                index: 0,
            });
        } else {
            self.start("Loading boards…", |client| {
                client.boards().map(Data::Boards)
            });
        }
    }

    fn load_board(&mut self, id: u64) {
        if self.demo {
            self.dialog = None;
            return;
        }
        self.start("Loading all board pages…", move |client| {
            client.board(id).map(Data::Board)
        });
    }

    fn load_details(&mut self, key: String) {
        if self.demo {
            let mut details = demo_details(&key);
            if let Some(old) = &self.details {
                if old.issue.key == key {
                    details = old.clone();
                }
            }
            if let Some(issue) = self.board.issues.iter().find(|i| i.key == key) {
                details.issue = issue.clone();
            }
            self.on_response(Ok(Data::Details(Box::new(details))));
        } else {
            self.start("Loading ticket, comments and worklogs…", move |client| {
                client
                    .details(&key)
                    .map(|details| Data::Details(Box::new(details)))
            });
        }
    }

    fn load_calendar(&mut self) {
        if self.demo {
            self.on_response(Ok(Data::Calendar(
                demo_details("DEMO-1").worklogs,
                "demo-user".into(),
                Vec::new(),
            )));
            return;
        }
        let issues = self.board.issues.clone();
        self.start(
            "Loading worklogs for current board tickets…",
            move |client| {
                let owner = client.myself()?;
                let mut logs = Vec::new();
                let mut warnings = Vec::new();
                // Bounded concurrency: four readers, never one thread per ticket.
                std::thread::scope(|scope| {
                    let handles: Vec<_> = issues
                        .chunks(issues.len().div_ceil(4).max(1))
                        .map(|chunk| {
                            let client = &client;
                            scope.spawn(move || {
                                let mut logs = Vec::new();
                                let mut warnings = Vec::new();
                                for issue in chunk {
                                    match client.worklogs(&issue.key) {
                                        Ok(values) => logs.extend(values),
                                        Err(error) => {
                                            warnings.push(format!("{}: {error}", issue.key))
                                        }
                                    }
                                }
                                (logs, warnings)
                            })
                        })
                        .collect();
                    for handle in handles {
                        match handle.join() {
                            Ok((ls, ws)) => {
                                logs.extend(ls);
                                warnings.extend(ws);
                            }
                            Err(_) => warnings.push("Worklog reader failed".into()),
                        }
                    }
                });
                Ok(Data::Calendar(logs, owner, warnings))
            },
        );
    }

    fn refresh(&mut self) {
        match self.view {
            View::Board => self.load_board(self.board.id),
            View::Details => {
                if let Some(key) = self.current_key() {
                    self.load_details(key);
                }
            }
            View::Calendar => self.load_calendar(),
            View::Image => {}
        }
    }

    fn transitions(&mut self, key: String, target: Option<usize>) {
        let statuses = target
            .and_then(|c| self.board.columns.get(c))
            .map(|c| c.statuses.clone());
        if self.demo {
            let list: Vec<_> = self
                .board
                .columns
                .iter()
                .flat_map(|c| {
                    c.statuses.iter().map(|id| Transition {
                        id: id.clone(),
                        name: c.name.clone(),
                        status_id: id.clone(),
                        required_fields: Vec::new(),
                    })
                })
                .filter(|t| statuses.as_ref().is_none_or(|ss| ss.contains(&t.status_id)))
                .collect();
            self.on_response(Ok(Data::Transitions(key, list)));
            return;
        }
        self.start("Checking available workflow transitions…", move |client| {
            let list = client
                .transitions(&key)?
                .into_iter()
                .filter(|t| statuses.as_ref().is_none_or(|ss| ss.contains(&t.status_id)))
                .collect();
            Ok(Data::Transitions(key, list))
        });
    }

    fn rank(&mut self, up: bool) {
        let groups = self.groups();
        let Some(group) = groups.get(self.column) else {
            return;
        };
        let Some(issue) = self.selected_issue() else {
            return;
        };
        let next = if up {
            self.row.checked_sub(1)
        } else {
            (self.row + 1 < group.len()).then_some(self.row + 1)
        };
        let Some(next) = next else {
            return;
        };
        let key = issue.key.clone();
        let neighbor = self.board.issues[group[next]].key.clone();
        let board = self.board.clone();
        if self.demo {
            self.board.issues.swap(group[self.row], group[next]);
            self.row = next;
            return;
        }
        self.write("Saving card order…", move |client| {
            client.rank(&board, &key, &neighbor, up)?;
            Ok(Data::Saved(key))
        });
    }

    fn new_worklog(&mut self, log: Option<Worklog>) {
        let key = log
            .as_ref()
            .map(|l| l.key.clone())
            .or_else(|| self.current_key());
        if let Some(key) = key {
            let date = (self.view == View::Calendar).then_some(self.calendar.selected);
            self.dialog = Some(Dialog::Worklog(WorklogForm::new(key, date, log.as_ref())));
        }
    }

    fn text_form(&mut self, title: &str, kind: TextKind, value: String) {
        self.dialog = Some(Dialog::Text(TextForm {
            title: title.into(),
            key: self.current_key().unwrap_or_default(),
            cursor: value.len(),
            value,
            kind,
        }));
    }

    fn edit_action(&mut self, index: usize) {
        let Some(key) = self.current_key() else {
            return;
        };
        let issue = if self.view == View::Details {
            self.details.as_ref().map(|d| &d.issue)
        } else {
            self.selected_issue()
        };
        match index {
            0 => {
                let value = issue.map(|i| i.summary.clone()).unwrap_or_default();
                self.text_form("Edit summary", TextKind::Summary, value);
            }
            1 => {
                let labels = issue.map(|i| i.labels.clone()).unwrap_or_default();
                self.text_form(
                    "Labels (comma separated)",
                    TextKind::Labels(labels.clone()),
                    labels.join(", "),
                );
            }
            2 => self.text_form(
                "Find assignee by name/email (empty = unassign)",
                TextKind::AssigneeSearch,
                String::new(),
            ),
            3 => {
                if self.demo {
                    self.dialog = Some(Dialog::Choices {
                        key,
                        field: "priority".into(),
                        items: vec![
                            ("1".into(), "High".into()),
                            ("2".into(), "Medium".into()),
                            ("3".into(), "Low".into()),
                        ],
                        index: 0,
                    });
                } else {
                    self.start("Loading priorities…", move |client| {
                        client
                            .priorities()
                            .map(|items| Data::Choices(key, "priority".into(), items))
                    });
                }
            }
            _ => {}
        }
    }

    fn edit_item(&mut self, reply: bool) {
        if self.view == View::Calendar {
            if let Some(log) = self.day_logs().get(self.calendar_item).cloned().cloned() {
                self.new_worklog(Some(log));
            }
            return;
        }
        let Some(details) = self.details.clone() else {
            return;
        };
        match self.detail_tab {
            1 => {
                if let Some(comment) = details.comments.get(self.item) {
                    let value = if reply {
                        let mention = if comment.account_id.is_empty() {
                            comment.author.clone()
                        } else {
                            format!(
                                "@{{{}|{}}}",
                                comment.account_id,
                                comment.author.replace(['{', '}', '|'], "")
                            )
                        };
                        format!(
                            "{mention}\n\n{}\n\n",
                            comment
                                .text
                                .lines()
                                .map(|l| format!("> {l}"))
                                .collect::<Vec<_>>()
                                .join("\n")
                        )
                    } else {
                        comment.text.clone()
                    };
                    self.text_form(
                        if reply {
                            "Reply (quoted comment + mention)"
                        } else {
                            "Edit comment (replaces formatting)"
                        },
                        TextKind::Comment(if reply {
                            None
                        } else {
                            Some(comment.id.clone())
                        }),
                        value,
                    );
                }
            }
            2 => {
                if let Some(log) = details.worklogs.get(self.item) {
                    self.new_worklog(Some(log.clone()));
                }
            }
            _ => self.dialog = Some(Dialog::EditMenu { index: 0 }),
        }
    }

    fn delete_item(&mut self) {
        if self.view == View::Calendar {
            if let Some(log) = self.day_logs().get(self.calendar_item) {
                self.dialog = Some(Dialog::Confirm {
                    key: log.key.clone(),
                    kind: "worklog".into(),
                    id: log.id.clone(),
                });
            }
            return;
        }
        let Some(details) = &self.details else {
            return;
        };
        let choice = match self.detail_tab {
            1 => details
                .comments
                .get(self.item)
                .map(|c| ("comment", c.id.clone())),
            2 => details
                .worklogs
                .get(self.item)
                .map(|l| ("worklog", l.id.clone())),
            _ => None,
        };
        if let Some((kind, id)) = choice {
            self.dialog = Some(Dialog::Confirm {
                key: details.issue.key.clone(),
                kind: kind.into(),
                id,
            });
        }
    }

    fn open_attachment(&mut self) {
        let Some(attachment) = self
            .details
            .as_ref()
            .and_then(|d| d.attachments.get(self.item))
            .cloned()
        else {
            return;
        };
        if self.demo {
            self.fail("Demo attachments are examples; no files are downloaded.");
            return;
        }
        self.start("Downloading attachment securely…", move |client| {
            let bytes = client.attachment(&attachment)?;
            Ok(Data::Attachment(attachment, bytes))
        });
    }

    fn submit(&mut self) {
        let Some(dialog) = self.dialog.clone() else {
            return;
        };
        match dialog {
            Dialog::Boards { boards, index } => {
                if let Some(board) = boards.get(index) {
                    self.view = View::Board;
                    self.load_board(board.id);
                }
            }
            Dialog::Transitions { key, list, index } => {
                if let Some(transition) = list.get(index).cloned() {
                    if self.demo {
                        if let Some(issue) = self.board.issues.iter_mut().find(|i| i.key == key) {
                            issue.status = transition.name;
                            issue.status_id = transition.status_id;
                        }
                        self.dialog = None;
                        self.select_key(&key);
                        self.notice = "Demo card moved locally".into();
                    } else {
                        self.write("Saving Jira transition…", move |client| {
                            client.transition(&key, &transition)?;
                            Ok(Data::Saved(key))
                        });
                    }
                }
            }
            Dialog::Choices {
                key,
                field,
                items,
                index,
            } => {
                if let Some((id, label)) = items.get(index).cloned() {
                    if self.demo {
                        if let Some(issue) = self.board.issues.iter_mut().find(|i| i.key == key) {
                            if field == "priority" {
                                issue.priority = label;
                            } else {
                                issue.assignee = label;
                            }
                        }
                        self.dialog = None;
                    } else {
                        self.write("Saving ticket field…", move |client| {
                            let mut fields = json!({});
                            fields[&field] = if field == "priority" {
                                json!({"id":id})
                            } else {
                                client.assignee_value(&id)
                            };
                            client.update_fields(&key, fields)?;
                            Ok(Data::Saved(key))
                        });
                    }
                }
            }
            Dialog::EditMenu { index } => self.edit_action(index),
            Dialog::Text(form) => {
                let key = form.key.clone();
                let value = form.value.trim().to_string();
                match &form.kind {
                    TextKind::Search => {
                        self.search = value;
                        self.dialog = None;
                        self.row = 0;
                        self.clamp_selection();
                    }
                    TextKind::AssigneeSearch if !value.is_empty() => {
                        if self.demo {
                            self.dialog = Some(Dialog::Choices {
                                key,
                                field: "assignee".into(),
                                items: vec![("demo-user".into(), "Samir".into())],
                                index: 0,
                            });
                        } else {
                            self.start("Finding assignable users…", move |client| {
                                client
                                    .assignable_users(&key, &value)
                                    .map(|items| Data::Choices(key, "assignee".into(), items))
                            });
                        }
                    }
                    TextKind::Comment(_) | TextKind::Summary if value.is_empty() => {
                        self.fail("This field cannot be empty")
                    }
                    _ => {
                        if self.demo {
                            self.apply_demo_text(&form);
                            self.dialog = None;
                            self.notice = "Saved locally in demo mode".into();
                            return;
                        }
                        self.write("Saving to Jira…", move |client| {
                            match form.kind {
                                TextKind::Labels(old) => {
                                    let new = split_labels(&value);
                                    client.labels(&key, &old, &new)?;
                                }
                                TextKind::Comment(id) => {
                                    client.save_comment(&key, id.as_deref(), &form.value)?
                                }
                                TextKind::Summary => {
                                    client.update_fields(&key, json!({"summary":value}))?
                                }
                                TextKind::AssigneeSearch => {
                                    client.update_fields(&key, json!({"assignee":Value::Null}))?
                                }
                                TextKind::Search => {}
                            }
                            Ok(Data::Saved(key))
                        });
                    }
                }
            }
            Dialog::Worklog(form) => {
                let started = match form.calendar.jira_started(&form.time) {
                    Ok(value) => value,
                    Err(error) => {
                        self.fail(&error);
                        return;
                    }
                };
                if form.duration.trim().is_empty() {
                    self.fail("Enter time spent, for example 1h 30m");
                    return;
                }
                if self.demo {
                    self.dialog = None;
                    self.notice = format!(
                        "Demo worklog validated: {} {} {} (not sent to Jira)",
                        form.key, form.calendar.selected, form.duration
                    );
                    self.error = false;
                    return;
                }
                self.write("Saving worklog…", move |client| {
                    client.save_worklog(
                        &form.key,
                        form.id.as_deref(),
                        WorklogInput {
                            duration: &form.duration,
                            started: &started,
                            comment: &form.comment,
                            estimate_mode: &form.estimate,
                            remaining: &form.remaining,
                        },
                    )?;
                    Ok(Data::Saved(form.key))
                });
            }
            Dialog::Confirm { key, kind, id } => {
                if self.demo {
                    self.dialog = None;
                    self.notice = "Demo deletion not sent to Jira".into();
                    return;
                }
                self.write("Deleting confirmed entry…", move |client| {
                    client.delete(&key, &kind, &id)?;
                    Ok(Data::Saved(key))
                });
            }
            Dialog::Help => self.dialog = None,
        }
    }

    fn apply_demo_text(&mut self, form: &TextForm) {
        if let Some(issue) = self.board.issues.iter_mut().find(|i| i.key == form.key) {
            match &form.kind {
                TextKind::Labels(_) => issue.labels = split_labels(&form.value),
                TextKind::Summary => issue.summary = form.value.clone(),
                TextKind::AssigneeSearch => issue.assignee = "Unassigned".into(),
                _ => {}
            }
        }
        if let Some(details) = &mut self.details {
            if let TextKind::Comment(id) = &form.kind {
                if let Some(comment) = id
                    .as_ref()
                    .and_then(|id| details.comments.iter_mut().find(|c| &c.id == id))
                {
                    comment.text = form.value.clone();
                } else {
                    details.comments.push(Comment {
                        id: format!("demo-{}", details.comments.len()),
                        author: "You".into(),
                        text: form.value.clone(),
                        ..Default::default()
                    });
                }
            }
            if let Some(issue) = self
                .board
                .issues
                .iter()
                .find(|i| i.key == details.issue.key)
            {
                details.issue = issue.clone();
            }
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if self.busy {
            if self.pending_write {
                return;
            } // Never abandon or duplicate an in-flight mutation.
            if matches!(key.code, KeyCode::Char('q'))
                || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
            {
                self.should_quit = true;
            } else if key.code == KeyCode::Esc {
                self.generation += 1;
                self.busy = false;
                self.dialog = None;
                self.notice = "Read cancelled; late responses will be ignored".into();
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        if self.dialog.is_some() {
            self.dialog_key(key);
            return;
        }
        if self.view == View::Image {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
                let _ = integrations::clear_graphics();
                self.view = View::Details;
            }
            return;
        }
        match key.code {
            KeyCode::Char('q') if self.view == View::Board => self.should_quit = true,
            KeyCode::Esc | KeyCode::Char('q') => {
                self.view = View::Board;
                self.item = 0;
                self.scroll = 0;
                self.clamp_selection();
            }
            KeyCode::Char('?') => self.dialog = Some(Dialog::Help),
            KeyCode::Char('b') => self.choose_board(),
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Char('/') => self.text_form(
                "Filter loaded board tickets",
                TextKind::Search,
                self.search.clone(),
            ),
            KeyCode::Char('o') => {
                if let Some(key) = self.current_key() {
                    if self.demo {
                        self.fail("Demo mode never opens external URLs");
                    } else if let Some(issue) = self.board.issues.iter().find(|i| i.key == key) {
                        if let Err(error) = integrations::open_url(&issue.url) {
                            self.fail(&error);
                        }
                    }
                }
            }
            KeyCode::Char('w') => self.new_worklog(None),
            KeyCode::Char('g') => self.load_calendar(),
            KeyCode::Char('m') => {
                if let Some(key) = self.current_key() {
                    self.transitions(key, None);
                }
            }
            KeyCode::Char('l') => self.edit_action(1),
            KeyCode::Char('c') | KeyCode::Char('i') => {
                self.detail_tab = if key.code == KeyCode::Char('c') { 1 } else { 3 };
                self.item = 0;
                self.scroll = 0;
                if self.view != View::Details {
                    if let Some(key) = self.current_key() {
                        self.load_details(key);
                    }
                }
            }
            _ => match self.view {
                View::Board => self.board_key(key),
                View::Details => self.details_key(key),
                View::Calendar => self.calendar_key(key),
                View::Image => {}
            },
        }
    }

    fn board_key(&mut self, key: KeyEvent) {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Up if shift => self.rank(true),
            KeyCode::Down if shift => self.rank(false),
            KeyCode::Left | KeyCode::Right if shift => {
                let target = if key.code == KeyCode::Left {
                    self.column.checked_sub(1)
                } else {
                    (self.column + 1 < self.board.columns.len()).then_some(self.column + 1)
                };
                if let (Some(target), Some(issue)) = (target, self.selected_issue()) {
                    self.transitions(issue.key.clone(), Some(target));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.row = self.row.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.row += 1;
                self.clamp_selection();
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.column = self.column.saturating_sub(1);
                self.clamp_selection();
            }
            KeyCode::Right => {
                self.column += 1;
                self.clamp_selection();
            }
            KeyCode::Enter => {
                if let Some(key) = self.selected_issue().map(|i| i.key.clone()) {
                    self.detail_tab = 0;
                    self.item = 0;
                    self.scroll = 0;
                    self.load_details(key);
                }
            }
            KeyCode::Char('e') => self.dialog = Some(Dialog::EditMenu { index: 0 }),
            _ => {}
        }
    }

    fn clamp_item(&mut self) {
        let count = self.details.as_ref().map_or(0, |d| match self.detail_tab {
            1 => d.comments.len(),
            2 => d.worklogs.len(),
            3 => d.attachments.len(),
            _ => 0,
        });
        self.item = self.item.min(count.saturating_sub(1));
    }

    fn details_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab | KeyCode::Right => {
                self.detail_tab = (self.detail_tab + 1) % 4;
                self.item = 0;
                self.scroll = 0;
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.detail_tab = (self.detail_tab + 3) % 4;
                self.item = 0;
                self.scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.detail_tab == 0 {
                    self.scroll = self.scroll.saturating_add(1);
                } else {
                    self.item += 1;
                    self.clamp_item();
                    self.scroll = 0;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.detail_tab == 0 {
                    self.scroll = self.scroll.saturating_sub(1);
                } else {
                    self.item = self.item.saturating_sub(1);
                    self.scroll = 0;
                }
            }
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(8),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(8),
            KeyCode::Char('n') => {
                if self.detail_tab == 1 {
                    self.text_form(
                        "New comment · @{accountId|Name} mentions",
                        TextKind::Comment(None),
                        String::new(),
                    );
                } else {
                    self.new_worklog(None);
                }
            }
            KeyCode::Char('e') => self.edit_item(false),
            KeyCode::Char('p') if self.detail_tab == 1 => self.edit_item(true),
            KeyCode::Char('D') => self.delete_item(),
            KeyCode::Enter if self.detail_tab == 3 => self.open_attachment(),
            _ => {}
        }
    }

    fn calendar_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left => self.calendar.shift_days(-1),
            KeyCode::Right => self.calendar.shift_days(1),
            KeyCode::Up => self.calendar.shift_days(-7),
            KeyCode::Down => self.calendar.shift_days(7),
            KeyCode::PageUp => self.calendar.shift_month(-1),
            KeyCode::PageDown => self.calendar.shift_month(1),
            KeyCode::Char('t') => self.calendar = Calendar::default(),
            KeyCode::Char('a') => self.calendar_all = !self.calendar_all,
            KeyCode::Char('v') => self.calendar_week = !self.calendar_week,
            KeyCode::Char('j') => {
                self.calendar_item =
                    (self.calendar_item + 1).min(self.day_logs().len().saturating_sub(1))
            }
            KeyCode::Char('k') => self.calendar_item = self.calendar_item.saturating_sub(1),
            KeyCode::Char('e') => self.edit_item(false),
            KeyCode::Char('D') => self.delete_item(),
            KeyCode::Enter => {
                if let Some(key) = self
                    .day_logs()
                    .get(self.calendar_item)
                    .map(|l| l.key.clone())
                {
                    self.detail_tab = 2;
                    self.item = 0;
                    self.load_details(key);
                }
            }
            _ => {}
        }
        self.calendar_item = self
            .calendar_item
            .min(self.day_logs().len().saturating_sub(1));
    }

    fn dialog_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.dialog = None;
            return;
        }
        if matches!(key.code, KeyCode::Enter | KeyCode::Char('s'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.submit();
            return;
        }
        let mut submit = false;
        match self.dialog.as_mut().unwrap() {
            Dialog::Text(form) => match key.code {
                KeyCode::Enter if matches!(form.kind, TextKind::Comment(_)) => {
                    crate::input::insert(&mut form.value, &mut form.cursor, "\n")
                }
                KeyCode::Enter => submit = true,
                _ => crate::input::edit(&mut form.value, &mut form.cursor, key),
            },
            Dialog::Worklog(form) => match key.code {
                KeyCode::Tab => form.field = (form.field + 1) % 8,
                KeyCode::BackTab => form.field = (form.field + 7) % 8,
                KeyCode::Left if form.field == 0 => form.calendar.shift_days(-1),
                KeyCode::Right if form.field == 0 => form.calendar.shift_days(1),
                KeyCode::Up if form.field == 0 => form.calendar.shift_days(-7),
                KeyCode::Down if form.field == 0 => form.calendar.shift_days(7),
                KeyCode::PageUp if form.field == 0 => form.calendar.shift_month(-1),
                KeyCode::PageDown if form.field == 0 => form.calendar.shift_month(1),
                KeyCode::Char('t') if form.field == 0 => form.calendar = Calendar::default(),
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.field == 4 => {
                    form.estimate = match form.estimate.as_str() {
                        "leave" => "auto",
                        "auto" => "new",
                        _ => "leave",
                    }
                    .into();
                }
                KeyCode::Enter if form.field == 3 => {
                    crate::input::insert(&mut form.comment, &mut form.cursors[2], "\n")
                }
                KeyCode::Enter if form.field == 7 => self.dialog = None,
                KeyCode::Enter if form.field == 6 => submit = true,
                KeyCode::Enter => form.field = (form.field + 1) % 8,
                _ => {
                    if let Some((value, cursor)) = worklog_input(form) {
                        crate::input::edit(value, cursor, key);
                    }
                }
            },
            Dialog::Boards { boards, index } => picker_key(key, index, boards.len(), &mut submit),
            Dialog::Transitions { list, index, .. } => {
                picker_key(key, index, list.len(), &mut submit)
            }
            Dialog::Choices { items, index, .. } => {
                picker_key(key, index, items.len(), &mut submit)
            }
            Dialog::EditMenu { index } => picker_key(key, index, 4, &mut submit),
            Dialog::Confirm { .. } => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Char('y')) {
                    submit = true;
                }
            }
            Dialog::Help => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Char('?')) {
                    self.dialog = None;
                }
            }
        }
        if submit {
            self.submit();
        }
    }

    pub fn on_paste(&mut self, value: &str) {
        if self.busy {
            return;
        }
        let safe: String = value
            .chars()
            .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
            .collect();
        match &mut self.dialog {
            Some(Dialog::Text(form)) => {
                crate::input::insert(&mut form.value, &mut form.cursor, &safe)
            }
            Some(Dialog::Worklog(form)) => {
                if let Some((value, cursor)) = worklog_input(form) {
                    crate::input::insert(value, cursor, &safe);
                }
            }
            _ => {}
        }
    }

    pub fn on_mouse(&mut self, event: MouseEvent) {
        if self.busy {
            return;
        }
        let x = event.column;
        let y = event.row;
        if let Some(dialog) = &mut self.dialog {
            if event.kind != MouseEventKind::Down(MouseButton::Left) {
                return;
            }
            if let Dialog::Worklog(form) = dialog {
                if let Some((_, date)) = self.hits.days.iter().find(|(r, _)| contains(*r, x, y)) {
                    form.calendar.selected = *date;
                    form.field = 0;
                }
                if let Some((_, field)) = self.hits.fields.iter().find(|(r, _)| contains(*r, x, y))
                {
                    form.field = *field;
                    if *field == 6 {
                        self.submit();
                    } else if *field == 7 {
                        self.dialog = None;
                    } else if *field == 4 {
                        if let Some(Dialog::Worklog(form)) = &mut self.dialog {
                            form.estimate = match form.estimate.as_str() {
                                "leave" => "auto",
                                "auto" => "new",
                                _ => "leave",
                            }
                            .into();
                        }
                    }
                }
            } else if let Some((_, choice)) =
                self.hits.picker.iter().find(|(r, _)| contains(*r, x, y))
            {
                match dialog {
                    Dialog::Boards { index, .. }
                    | Dialog::Transitions { index, .. }
                    | Dialog::Choices { index, .. }
                    | Dialog::EditMenu { index } => *index = *choice,
                    _ => {}
                }
                self.submit();
            }
            return;
        }
        if self.view == View::Board {
            match event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some((_, column, row)) = self
                        .hits
                        .cards
                        .iter()
                        .find(|(r, _, _)| contains(*r, x, y))
                        .copied()
                    {
                        self.column = column;
                        self.row = row;
                        self.drag = self.selected_issue().map(|i| (i.key.clone(), column));
                    }
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    if let Some((key, original)) = self.drag.take() {
                        if let Some((_, target)) = self
                            .hits
                            .columns
                            .iter()
                            .find(|(r, _)| contains(*r, x, y))
                            .copied()
                        {
                            if target != original {
                                self.transitions(key, Some(target));
                                return;
                            }
                        }
                        if self.last_click.as_ref().is_some_and(|(previous, time)| {
                            previous == &key && time.elapsed() < Duration::from_millis(450)
                        }) {
                            self.detail_tab = 0;
                            self.load_details(key);
                            self.last_click = None;
                        } else {
                            self.last_click = Some((key, Instant::now()));
                        }
                    }
                }
                MouseEventKind::ScrollDown => {
                    self.row += 1;
                    self.clamp_selection();
                }
                MouseEventKind::ScrollUp => self.row = self.row.saturating_sub(1),
                _ => {}
            }
        } else if event.kind == MouseEventKind::Down(MouseButton::Left) {
            if let Some((_, tab)) = self.hits.tabs.iter().find(|(r, _)| contains(*r, x, y)) {
                self.detail_tab = *tab;
                self.item = 0;
                self.scroll = 0;
            }
            if let Some((_, date)) = self.hits.days.iter().find(|(r, _)| contains(*r, x, y)) {
                self.calendar.selected = *date;
                self.calendar_item = 0;
            }
        }
    }
}

fn worklog_input(form: &mut WorklogForm) -> Option<(&mut String, &mut usize)> {
    match form.field {
        1 => Some((&mut form.time, &mut form.cursors[0])),
        2 => Some((&mut form.duration, &mut form.cursors[1])),
        3 => Some((&mut form.comment, &mut form.cursors[2])),
        5 => Some((&mut form.remaining, &mut form.cursors[3])),
        _ => None,
    }
}
fn picker_key(key: KeyEvent, index: &mut usize, count: usize, submit: &mut bool) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => *index = index.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => *index = (*index + 1).min(count.saturating_sub(1)),
        KeyCode::Enter if count > 0 => *submit = true,
        _ => {}
    }
}
fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x && x < rect.right() && y >= rect.y && y < rect.bottom()
}
fn split_labels(value: &str) -> Vec<String> {
    let mut labels: Vec<_> = value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    labels.sort();
    labels.dedup();
    labels
}

pub fn demo_board() -> Board {
    Board{id:1,name:"Engineering · demo".into(),rank_field:Some(10020),columns:vec![
        Column{name:"To do".into(),statuses:vec!["1".into()],max:None},Column{name:"In progress".into(),statuses:vec!["2".into()],max:Some(4)},
        Column{name:"In review".into(),statuses:vec!["3".into()],max:Some(3)},Column{name:"Done".into(),statuses:vec!["4".into()],max:None}],
        issues:vec![
            ("DEMO-1","Calendar-based Jira worklogs","1","To do","High","calendar"),
            ("DEMO-2","Keep board columns in sync","1","To do","Medium","board"),
            ("DEMO-3","Native ticket detail panel","2","In progress","High","workspace"),
            ("DEMO-4","Comments and quoted replies","3","In review","Medium","comments"),
            ("DEMO-5","Secure credential commands","4","Done","High","security")].into_iter().map(|(key,summary,status_id,status,priority,label)|Issue{
                key:key.into(),summary:summary.into(),status_id:status_id.into(),status:status.into(),priority:priority.into(),assignee:"Samir".into(),labels:vec![label.into()],
                issue_type:"Task".into(),url:format!("https://example.atlassian.net/browse/{key}"),description:"A Jira workspace that stays inside Herdr.\n\n• Use the real Jira board configuration\n• Log time through a visual calendar\n• Keep every action keyboard-accessible\n\nThis is demo data; no network requests are made.".into(),time_spent:5400,
            }).collect()}
}

fn demo_details(key: &str) -> Details {
    Details{issue:demo_board().issues.into_iter().find(|i|i.key==key).unwrap_or_default(),
        comments:vec![Comment{id:"1".into(),author:"Alex".into(),account_id:"demo-alex".into(),created:"2026-10-10T09:15:00.000+0000".into(),text:"Please keep the calendar navigable with arrow keys.\n\n[image/attachment: Calendar mockup]".into()},Comment{id:"2".into(),author:"Samir".into(),account_id:"demo-user".into(),created:"2026-10-10T09:30:00.000+0000".into(),text:"Agreed. Tab will move between date, time, duration, and work description.".into()}],
        worklogs:vec![Worklog{id:"1".into(),key:key.into(),author:"Samir".into(),account_id:"demo-user".into(),started:Calendar::default().jira_started("09:00").unwrap(),seconds:5400,text:"Built the calendar worklog dialog".into()}],
        attachments:vec![Attachment{id:"1".into(),filename:"calendar-mockup.png".into(),mime:"image/png".into(),size:42000,..Default::default()}],warnings:Vec::new()}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        let (tx, _) = std::sync::mpsc::channel();
        App::new(tx, true)
    }
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    #[test]
    fn moving_a_card_requires_confirmation() {
        let mut app = app();
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        assert!(matches!(app.dialog, Some(Dialog::Transitions { .. })));
        assert_eq!(app.board.issues[0].status_id, "1");
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.board.issues[0].status_id, "2");
    }
    #[test]
    fn failed_save_retains_form_and_unicode_input() {
        let mut app = app();
        app.on_key(key(KeyCode::Char('w')));
        app.on_key(key(KeyCode::Tab));
        app.on_paste("12:00");
        app.on_response(Err("HTTP 403".into()));
        assert!(matches!(app.dialog, Some(Dialog::Worklog(_))));
        assert!(app.error);
        app.dialog = Some(Dialog::Text(TextForm {
            title: "Comment".into(),
            key: "DEMO-1".into(),
            value: "नेपाल".into(),
            cursor: "नेपाल".len(),
            kind: TextKind::Comment(None),
        }));
        app.on_key(key(KeyCode::Backspace));
        if let Some(Dialog::Text(form)) = &app.dialog {
            assert_eq!(form.value, "नेपा");
        }
    }
    #[test]
    fn filters_do_not_change_the_board() {
        let mut app = app();
        app.search = "calendar".into();
        assert_eq!(app.groups().iter().map(Vec::len).sum::<usize>(), 1);
        assert_eq!(app.board.issues.len(), 5);
    }
    #[test]
    fn form_can_be_cancelled_and_duplicate_saves_are_blocked() {
        let mut app = app();
        app.on_key(key(KeyCode::Char('w')));
        app.busy = true;
        app.pending_write = true;
        app.on_key(key(KeyCode::Esc));
        assert!(app.dialog.is_some());
        app.busy = false;
        app.on_key(key(KeyCode::Esc));
        assert!(app.dialog.is_none());
    }

    #[test]
    fn cancelled_reads_cannot_replace_current_state() {
        let mut app = app();
        app.busy = true;
        app.generation = 7;
        app.on_key(key(KeyCode::Esc));
        assert!(!app.busy);
        app.on_message(Response {
            generation: 7,
            result: Ok(Data::Board(Board::default())),
        });
        assert_eq!(app.board.issues.len(), 5);
    }

    #[test]
    fn quitting_is_allowed_during_reads_but_not_during_writes() {
        let mut app = app();
        app.busy = true;
        app.pending_write = true;
        app.on_key(key(KeyCode::Char('q')));
        assert!(!app.should_quit);
        app.pending_write = false;
        app.on_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn mouse_drag_stages_a_move_without_mutating_jira_state() {
        let mut app = app();
        app.hits.cards.push((Rect::new(0, 0, 20, 6), 0, 0));
        app.hits
            .columns
            .extend([(Rect::new(0, 0, 20, 20), 0), (Rect::new(20, 0, 20, 20), 1)]);
        app.on_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 2,
            modifiers: KeyModifiers::NONE,
        });
        app.on_mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: 22,
            row: 2,
            modifiers: KeyModifiers::NONE,
        });
        assert!(matches!(app.dialog, Some(Dialog::Transitions { .. })));
        assert_eq!(app.board.issues[0].status_id, "1");
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.board.issues[0].status_id, "2");
    }

    #[test]
    fn calendar_selection_and_form_validation_preserve_input() {
        let mut app = app();
        app.on_key(key(KeyCode::Char('w')));
        let selected = match &app.dialog {
            Some(Dialog::Worklog(form)) => form.calendar.selected,
            _ => panic!(),
        };
        app.on_key(key(KeyCode::Right));
        if let Some(Dialog::Worklog(form)) = &mut app.dialog {
            assert_eq!(form.calendar.selected, selected + chrono::Duration::days(1));
            form.time = "25:00".into();
            form.duration = "1h".into();
        }
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        assert!(app.error);
        assert!(app.notice.contains("HH:MM"));
        assert!(matches!(app.dialog, Some(Dialog::Worklog(_))));
    }

    #[test]
    fn personal_and_team_calendar_totals_are_distinct() {
        let mut app = app();
        app.on_key(key(KeyCode::Char('g')));
        let date = app.calendar.selected;
        let mut other = app.calendar_logs[0].clone();
        other.account_id = "another-user".into();
        other.seconds = 1800;
        app.calendar_logs.push(other);
        assert_eq!(app.total_on(date), 5400);
        app.on_key(key(KeyCode::Char('a')));
        assert_eq!(app.total_on(date), 7200);
    }

    #[test]
    fn comments_allow_multiline_paste_and_quoted_replies() {
        let mut app = app();
        app.on_key(key(KeyCode::Char('c')));
        assert_eq!(app.view, View::Details);
        assert_eq!(app.detail_tab, 1);
        app.on_key(key(KeyCode::Char('p')));
        if let Some(Dialog::Text(form)) = &app.dialog {
            assert!(form.value.contains("@{demo-alex|Alex}"));
            assert!(form.value.contains("> Please"));
        } else {
            panic!("No reply form");
        }
        app.on_paste("Reply\nsecond line");
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        assert_eq!(app.details.as_ref().unwrap().comments.len(), 3);
        assert!(app.details.as_ref().unwrap().comments[2]
            .text
            .contains("Reply\nsecond line"));
    }
}
