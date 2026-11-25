//! wasi:http を非ブロッキングに使う小さなクライアント。
//!
//! ```ignore
//! let mut req = http::Fetch::get("https://example.com/")?;
//! // on_event(Event::Io) のたびに:
//! if let Some(result) = req.poll() { /* 完了 */ }
//! ```
//!
//! `poll` はブロックしない。ホストは future / stream が進むと `Event::Io` を送るので、
//! そのたびに呼べばよい。

use crate::bindings::wasi::http::outgoing_handler;
use crate::bindings::wasi::http::types::{
    ErrorCode, Fields, FutureIncomingResponse, IncomingBody, Method, OutgoingBody, OutgoingRequest,
    Scheme,
};
use crate::bindings::wasi::io::streams::{InputStream, StreamError};

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn header(&self, name: &str) -> Option<&[u8]> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_slice())
    }
}

enum State {
    Waiting(FutureIncomingResponse),
    // フィールドの宣言順 = drop 順。stream は body の子リソースなので先に drop する
    Body {
        stream: InputStream,
        body: IncomingBody,
        status: u16,
        headers: Vec<(String, Vec<u8>)>,
        buf: Vec<u8>,
    },
    Done,
}

pub struct Fetch {
    state: State,
}

impl Fetch {
    pub fn get(url: &str) -> Result<Fetch, String> {
        Self::request("GET", url, &[], None)
    }

    pub fn request(
        method: &str,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Result<Fetch, String> {
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| format!("bad url: {url}"))?;
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };

        let fields = Fields::new();
        for (k, v) in headers {
            fields
                .append(k, v.as_bytes())
                .map_err(|e| format!("header {k}: {e:?}"))?;
        }
        let req = OutgoingRequest::new(fields);
        let method = match method.to_ascii_uppercase().as_str() {
            "GET" => Method::Get,
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "DELETE" => Method::Delete,
            "PATCH" => Method::Patch,
            "HEAD" => Method::Head,
            "OPTIONS" => Method::Options,
            other => Method::Other(other.to_string()),
        };
        req.set_method(&method).map_err(|_| "method")?;
        let scheme = match scheme {
            "https" => Scheme::Https,
            "http" => Scheme::Http,
            other => Scheme::Other(other.to_string()),
        };
        req.set_scheme(Some(&scheme)).map_err(|_| "scheme")?;
        req.set_authority(Some(authority))
            .map_err(|_| "authority")?;
        req.set_path_with_query(Some(path)).map_err(|_| "path")?;

        let out = req.body().map_err(|_| "body")?;
        if let Some(bytes) = body {
            let stream = out.write().map_err(|_| "body stream")?;
            for chunk in bytes.chunks(4096) {
                stream
                    .blocking_write_and_flush(chunk)
                    .map_err(|e| format!("write: {e:?}"))?;
            }
        }
        OutgoingBody::finish(out, None).map_err(|e| format!("finish: {e:?}"))?;

        let future = outgoing_handler::handle(req, None).map_err(fmt_error)?;
        Ok(Fetch {
            state: State::Waiting(future),
        })
    }

    /// 完了していれば `Some`。完了後に呼ぶと `Some(Err)`。
    pub fn poll(&mut self) -> Option<Result<Response, String>> {
        loop {
            match &mut self.state {
                State::Waiting(future) => {
                    let resp = match future.get()? {
                        Ok(Ok(resp)) => resp,
                        Ok(Err(e)) => return self.fail(fmt_error(e)),
                        Err(()) => return self.fail("response already taken".into()),
                    };
                    let status = resp.status();
                    let headers = resp.headers().entries();
                    let body = match resp.consume() {
                        Ok(b) => b,
                        Err(()) => return self.fail("consume".into()),
                    };
                    let stream = match body.stream() {
                        Ok(s) => s,
                        Err(()) => return self.fail("body stream".into()),
                    };
                    self.state = State::Body {
                        stream,
                        body,
                        status,
                        headers,
                        buf: Vec::new(),
                    };
                }
                State::Body { stream, buf, .. } => match stream.read(64 * 1024) {
                    Ok(chunk) if chunk.is_empty() => return None,
                    Ok(chunk) => buf.extend_from_slice(&chunk),
                    Err(StreamError::Closed) => {
                        let State::Body {
                            stream,
                            body,
                            status,
                            headers,
                            buf,
                        } = std::mem::replace(&mut self.state, State::Done)
                        else {
                            unreachable!()
                        };
                        drop(stream);
                        drop(IncomingBody::finish(body));
                        return Some(Ok(Response {
                            status,
                            headers,
                            body: buf,
                        }));
                    }
                    Err(StreamError::LastOperationFailed(e)) => {
                        return self.fail(format!("read: {}", e.to_debug_string()))
                    }
                },
                State::Done => return Some(Err("already finished".into())),
            }
        }
    }

    fn fail(&mut self, msg: String) -> Option<Result<Response, String>> {
        self.state = State::Done;
        Some(Err(msg))
    }
}

fn fmt_error(e: ErrorCode) -> String {
    format!("{e:?}")
}
