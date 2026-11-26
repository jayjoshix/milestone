use milestone_adversarial::attack_spec;
use milestone_core::{to_yaml_string, MilestoneSpec};
use milestone_lint::evaluate_listability;
use milestone_parser::parse_headline;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

const INDEX_HTML: &str = include_str!("index.html");

#[derive(Deserialize)]
struct ParseReq {
    headline: String,
}

#[derive(Deserialize)]
struct AnalyzeReq {
    spec: MilestoneSpec,
}

#[derive(Serialize)]
struct AnalyzeResp {
    spec: MilestoneSpec,
    yaml: String,
    listability: milestone_lint::ListabilityReport,
    attack: milestone_adversarial::AttackReport,
}

pub fn serve(addr: &str) -> Result<(), String> {
    let listener =
        TcpListener::bind(addr).map_err(|e| format!("Failed to bind {}: {}", addr, e))?;
    println!(
        "milestone-spec interactive demo listening on http://{}",
        addr
    );
    println!("Press Ctrl+C to stop.");

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Err(e) = handle_conn(s) {
                    eprintln!("request error: {}", e);
                }
            }
            Err(e) => eprintln!("accept error: {}", e),
        }
    }
    Ok(())
}

fn handle_conn(mut stream: TcpStream) -> Result<(), String> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|e| e.to_string())?;
    if request_line.trim().is_empty() {
        return Ok(());
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).map_err(|e| e.to_string())?;
        let trimmed = header.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse::<usize>().unwrap_or(0);
            }
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    }

    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => write_response(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            INDEX_HTML.as_bytes(),
        ),
        ("POST", "/api/parse") => {
            let req: ParseReq =
                serde_json::from_slice(&body).map_err(|e| format!("bad parse JSON: {}", e))?;
            let out = parse_headline(&req.headline).map_err(|e| e.to_string())?;
            let json = serde_json::to_vec(&out).map_err(|e| e.to_string())?;
            write_response(&mut stream, "200 OK", "application/json", &json)
        }
        ("POST", "/api/analyze") => {
            let req: AnalyzeReq =
                serde_json::from_slice(&body).map_err(|e| format!("bad analyze JSON: {}", e))?;
            let listability = evaluate_listability(&req.spec);
            let attack = attack_spec(&req.spec);
            let yaml = to_yaml_string(&req.spec).unwrap_or_default();
            let resp = AnalyzeResp {
                spec: req.spec,
                yaml,
                listability,
                attack,
            };
            let json = serde_json::to_vec(&resp).map_err(|e| e.to_string())?;
            write_response(&mut stream, "200 OK", "application/json", &json)
        }
        _ => write_response(&mut stream, "404 Not Found", "text/plain", b"Not Found"),
    }
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn web_demo_endpoints_work() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for s in listener.incoming().take(3).flatten() {
                let _ = handle_conn(s);
            }
        });

        // 1. GET /
        let mut s1 = TcpStream::connect(addr).unwrap();
        s1.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut resp1 = String::new();
        s1.read_to_string(&mut resp1).unwrap();
        assert!(resp1.starts_with("HTTP/1.1 200 OK"));
        assert!(resp1.contains("milestone-spec"));

        // 2. POST /api/parse
        let body2 = br#"{"headline":"Will Acme raise $5M before Dec 31?"}"#;
        let req2 = format!(
            "POST /api/parse HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body2.len()
        );
        let mut s2 = TcpStream::connect(addr).unwrap();
        s2.write_all(req2.as_bytes()).unwrap();
        s2.write_all(body2).unwrap();
        let mut resp2 = String::new();
        s2.read_to_string(&mut resp2).unwrap();
        assert!(resp2.starts_with("HTTP/1.1 200 OK"));
        let json2 = resp2.split("\r\n\r\n").nth(1).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(json2).unwrap();
        assert_eq!(parsed["status"], "PARSED_WITH_GAPS");

        // 3. POST /api/analyze
        let body3 = serde_json::to_vec(&serde_json::json!({ "spec": parsed["spec"] })).unwrap();
        let req3 = format!(
            "POST /api/analyze HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body3.len()
        );
        let mut s3 = TcpStream::connect(addr).unwrap();
        s3.write_all(req3.as_bytes()).unwrap();
        s3.write_all(&body3).unwrap();
        let mut resp3 = String::new();
        s3.read_to_string(&mut resp3).unwrap();
        assert!(resp3.starts_with("HTTP/1.1 200 OK"));
        let json3 = resp3.split("\r\n\r\n").nth(1).unwrap();
        let analyzed: serde_json::Value = serde_json::from_str(json3).unwrap();
        assert_eq!(analyzed["listability"]["status"], "NOT_LISTABLE");
        assert!(analyzed["attack"]["ambiguous_count"].as_u64().unwrap() >= 5);
    }
}
