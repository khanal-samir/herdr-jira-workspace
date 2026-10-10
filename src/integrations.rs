use crate::jira::Attachment;
use base64::Engine;
use serde_json::{json, Value};
use std::os::unix::{
    fs::{OpenOptionsExt, PermissionsExt},
    net::UnixStream,
};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

pub fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("Refusing to open a non-HTTPS Jira URL".into());
    }
    open(url)
}

fn open(target: &str) -> Result<(), String> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(opener)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| format!("Could not start {opener}; install a system viewer"))?;
    Ok(())
}

fn graphics(method: &str, params: Value) -> Result<Value, String> {
    let socket =
        std::env::var("HERDR_SOCKET_PATH").map_err(|_| "Herdr socket unavailable".to_string())?;
    request_at(Path::new(&socket), method, params)
}

fn request_at(socket: &Path, method: &str, params: Value) -> Result<Value, String> {
    let mut stream =
        UnixStream::connect(socket).map_err(|_| "Could not connect to Herdr".to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| "Could not configure socket".to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| "Could not configure socket".to_string())?;
    let request = json!({"id":"jira-workspace","method":method,"params":params});
    writeln!(stream, "{request}").map_err(|_| "Could not send graphics request".to_string())?;
    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .map_err(|_| "Herdr graphics response timed out".to_string())?;
    let value: Value =
        serde_json::from_str(&response).map_err(|_| "Invalid graphics response".to_string())?;
    if value.get("error").is_some() || value.get("result").is_none() {
        return Err("Herdr request unavailable or rejected".into());
    }
    Ok(value["result"].clone())
}

pub fn notify(title: &str, body: &str) -> Result<bool, String> {
    let socket =
        std::env::var("HERDR_SOCKET_PATH").map_err(|_| "Herdr socket unavailable".to_string())?;
    notify_at(Path::new(&socket), title, body)
}
fn notify_at(socket: &Path, title: &str, body: &str) -> Result<bool, String> {
    let value = request_at(
        socket,
        "notification.show",
        json!({"title":title,"body":body,"sound":"none"}),
    )?;
    value["shown"]
        .as_bool()
        .ok_or("Invalid Herdr notification response".into())
}

pub fn clear_graphics() -> Result<(), String> {
    let pane = std::env::var("HERDR_PANE_ID").map_err(|_| "No plugin pane".to_string())?;
    graphics("pane.graphics.clear", json!({"pane_id":pane}))?;
    Ok(())
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    (width > 0 && height > 0 && width as u64 * height as u64 <= 40_000_000)
        .then_some((width, height))
}

/// Returns true for an inline Herdr preview, false for the external viewer.
pub fn show_attachment(
    attachment: &Attachment,
    bytes: &[u8],
    columns: u16,
    rows: u16,
) -> Result<bool, String> {
    if attachment.mime == "image/png" && bytes.len() < 8 * 1024 * 1024 {
        if let (Some((width, height)), Ok(pane)) =
            (png_dimensions(bytes), std::env::var("HERDR_PANE_ID"))
        {
            if graphics("pane.graphics.info", json!({"pane_id":pane})).is_ok() {
                let result = graphics(
                    "pane.graphics.set",
                    json!({
                        "pane_id":pane,"format":"png","image_width":width,"image_height":height,
                        "data_base64":base64::engine::general_purpose::STANDARD.encode(bytes),
                        "placement":{"viewport_col":1,"viewport_row":5,"grid_cols":columns.saturating_sub(2).max(1),"grid_rows":rows.saturating_sub(10).max(1)}
                    }),
                );
                if result.is_ok() {
                    return Ok(true);
                }
            }
        }
    }
    if attachment.id.is_empty() || !attachment.id.chars().all(|c| c.is_ascii_digit()) {
        return Err("Invalid attachment ID".into());
    }
    let root = std::env::var_os("HERDR_PLUGIN_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join(".local/state/herdr/plugins/herdr-jira-workspace")
        })
        .join("attachments");
    fs::create_dir_all(&root)
        .map_err(|_| "Could not create private attachment cache".to_string())?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
        .map_err(|_| "Could not protect attachment cache".to_string())?;
    let suffix = match attachment.mime.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        _ => "bin",
    };
    let path = root.join(format!("{}.{}", attachment.id, suffix));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|_| "Could not cache attachment".to_string())?;
    file.write_all(bytes)
        .map_err(|_| "Could not save attachment".to_string())?;
    open(path.to_str().ok_or("Invalid attachment cache path")?)?;
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_toast_uses_result_envelope_and_respects_delivery_policy() {
        for shown in [true, false] {
            let path = std::env::temp_dir().join(format!(
                "jira-notify-{}-{}",
                std::process::id(),
                chrono::Local::now().timestamp_nanos_opt().unwrap()
            ));
            let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], "notification.show");
                assert_eq!(
                    request["params"],
                    json!({"title":"Jira","body":"Update","sound":"none"})
                );
                writeln!(stream,"{}",json!({"id":request["id"],"result":{"shown":shown,"reason":if shown{"shown"}else{"busy"}}})).unwrap();
            });
            assert_eq!(notify_at(&path, "Jira", "Update").unwrap(), shown);
            server.join().unwrap();
            std::fs::remove_file(path).unwrap();
        }
    }
    #[test]
    fn png_headers_are_validated_before_rendering() {
        assert!(png_dimensions(b"not a png").is_none());
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(800u32.to_be_bytes());
        bytes.extend(600u32.to_be_bytes());
        assert_eq!(png_dimensions(&bytes), Some((800, 600)));
        bytes[16..20].copy_from_slice(&100_000u32.to_be_bytes());
        assert!(png_dimensions(&bytes).is_none());
    }
    #[test]
    fn unsafe_browser_schemes_are_rejected() {
        assert!(open_url("javascript:alert(1)").is_err());
        assert!(open_url("file:///etc/passwd").is_err());
    }
}
