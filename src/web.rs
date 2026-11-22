//! Веб-интерфейс: порт Flask-сервера из DeleteGdzSuper6.py.
//! Минимальный HTTP/1.1-сервер на std::net, без внешних веб-фреймворков.

use anyhow::{anyhow, Result};
use base64::Engine as _;
use engine::WatermarkRemover;
use serde_json::json;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::engine;

const HTML: &str = include_str!("../assets/web/index.html");
const MAX_BODY: usize = 100 * 1024 * 1024; // 100MB, как MAX_CONTENT_LENGTH

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

fn local_ip() -> String {
    match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => {
            let _ = s.connect("8.8.8.8:80");
            s.local_addr()
                .map(|a| a.ip().to_string())
                .unwrap_or_else(|_| "127.0.0.1".to_string())
        }
        Err(_) => "127.0.0.1".to_string(),
    }
}

fn respond(
    stream: &mut TcpStream,
    code: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
    extra: &[(&str, String)],
) -> Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        code,
        reason,
        content_type,
        body.len()
    );
    for (k, v) in extra {
        head.push_str(&format!("{}: {}\r\n", k, v));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

fn read_request(stream: &mut TcpStream) -> Result<(String, String, String, Vec<u8>)> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let header_end;
    loop {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Err(anyhow!("пустой запрос"));
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
        if buf.len() > 128 * 1024 {
            return Err(anyhow!("слишком большие заголовки"));
        }
    }

    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or("");
    let mut rl = request_line.split_whitespace();
    let method = rl.next().unwrap_or("").to_string();
    let path = rl.next().unwrap_or("/").to_string();

    let mut content_len = 0usize;
    let mut content_type = String::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => {
                    content_len = v.trim().parse().unwrap_or(0);
                }
                "content-type" => content_type = v.trim().to_string(),
                _ => {}
            }
        }
    }
    if content_len > MAX_BODY {
        return Err(anyhow!("файл слишком большой (максимум 100 МБ)"));
    }

    let mut body = buf[header_end..].to_vec();
    while body.len() < content_len {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    Ok((method, path, content_type, body))
}

fn split_bytes<'a>(data: &'a [u8], sep: &[u8]) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i + sep.len() <= data.len() {
        if &data[i..i + sep.len()] == sep {
            out.push(&data[start..i]);
            i += sep.len();
            start = i;
        } else {
            i += 1;
        }
    }
    out.push(&data[start..]);
    out
}

fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.png");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "file.png".to_string()
    } else {
        cleaned
    }
}

fn parse_multipart(body: &[u8], content_type: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let boundary = content_type
        .split("boundary=")
        .nth(1)
        .map(|s| s.trim().trim_matches('"').to_string())
        .ok_or_else(|| anyhow!("не указан boundary"))?;
    let delim = format!("--{}", boundary);
    let mut files = Vec::new();

    for part in split_bytes(body, delim.as_bytes()) {
        let mut part = part;
        while part.starts_with(b"\r\n") || part.starts_with(b"\n") {
            part = &part[1..];
        }
        if part.is_empty() || part.starts_with(b"--") {
            continue;
        }
        let pos = find_subslice(part, b"\r\n\r\n")
            .or_else(|| find_subslice(part, b"\n\n"))
            .ok_or_else(|| anyhow!("некорректная часть multipart"))?;
        let headers = String::from_utf8_lossy(&part[..pos]).to_string();
        let sep_len = if find_subslice(part, b"\r\n\r\n").is_some() {
            4
        } else {
            2
        };
        let mut content = part[pos + sep_len..].to_vec();
        while content.ends_with(b"\r\n") {
            content.truncate(content.len() - 2);
        }
        while content.ends_with(b"\n") {
            content.pop();
        }

        let filename = headers
            .split("filename=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .map(sanitize_filename);
        if let Some(name) = filename {
            files.push((name, content));
        }
    }
    Ok(files)
}

fn handle_client(
    mut stream: TcpStream,
    remover: Arc<WatermarkRemover>,
    uploads: PathBuf,
    cleaned: PathBuf,
) {
    let result = (|| -> Result<()> {
        let (method, path, content_type, body) = read_request(&mut stream)?;

        if method == "GET" && path == "/" {
            let html = HTML.replace("{{ templates_count }}", &remover.templates.len().to_string());
            respond(&mut stream, 200, "OK", "text/html; charset=utf-8", html.as_bytes(), &[])?;
            return Ok(());
        }

        if method == "GET" && path.starts_with("/download/") {
            let raw = path.trim_start_matches("/download/");
            let name = sanitize_filename(&raw.replace("%20", " "));
            let file_path = cleaned.join(&name);
            if !file_path.is_file() {
                respond(&mut stream, 404, "Not Found", "text/plain; charset=utf-8", b"File not found", &[])?;
                return Ok(());
            }
            let data = fs::read(&file_path)?;
            let disp = format!("attachment; filename=\"{}\"", name);
            respond(
                &mut stream,
                200,
                "OK",
                "application/octet-stream",
                &data,
                &[("Content-Disposition", disp)],
            )?;
            return Ok(());
        }

        if method == "POST" && path == "/process" {
            let files = parse_multipart(&body, &content_type)?;
            let mut results = Vec::new();

            for (original, content) in files {
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let temp_name = format!("{}_{}", ts, original);
                let temp_path = uploads.join(&temp_name);
                if let Err(e) = fs::write(&temp_path, &content) {
                    results.push(json!({
                        "original": original,
                        "success": false,
                        "message": format!("Ошибка записи: {}", e)
                    }));
                    continue;
                }

                let stem = Path::new(&original)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("image");

                match remover.process_bytes(&content) {
                    Ok((png, count)) if count > 0 => {
                        let out_path = cleaned.join(&temp_name);
                        if let Err(e) = fs::write(&out_path, &png) {
                            results.push(json!({
                                "original": original,
                                "success": false,
                                "message": format!("Ошибка сохранения: {}", e)
                            }));
                            continue;
                        }
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
                        results.push(json!({
                            "original": original,
                            "success": true,
                            "message": format!("Удалено {} водяных знаков", count),
                            "image_b64": b64,
                            "filename": temp_name,
                            "download_name": format!("{}_cleaned.png", stem)
                        }));
                    }
                    Ok(_) => {
                        results.push(json!({
                            "original": original,
                            "success": false,
                            "message": "Водяные знаки не найдены"
                        }));
                    }
                    Err(e) => {
                        results.push(json!({
                            "original": original,
                            "success": false,
                            "message": format!("Ошибка: {}", e)
                        }));
                    }
                }
            }
            let body = serde_json::to_vec(&results)?;
            respond(&mut stream, 200, "OK", "application/json; charset=utf-8", &body, &[])?;
            return Ok(());
        }

        respond(&mut stream, 404, "Not Found", "text/plain; charset=utf-8", b"Not found", &[])?;
        Ok(())
    })();

    if let Err(e) = result {
        let msg = format!("Ошибка сервера: {}", e);
        let _ = respond(&mut stream, 500, "Internal Server Error", "text/plain; charset=utf-8", msg.as_bytes(), &[]);
    }
}

pub fn run(remover: WatermarkRemover, host: &str, port: u16) -> Result<()> {
    let uploads = std::env::current_dir()?.join("web_temp_uploads");
    let cleaned = std::env::current_dir()?.join("web_temp_cleaned");
    fs::create_dir_all(&uploads)?;
    fs::create_dir_all(&cleaned)?;

    let listener = TcpListener::bind((host, port))?;
    let remover = Arc::new(remover);

    println!("\n============================================================");
    println!("🌐 ВЕБ-СЕРВЕР ЗАПУЩЕН");
    println!("============================================================");
    println!("📍 На этом компьютере: http://127.0.0.1:{}", port);
    println!("📱 В локальной сети:   http://{}:{}", local_ip(), port);
    println!("============================================================");
    println!("Нажмите Ctrl+C для остановки сервера\n");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let remover = Arc::clone(&remover);
                let uploads = uploads.clone();
                let cleaned = cleaned.clone();
                thread::spawn(move || handle_client(stream, remover, uploads, cleaned));
            }
            Err(e) => eprintln!("[WARN] Ошибка подключения: {}", e),
        }
    }
    Ok(())
}
