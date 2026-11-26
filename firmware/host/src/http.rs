//! wasi:http / wasi:io のリソースの中身（ResourceTable に入れる）。
//!
//! 実際の通信は [`crate::Platform::http_start`] に任せ、ここは状態を持つだけ。
//! 本文は送信前に全部書く前提（`outgoing-body.finish` の後に `handle`）。
//! 応答の本文はプラットフォームが全部受け取ってから future を完了させるので、
//! `input-stream.read` は常に即座に返る。

use alloc::string::String;
use alloc::vec::Vec;

use crate::bindings::wasi::http::types::{Method, Scheme};

pub struct Fields {
    pub entries: Vec<(String, Vec<u8>)>,
    pub immutable: bool,
}

pub struct OutgoingRequest {
    pub method: Method,
    pub scheme: Option<Scheme>,
    pub authority: Option<String>,
    pub path_with_query: Option<String>,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
    pub body_taken: bool,
}

/// `outgoing-request.body()` の戻り値。書き込み先のリクエストを rep で指す
pub struct OutgoingBody {
    pub request: u32,
    pub stream_taken: bool,
}

pub struct OutputStream {
    pub request: u32,
}

pub struct RequestOptions;

pub struct FutureIncomingResponse {
    pub handle: u32,
    pub taken: bool,
}

pub struct IncomingResponse {
    pub status: u16,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Option<Vec<u8>>,
}

pub struct IncomingBody {
    pub data: Option<Vec<u8>>,
}

pub struct InputStream {
    pub data: Vec<u8>,
    pub pos: usize,
}

pub struct FutureTrailers;

pub enum Pollable {
    Ready,
    /// HTTP の完了待ち
    Http(u32),
    /// monotonic ns
    At(u64),
}

pub struct IoError(pub String);

/// サーバ側（incoming-request 等）。アプリでは使わない
pub struct Unsupported;

pub fn method_name(m: &Method) -> String {
    use alloc::string::ToString;
    match m {
        Method::Get => "GET".into(),
        Method::Head => "HEAD".into(),
        Method::Post => "POST".into(),
        Method::Put => "PUT".into(),
        Method::Delete => "DELETE".into(),
        Method::Connect => "CONNECT".into(),
        Method::Options => "OPTIONS".into(),
        Method::Trace => "TRACE".into(),
        Method::Patch => "PATCH".into(),
        Method::Other(s) => s.to_string(),
    }
}
