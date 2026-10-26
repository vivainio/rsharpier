//! `server`: a small HTTP server for editor integrations, POST /format with
//! `{ "fileName": "...", "fileContents": "..." }`

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::config::OptionsProvider;
use super::ignore_file::IgnoreFile;
use super::is_generated_code_file;
use crate::options::{format_contents, FormatOutcome, Formatter};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatFileParameter {
    pub file_contents: String,
    pub file_name: String,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Formatted,
    Ignored,
    Failed,
    UnsupportedFile,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FormatFileResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formatted_file: Option<String>,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl FormatFileResult {
    fn new(status: Status) -> Self {
        FormatFileResult { formatted_file: None, status, error_message: None }
    }
    fn failed(message: impl Into<String>) -> Self {
        FormatFileResult { error_message: Some(message.into()), ..FormatFileResult::new(Status::Failed) }
    }
}

pub fn format_file(parameter: &FormatFileParameter) -> FormatFileResult {
    let result = std::panic::catch_unwind(|| format_file_inner(parameter));
    match result {
        Ok(r) => r,
        Err(_) => FormatFileResult::failed("An exception was thrown"),
    }
}

fn format_file_inner(parameter: &FormatFileParameter) -> FormatFileResult {
    let path = super::full_path(&parameter.file_name);
    let Some(directory) = path.parent() else {
        return FormatFileResult::failed(format!("There was no directory found for file {}", parameter.file_name));
    };
    let ignored = IgnoreFile::create(directory, None).map(|f| f.is_ignored(&path, false)).unwrap_or(false);
    if is_generated_code_file(&path) || ignored {
        return FormatFileResult::new(Status::Ignored);
    }
    let provider = OptionsProvider::new(None).unwrap();
    let Some(options) = provider.options_for(&path) else {
        return FormatFileResult::new(Status::UnsupportedFile);
    };
    if options.formatter == Formatter::Unknown {
        return FormatFileResult::new(Status::UnsupportedFile);
    }
    match format_contents(&parameter.file_contents, &options, false, true) {
        FormatOutcome::Formatted(code) => FormatFileResult { formatted_file: Some(code), ..FormatFileResult::new(Status::Formatted) },
        FormatOutcome::SyntaxError(_) => FormatFileResult::failed("File had compilation errors and could not be formatted"),
        FormatOutcome::Warning(w) | FormatOutcome::Failure(w) => FormatFileResult::failed(w),
    }
}

fn handle_connection(stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut stream = stream;
    loop {
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
            return;
        }
        let mut content_length = 0usize;
        let mut keep_alive = true;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                match k.to_lowercase().as_str() {
                    "content-length" => content_length = v.trim().parse().unwrap_or(0),
                    "connection" => keep_alive = !v.trim().eq_ignore_ascii_case("close"),
                    _ => {}
                }
            }
        }
        let mut body = vec![0u8; content_length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        let mut parts = request_line.split_whitespace();
        let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let path = target.split('?').next().unwrap_or("");

        let response = if path == "/format" && method == "POST" {
            match serde_json::from_slice::<FormatFileParameter>(&body) {
                Ok(p) => {
                    let json = serde_json::to_vec(&format_file(&p)).unwrap();
                    format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}\r\n", json.len(), if keep_alive { "" } else { "Connection: close\r\n" })
                        .into_bytes()
                        .into_iter()
                        .chain(json)
                        .collect::<Vec<u8>>()
                }
                Err(_) => b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n".to_vec(),
            }
        } else {
            b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n".to_vec()
        };
        if stream.write_all(&response).is_err() || !keep_alive {
            return;
        }
    }
}

pub fn run_server(port: Option<u16>) -> i32 {
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, port.unwrap_or(0))) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Could not start the server: {e}");
            return 1;
        }
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    println!("Started on {port}");
    let _ = std::io::stdout().flush();
    for stream in listener.incoming().flatten() {
        std::thread::spawn(move || handle_connection(stream));
    }
    0
}

#[allow(dead_code)]
fn unused(_: &Path) {}
