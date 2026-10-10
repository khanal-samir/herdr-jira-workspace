//! Jira REST client. Authentication and ADF traversal adapted from a2u/herdr-jira (MIT).
use base64::Engine;
use serde_json::{json, Value};
use std::{collections::HashSet, io::Read, time::Duration};

#[derive(Debug, Clone, Default)]
pub struct Issue {
    pub key: String,
    pub summary: String,
    pub status: String,
    pub status_id: String,
    pub issue_type: String,
    pub priority: String,
    pub assignee: String,
    pub labels: Vec<String>,
    pub description: String,
    pub url: String,
    pub time_spent: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Column {
    pub name: String,
    pub statuses: Vec<String>,
    pub max: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct Board {
    pub id: u64,
    pub name: String,
    pub columns: Vec<Column>,
    pub rank_field: Option<u64>,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone, Default)]
pub struct Transition {
    pub id: String,
    pub name: String,
    pub status_id: String,
    pub required_fields: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub account_id: String,
    pub created: String,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Worklog {
    pub id: String,
    pub key: String,
    pub author: String,
    pub account_id: String,
    pub started: String,
    pub seconds: i64,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Attachment {
    pub id: String,
    pub filename: String,
    pub mime: String,
    pub content_url: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Details {
    pub issue: Issue,
    pub comments: Vec<Comment>,
    pub worklogs: Vec<Worklog>,
    pub attachments: Vec<Attachment>,
    pub warnings: Vec<String>,
}

pub struct WorklogInput<'a> {
    pub duration: &'a str,
    pub started: &'a str,
    pub comment: &'a str,
    pub estimate_mode: &'a str,
    pub remaining: &'a str,
}

#[derive(Clone)]
pub struct JiraClient {
    base: String,
    auth_header: String,
    page_size: u32,
    agent: ureq::Agent,
    cloud: bool,
}

const FIELDS: &str = "summary,status,issuetype,priority,assignee,labels,description,timetracking";

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}
fn id(v: &Value) -> String {
    v.as_str()
        .map(String::from)
        .unwrap_or_else(|| v.as_u64().map(|n| n.to_string()).unwrap_or_default())
}
fn person(v: &Value) -> String {
    v["displayName"].as_str().unwrap_or("Unassigned").into()
}

impl JiraClient {
    pub fn new(cfg: &crate::config::Config) -> Result<Self, String> {
        let token = cfg.resolve_token()?;
        let auth_header = match cfg.jira.auth.as_str() {
            "bearer" => format!("Bearer {token}"),
            "basic" if !cfg.jira.email.trim().is_empty() => format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD
                    .encode(format!("{}:{token}", cfg.jira.email.trim()))
            ),
            _ => return Err("Basic authentication requires jira.email".into()),
        };
        Ok(Self {
            base: cfg.jira.base_url.clone(),
            auth_header,
            page_size: cfg.jira.max_results,
            cloud: cfg.jira.base_url.contains(".atlassian.net"),
            // Never send credentials through redirects to another origin.
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(25))
                .redirects(0)
                .build(),
        })
    }

    fn api(&self) -> &str {
        if self.cloud {
            "3"
        } else {
            "2"
        }
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Value, String> {
        let mut request = self
            .agent
            .request(method, &format!("{}{path}", self.base))
            .set("Authorization", &self.auth_header)
            .set("Accept", "application/json");
        for (key, value) in query {
            request = request.query(key, value);
        }
        let response = match body {
            Some(body) => request
                .set("Content-Type", "application/json")
                .send_json(body),
            None => request.call(),
        };
        match response {
            Ok(response) => {
                if response.status() >= 300 {
                    return Err(format!(
                        "HTTP {}: unexpected redirect; check Jira base_url",
                        response.status()
                    ));
                }
                let raw = response
                    .into_string()
                    .map_err(|_| "Could not read Jira response".to_string())?;
                if raw.trim().is_empty() {
                    return Ok(Value::Null);
                }
                serde_json::from_str(&raw).map_err(|_| "Jira returned invalid JSON".into())
            }
            Err(ureq::Error::Status(code, response)) => {
                let raw = response.into_string().unwrap_or_default();
                let body: Value = serde_json::from_str(&raw).unwrap_or_default();
                let mut messages = Vec::new();
                if let Some(errors) = body["errorMessages"].as_array() {
                    messages.extend(errors.iter().filter_map(Value::as_str).map(String::from));
                }
                if let Some(errors) = body["errors"].as_object() {
                    messages.extend(
                        errors.iter().map(|(k, v)| {
                            format!("{k}: {}", v.as_str().unwrap_or("invalid value"))
                        }),
                    );
                }
                if messages.is_empty() {
                    messages.push("Request rejected; check credentials and permissions".into());
                }
                Err(format!("HTTP {code}: {}", messages.join("; ")))
            }
            Err(_) if method != "GET" => Err(
                "Save outcome unknown: check Jira before retrying to avoid duplicate entries"
                    .into(),
            ),
            Err(_) => Err("Could not connect to Jira; check connectivity and retry".into()),
        }
    }

    /// Fetch every page, rejecting malformed/repeating pagination rather than silently truncating.
    fn pages(
        &self,
        path: &str,
        field: &str,
        token_based: bool,
        extra: &[(&str, String)],
    ) -> Result<Vec<Value>, String> {
        let mut values = Vec::new();
        let mut offset = 0;
        let mut token = String::new();
        let mut seen = HashSet::new();
        let mut previous_page: Option<Vec<Value>> = None;
        loop {
            let mut query = extra.to_vec();
            query.push(("maxResults", self.page_size.to_string()));
            if token_based {
                if !token.is_empty() {
                    query.push(("nextPageToken", token.clone()));
                }
            } else {
                query.push(("startAt", offset.to_string()));
            }
            let page = self.request("GET", path, &query, None)?;
            let items = page[field]
                .as_array()
                .ok_or_else(|| format!("Jira response has no {field} array"))?;
            if !token_based
                && previous_page
                    .as_ref()
                    .is_some_and(|last| !items.is_empty() && last == items)
            {
                return Err("Jira repeated an offset page; refusing incomplete results".into());
            }
            previous_page = Some(items.clone());
            values.extend(items.iter().cloned());
            if page["isLast"].as_bool() == Some(true) {
                break;
            }
            if token_based {
                token = text(&page["nextPageToken"]);
                if token.is_empty() {
                    if page["isLast"].as_bool() == Some(false) {
                        return Err("Missing Jira pagination token".into());
                    }
                    break;
                }
                if !seen.insert(token.clone()) {
                    return Err("Jira returned a repeated pagination token".into());
                }
            } else {
                offset += items.len();
                if items.is_empty()
                    || page["total"]
                        .as_u64()
                        .is_some_and(|total| offset as u64 >= total)
                {
                    break;
                }
                if page["total"].is_null()
                    && page["isLast"].is_null()
                    && items.len() < self.page_size as usize
                {
                    break;
                }
            }
        }
        Ok(values)
    }

    pub fn boards(&self) -> Result<Vec<Board>, String> {
        Ok(self
            .pages("/rest/agile/1.0/board", "values", false, &[])?
            .iter()
            .map(|v| Board {
                id: v["id"].as_u64().unwrap_or_default(),
                name: text(&v["name"]),
                ..Board::default()
            })
            .collect())
    }

    pub fn board(&self, board_id: u64) -> Result<Board, String> {
        let config = self.request(
            "GET",
            &format!("/rest/agile/1.0/board/{board_id}/configuration"),
            &[],
            None,
        )?;
        let columns = config["columnConfig"]["columns"]
            .as_array()
            .ok_or("Board has no column configuration")?
            .iter()
            .map(|c| Column {
                name: text(&c["name"]),
                max: c["max"].as_u64(),
                statuses: c["statuses"]
                    .as_array()
                    .map(|ss| ss.iter().map(|s| id(&s["id"])).collect())
                    .unwrap_or_default(),
            })
            .collect();
        let extra = [("fields", FIELDS.to_string())];
        let issues = if self.cloud {
            match self.pages(
                &format!("/rest/software/1.0/board/{board_id}/issue"),
                "issues",
                true,
                &extra,
            ) {
                Err(e) if e.starts_with("HTTP 404") || e.starts_with("HTTP 405") => self.pages(
                    &format!("/rest/agile/1.0/board/{board_id}/issue"),
                    "issues",
                    false,
                    &extra,
                )?,
                result => result?,
            }
        } else {
            self.pages(
                &format!("/rest/agile/1.0/board/{board_id}/issue"),
                "issues",
                false,
                &extra,
            )?
        };
        Ok(Board {
            id: board_id,
            name: text(&config["name"]),
            columns,
            rank_field: config["ranking"]["rankCustomFieldId"].as_u64(),
            issues: issues.iter().map(|v| self.parse_issue(v)).collect(),
        })
    }

    fn parse_issue(&self, v: &Value) -> Issue {
        let f = &v["fields"];
        let key = text(&v["key"]);
        Issue {
            url: format!("{}/browse/{key}", self.base),
            key,
            summary: text(&f["summary"]),
            status: text(&f["status"]["name"]),
            status_id: id(&f["status"]["id"]),
            issue_type: text(&f["issuetype"]["name"]),
            priority: text(&f["priority"]["name"]),
            assignee: person(&f["assignee"]),
            description: document_text(&f["description"]),
            labels: f["labels"]
                .as_array()
                .map(|ls| {
                    ls.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default(),
            time_spent: f["timetracking"]["timeSpentSeconds"].as_i64().unwrap_or(0),
        }
    }

    pub fn details(&self, key: &str) -> Result<Details, String> {
        let issue = self.request(
            "GET",
            &format!("/rest/api/{}/issue/{key}", self.api()),
            &[("fields", format!("{FIELDS},attachment"))],
            None,
        )?;
        let attachments = issue["fields"]["attachment"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|v| Attachment {
                        id: id(&v["id"]),
                        filename: text(&v["filename"]),
                        mime: text(&v["mimeType"]),
                        content_url: text(&v["content"]),
                        size: v["size"].as_u64().unwrap_or(0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut warnings = Vec::new();
        let comments = match self.pages(
            &format!("/rest/api/{}/issue/{key}/comment", self.api()),
            "comments",
            false,
            &[],
        ) {
            Ok(values) => values
                .iter()
                .map(|v| Comment {
                    id: id(&v["id"]),
                    author: person(&v["author"]),
                    account_id: self.person_id(&v["author"]),
                    created: text(&v["created"]),
                    text: document_text(&v["body"]),
                })
                .collect(),
            Err(error) => {
                warnings.push(format!("Comments unavailable: {error}"));
                Vec::new()
            }
        };
        let worklogs = match self.worklogs(key) {
            Ok(values) => values,
            Err(error) => {
                warnings.push(format!("Worklogs unavailable: {error}"));
                Vec::new()
            }
        };
        Ok(Details {
            issue: self.parse_issue(&issue),
            attachments,
            comments,
            worklogs,
            warnings,
        })
    }

    fn person_id(&self, person: &Value) -> String {
        text(if self.cloud {
            &person["accountId"]
        } else {
            &person["name"]
        })
    }

    pub fn myself(&self) -> Result<String, String> {
        let user = self.request(
            "GET",
            &format!("/rest/api/{}/myself", self.api()),
            &[],
            None,
        )?;
        Ok(self.person_id(&user))
    }

    pub fn transitions(&self, key: &str) -> Result<Vec<Transition>, String> {
        let result = self.request(
            "GET",
            &format!("/rest/api/{}/issue/{key}/transitions", self.api()),
            &[("expand", "transitions.fields".into())],
            None,
        )?;
        Ok(result["transitions"]
            .as_array()
            .ok_or("Missing transition list")?
            .iter()
            .map(|v| Transition {
                id: id(&v["id"]),
                name: text(&v["name"]),
                status_id: id(&v["to"]["id"]),
                required_fields: v["fields"]
                    .as_object()
                    .map(|fs| {
                        fs.iter()
                            .filter(|(_, f)| {
                                f["required"].as_bool() == Some(true)
                                    && f["hasDefaultValue"].as_bool() != Some(true)
                            })
                            .map(|(key, _)| key.clone())
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect())
    }

    pub fn transition(&self, key: &str, transition: &Transition) -> Result<(), String> {
        if !transition.required_fields.is_empty() {
            return Err(format!("This transition requires {}. Open the ticket in Jira to complete its workflow form.", transition.required_fields.join(", ")));
        }
        self.request(
            "POST",
            &format!("/rest/api/{}/issue/{key}/transitions", self.api()),
            &[],
            Some(json!({"transition":{"id":transition.id}})),
        )?;
        Ok(())
    }

    pub fn rank(
        &self,
        board: &Board,
        key: &str,
        neighbor: &str,
        before: bool,
    ) -> Result<(), String> {
        let mut body = json!({"issues":[key]});
        body[if before {
            "rankBeforeIssue"
        } else {
            "rankAfterIssue"
        }] = json!(neighbor);
        if let Some(field) = board.rank_field {
            body["rankCustomFieldId"] = json!(field);
        }
        let result = self.request("PUT", "/rest/agile/1.0/issue/rank", &[], Some(body))?;
        if !result.is_null() {
            return Err(
                "Jira returned a partial ranking result; refresh the board to verify".into(),
            );
        }
        Ok(())
    }

    pub fn priorities(&self) -> Result<Vec<(String, String)>, String> {
        let values = self.request(
            "GET",
            &format!("/rest/api/{}/priority", self.api()),
            &[],
            None,
        )?;
        Ok(values
            .as_array()
            .ok_or("Missing priority list")?
            .iter()
            .map(|v| (id(&v["id"]), text(&v["name"])))
            .collect())
    }

    pub fn assignee_value(&self, account: &str) -> Value {
        if self.cloud {
            json!({"accountId":account})
        } else {
            json!({"name":account})
        }
    }

    pub fn assignable_users(
        &self,
        key: &str,
        query: &str,
    ) -> Result<Vec<(String, String)>, String> {
        let result = self.request(
            "GET",
            &format!("/rest/api/{}/user/assignable/search", self.api()),
            &[
                ("issueKey", key.into()),
                (if self.cloud { "query" } else { "username" }, query.into()),
                ("maxResults", "100".into()),
            ],
            None,
        )?;
        Ok(result
            .as_array()
            .ok_or("Missing user list")?
            .iter()
            .map(|v| (self.person_id(v), person(v)))
            .collect())
    }

    pub fn update_fields(&self, key: &str, fields: Value) -> Result<(), String> {
        self.request(
            "PUT",
            &format!("/rest/api/{}/issue/{key}", self.api()),
            &[],
            Some(json!({"fields": fields})),
        )?;
        Ok(())
    }

    pub fn labels(&self, key: &str, old: &[String], new: &[String]) -> Result<(), String> {
        // Incremental operations preserve unrelated labels added by other people.
        let mut changes: Vec<Value> = old
            .iter()
            .filter(|l| !new.contains(l))
            .map(|l| json!({"remove":l}))
            .collect();
        changes.extend(
            new.iter()
                .filter(|l| !old.contains(l))
                .map(|l| json!({"add":l})),
        );
        if !changes.is_empty() {
            self.request(
                "PUT",
                &format!("/rest/api/{}/issue/{key}", self.api()),
                &[],
                Some(json!({"update":{"labels":changes}})),
            )?;
        }
        Ok(())
    }

    fn document(&self, value: &str) -> Value {
        if !self.cloud {
            return json!(value);
        }
        json!({"type":"doc","version":1,"content":value.split('\n').map(|line| {
            if line.is_empty() { json!({"type":"paragraph","content":[]}) }
            else { json!({"type":"paragraph","content":mention_nodes(line)}) }
        }).collect::<Vec<_>>()})
    }

    pub fn save_comment(
        &self,
        key: &str,
        comment_id: Option<&str>,
        value: &str,
    ) -> Result<(), String> {
        if value.trim().is_empty() {
            return Err("Comment cannot be empty".into());
        }
        let path = format!(
            "/rest/api/{}/issue/{key}/comment{}",
            self.api(),
            comment_id.map(|s| format!("/{s}")).unwrap_or_default()
        );
        self.request(
            if comment_id.is_some() { "PUT" } else { "POST" },
            &path,
            &[],
            Some(json!({"body":self.document(value)})),
        )?;
        Ok(())
    }

    pub fn worklogs(&self, key: &str) -> Result<Vec<Worklog>, String> {
        Ok(self
            .pages(
                &format!("/rest/api/{}/issue/{key}/worklog", self.api()),
                "worklogs",
                false,
                &[],
            )?
            .iter()
            .map(|v| Worklog {
                id: id(&v["id"]),
                key: key.into(),
                author: person(&v["author"]),
                account_id: self.person_id(&v["author"]),
                started: text(&v["started"]),
                seconds: v["timeSpentSeconds"].as_i64().unwrap_or(0),
                text: document_text(&v["comment"]),
            })
            .collect())
    }

    pub fn save_worklog(
        &self,
        key: &str,
        worklog_id: Option<&str>,
        input: WorklogInput<'_>,
    ) -> Result<(), String> {
        let WorklogInput {
            duration,
            started,
            comment,
            estimate_mode,
            remaining,
        } = input;
        if duration.trim().is_empty() {
            return Err("Enter time spent, for example 1h 30m".into());
        }
        if !["leave", "auto", "new"].contains(&estimate_mode) {
            return Err("Estimate mode must be leave, auto, or new".into());
        }
        if estimate_mode == "new" && remaining.trim().is_empty() {
            return Err("Enter a new remaining estimate".into());
        }
        let mut query = vec![("adjustEstimate", estimate_mode.into())];
        if estimate_mode == "new" {
            query.push(("newEstimate", remaining.into()));
        }
        let mut body = json!({"timeSpent":duration,"started":started});
        if !comment.trim().is_empty() || worklog_id.is_some() {
            body["comment"] = self.document(comment);
        }
        let path = format!(
            "/rest/api/{}/issue/{key}/worklog{}",
            self.api(),
            worklog_id.map(|s| format!("/{s}")).unwrap_or_default()
        );
        self.request(
            if worklog_id.is_some() { "PUT" } else { "POST" },
            &path,
            &query,
            Some(body),
        )?;
        Ok(())
    }

    pub fn delete(&self, key: &str, kind: &str, resource_id: &str) -> Result<(), String> {
        if !["comment", "worklog"].contains(&kind) {
            return Err("Unsupported delete action".into());
        }
        let query = if kind == "worklog" {
            vec![("adjustEstimate", "leave".into())]
        } else {
            vec![]
        };
        self.request(
            "DELETE",
            &format!("/rest/api/{}/issue/{key}/{kind}/{resource_id}", self.api()),
            &query,
            None,
        )?;
        Ok(())
    }

    pub fn attachment(&self, attachment: &Attachment) -> Result<Vec<u8>, String> {
        const LIMIT: u64 = 20 * 1024 * 1024;
        if attachment.size > LIMIT {
            return Err("Attachment exceeds the 20 MiB viewer limit; open it in Jira".into());
        }
        if !attachment.id.chars().all(|c| c.is_ascii_digit()) || attachment.id.is_empty() {
            return Err("Invalid attachment ID".into());
        }
        let url = if self.cloud {
            format!(
                "{}/rest/api/3/attachment/content/{}?redirect=false",
                self.base, attachment.id
            )
        } else {
            if !attachment
                .content_url
                .starts_with(&format!("{}/", self.base))
            {
                return Err("Refusing to send credentials to an external attachment URL".into());
            }
            attachment.content_url.clone()
        };
        let response = self
            .agent
            .get(&url)
            .set("Authorization", &self.auth_header)
            .call()
            .map_err(|_| "Could not download attachment; open it in Jira".to_string())?;
        if response.status() >= 300 {
            return Err("Attachment requires a redirect; open it in Jira".into());
        }
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Could not read attachment".to_string())?;
        if bytes.len() as u64 > LIMIT {
            return Err("Attachment exceeds the 20 MiB viewer limit".into());
        }
        Ok(bytes)
    }
}

fn mention_nodes(mut line: &str) -> Vec<Value> {
    let mut nodes = Vec::new();
    while let Some(start) = line.find("@{") {
        let rest = &line[start + 2..];
        let Some(end) = rest.find('}') else {
            break;
        };
        let Some((account, name)) = rest[..end].split_once('|') else {
            break;
        };
        if account.is_empty() || name.is_empty() {
            break;
        }
        if start > 0 {
            nodes.push(json!({"type":"text","text":&line[..start]}));
        }
        nodes.push(json!({"type":"mention","attrs":{"id":account,"text":format!("@{name}")}}));
        line = &rest[end + 1..];
    }
    if !line.is_empty() {
        nodes.push(json!({"type":"text","text":line}));
    }
    nodes
}

pub fn document_text(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        return s.into();
    }
    fn walk(value: &Value, out: &mut String) {
        match value["type"].as_str().unwrap_or("") {
            "text" => {
                out.push_str(value["text"].as_str().unwrap_or_default());
                return;
            }
            "hardBreak" => {
                out.push('\n');
                return;
            }
            "mention" => {
                out.push_str(value["attrs"]["text"].as_str().unwrap_or("@user"));
                return;
            }
            "inlineCard" | "blockCard" => {
                out.push_str(value["attrs"]["url"].as_str().unwrap_or_default());
                out.push('\n');
                return;
            }
            "media" => {
                out.push_str("[image/attachment: ");
                out.push_str(value["attrs"]["alt"].as_str().unwrap_or("see Attachments"));
                out.push_str("]\n");
                return;
            }
            "listItem" => out.push_str("• "),
            _ => {}
        }
        if let Some(children) = value["content"].as_array() {
            for child in children {
                walk(child, out);
            }
        }
        if matches!(
            value["type"].as_str(),
            Some("paragraph" | "heading" | "codeBlock" | "blockquote" | "listItem")
        ) && !out.ends_with('\n')
        {
            out.push('\n');
        }
    }
    let mut out = String::new();
    walk(value, &mut out);
    out.trim().into()
}

#[cfg(test)]
mod tests;
