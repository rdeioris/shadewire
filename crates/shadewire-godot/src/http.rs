//! The transport: Godot's `HTTPClient`, driven by a blocking poll loop on the worker thread that fetches the
//! material. It follows the project's TLS setup (its certificate bundle) and keeps the library free of a TLS stack.
//! One connection per request.

use std::time::{Duration, Instant};

use godot::classes::http_client::{Method, Status};
use godot::classes::{HttpClient, TlsOptions};
use godot::global::Error as GdError;
use godot::prelude::*;

use crate::fetch::{Response, Transport, Wire};
use crate::headers::Header;
use crate::url::Url;

/// Between polls: short enough for local servers, long enough not to spin.
const POLL: Duration = Duration::from_millis(1);

pub struct GodotHttp;

impl Transport for GodotHttp {
    fn get(&self, url: &Url, headers: &[Header], wire: &Wire) -> Result<Response, String> {
        let name = url.to_string();
        let mut client = HttpClient::new_gd();
        let tls = if url.https { TlsOptions::client() } else { None };
        let status = client.connect_to_host_ex(&url.host).port(url.port as i32).tls_options(tls.as_ref()).done();
        if status != GdError::OK {
            return Err(format!("{name}: cannot connect ({status:?})"));
        }
        let connect_deadline = Instant::now() + wire.connect_timeout;
        loop {
            client.poll();
            match client.get_status() {
                Status::RESOLVING | Status::CONNECTING => {}
                Status::CONNECTED => break,
                status => return Err(format!("{name}: {}", failure(status))),
            }
            wire.check(&name)?;
            if Instant::now() > connect_deadline {
                return Err(format!("{name}: connect timed out"));
            }
            std::thread::sleep(POLL);
        }

        let lines: PackedStringArray = headers.iter().map(|h| GString::from(h.line().as_str())).collect();
        let status = client.request(Method::GET, &url.target, &lines);
        if status != GdError::OK {
            return Err(format!("{name}: cannot send the request ({status:?})"));
        }
        loop {
            client.poll();
            match client.get_status() {
                Status::REQUESTING => {}
                Status::BODY | Status::CONNECTED => break,
                status => return Err(format!("{name}: {}", failure(status))),
            }
            wire.check(&name)?;
            std::thread::sleep(POLL);
        }
        if !client.has_response() {
            return Err(format!("{name}: no response"));
        }

        let status = client.get_response_code() as u16;
        let response_headers = client
            .get_response_headers()
            .as_slice()
            .iter()
            .filter_map(|line| {
                let line = line.to_string();
                let (name, value) = line.split_once(':')?;
                Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
            })
            .collect();
        let length = client.get_response_body_length();
        if length > 0 && length as u64 > wire.max_bytes {
            return Err(format!("{name}: {length} bytes exceed max_response_bytes ({})", wire.max_bytes));
        }
        let known = length >= 0;
        if known {
            (wire.progress)(0, length as u64);
        }
        let mut body = Vec::with_capacity(if known { length as usize } else { 0 });
        // A known length ends the body; polling past it would see the server close the connection (HTTP/1.0).
        while client.get_status() == Status::BODY && !(known && body.len() as i64 >= length) {
            let chunk = client.read_response_body_chunk();
            if chunk.is_empty() {
                client.poll();
                wire.check(&name)?;
                std::thread::sleep(POLL);
                continue;
            }
            body.extend_from_slice(chunk.as_slice());
            if body.len() as u64 > wire.max_bytes {
                return Err(format!("{name}: the response exceeds max_response_bytes ({})", wire.max_bytes));
            }
            let n = chunk.len() as u64;
            (wire.progress)(n, if known { 0 } else { n });
        }
        if known && body.len() as i64 != length {
            return Err(format!("{name}: the response ended after {} of {length} bytes", body.len()));
        }
        if !known && !matches!(client.get_status(), Status::CONNECTED | Status::DISCONNECTED) {
            return Err(format!("{name}: {}", failure(client.get_status())));
        }
        client.close();
        Ok(Response { status, headers: response_headers, body })
    }
}

fn failure(status: Status) -> &'static str {
    match status {
        Status::CANT_RESOLVE => "cannot resolve the host",
        Status::CANT_CONNECT => "cannot connect",
        Status::CONNECTION_ERROR => "connection error",
        Status::TLS_HANDSHAKE_ERROR => "TLS handshake failed",
        Status::DISCONNECTED => "disconnected",
        _ => "unexpected connection state",
    }
}
