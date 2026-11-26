//! WIT の import の実装。未対応の関数は trap（`Err`）にする。

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use wasmtime::component::Resource;
use wasmtime::{format_err, Result};

use crate::bindings::wasi::clocks::monotonic_clock;
use crate::bindings::wasi::http::{outgoing_handler, types as ht};
use crate::bindings::wasi::io::{error as ioe, poll, streams};
use crate::bindings::wasmos::os::{gfx, system, types as ost};
use crate::http::*;
use crate::platform::{HttpRequest, Platform};
use crate::render::{HEIGHT, WIDTH};
use crate::runtime::State;

fn unsupported<T>(what: &str) -> Result<T> {
    Err(format_err!("{what} is not supported on this host"))
}

// --- wasmos:os ---------------------------------------------------------------

impl<P: Platform> ost::Host for State<P> {}

impl<P: Platform> gfx::Host for State<P> {
    fn screen_size(&mut self) -> Result<(u32, u32)> {
        Ok((WIDTH, HEIGHT))
    }
    fn measure_text(&mut self, text: String, size: u16) -> Result<u32> {
        Ok(self.renderer.measure_text(&text, size))
    }
    fn load_image(&mut self, _data: Vec<u8>) -> Result<u32> {
        // PNG/JPEG のデコードは未実装: id だけ返し、image-size は None
        Ok(self.renderer.reserve_image())
    }
    fn image_from_rgba(&mut self, width: u32, height: u32, pixels: Vec<u8>) -> Result<u32> {
        if pixels.len() < (width as usize) * (height as usize) * 4 {
            return Err(format_err!("image-from-rgba: pixels too short"));
        }
        Ok(self.renderer.add_rgba(width, height, &pixels))
    }
    fn image_size(&mut self, id: u32) -> Result<Option<(u32, u32)>> {
        Ok(self.renderer.image_size(id))
    }
    fn drop_image(&mut self, id: u32) -> Result<()> {
        self.renderer.drop_image(id);
        Ok(())
    }
    fn submit(&mut self, frame: Vec<gfx::Cmd>) -> Result<()> {
        self.renderer.submit(&frame);
        Ok(())
    }
}

impl<P: Platform> system::Host for State<P> {
    fn log(&mut self, message: String) -> Result<()> {
        State::log(self, &message);
        Ok(())
    }
    fn now_ms(&mut self) -> Result<u64> {
        Ok(self.p.now_ms())
    }
    fn wall_ms(&mut self) -> Result<Option<u64>> {
        Ok(self.p.wall_ms())
    }
    fn request_frame(&mut self) -> Result<()> {
        self.frame_requested = true;
        Ok(())
    }
    fn set_timer(&mut self, ms: u32) -> Result<u32> {
        let id = self.next_timer;
        self.next_timer = self.next_timer.wrapping_add(1).max(1);
        let due = self.p.now_ms() + ms as u64;
        self.timers.push((id, due));
        Ok(id)
    }
    fn cancel_timer(&mut self, id: u32) -> Result<()> {
        self.timers.retain(|t| t.0 != id);
        Ok(())
    }
    fn installed(&mut self) -> Result<Vec<system::AppInfo>> {
        Ok(State::installed(self))
    }
    fn launch(&mut self, id: String, args: String) -> Result<()> {
        self.switch = Some((id, args));
        Ok(())
    }
    fn exit(&mut self) -> Result<()> {
        self.switch = Some((self.launcher.clone(), String::new()));
        Ok(())
    }
}

// --- wasi:clocks / wasi:io ---------------------------------------------------

impl<P: Platform> monotonic_clock::Host for State<P> {
    fn now(&mut self) -> Result<u64> {
        Ok(self.p.now_ms() * 1_000_000)
    }
    fn resolution(&mut self) -> Result<u64> {
        Ok(1_000_000)
    }
    fn subscribe_instant(&mut self, when: u64) -> Result<Resource<Pollable>> {
        Ok(self.table.push(Pollable::At(when))?)
    }
    fn subscribe_duration(&mut self, d: u64) -> Result<Resource<Pollable>> {
        let at = self.p.now_ms() * 1_000_000 + d;
        Ok(self.table.push(Pollable::At(at))?)
    }
}

impl<P: Platform> State<P> {
    fn pollable_ready(&mut self, p: &Resource<Pollable>) -> Result<bool> {
        let now = self.p.now_ms() * 1_000_000;
        Ok(match self.table.get(p)? {
            Pollable::Ready => true,
            Pollable::At(t) => now >= *t,
            Pollable::Http(h) => {
                let h = *h;
                self.p.http_ready(h)
            }
        })
    }
}

impl<P: Platform> poll::HostPollable for State<P> {
    fn ready(&mut self, p: Resource<Pollable>) -> Result<bool> {
        self.pollable_ready(&p)
    }
    fn block(&mut self, p: Resource<Pollable>) -> Result<()> {
        if self.pollable_ready(&p)? {
            Ok(())
        } else {
            Err(format_err!(
                "pollable.block would block: apps must be event driven"
            ))
        }
    }
    fn drop(&mut self, rep: Resource<Pollable>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl<P: Platform> poll::Host for State<P> {
    fn poll(&mut self, list: Vec<Resource<Pollable>>) -> Result<Vec<u32>> {
        let mut ready = Vec::new();
        for (i, p) in list.iter().enumerate() {
            if self.pollable_ready(p)? {
                ready.push(i as u32);
            }
        }
        if ready.is_empty() {
            return Err(format_err!("poll would block: apps must be event driven"));
        }
        Ok(ready)
    }
}

impl<P: Platform> ioe::HostError for State<P> {
    fn to_debug_string(&mut self, e: Resource<IoError>) -> Result<String> {
        Ok(self.table.get(&e)?.0.clone())
    }
    fn drop(&mut self, rep: Resource<IoError>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}
impl<P: Platform> ioe::Host for State<P> {}

impl<P: Platform> streams::HostInputStream for State<P> {
    fn read(
        &mut self,
        s: Resource<InputStream>,
        len: u64,
    ) -> Result<Result<Vec<u8>, streams::StreamError>> {
        let st = self.table.get_mut(&s)?;
        if st.pos >= st.data.len() {
            return Ok(Err(streams::StreamError::Closed));
        }
        let end = st.data.len().min(st.pos + len as usize);
        let out = st.data[st.pos..end].to_vec();
        st.pos = end;
        Ok(Ok(out))
    }
    fn blocking_read(
        &mut self,
        s: Resource<InputStream>,
        len: u64,
    ) -> Result<Result<Vec<u8>, streams::StreamError>> {
        self.read(s, len)
    }
    fn skip(
        &mut self,
        s: Resource<InputStream>,
        len: u64,
    ) -> Result<Result<u64, streams::StreamError>> {
        Ok(self.read(s, len)?.map(|v| v.len() as u64))
    }
    fn blocking_skip(
        &mut self,
        s: Resource<InputStream>,
        len: u64,
    ) -> Result<Result<u64, streams::StreamError>> {
        self.skip(s, len)
    }
    fn subscribe(&mut self, _s: Resource<InputStream>) -> Result<Resource<Pollable>> {
        Ok(self.table.push(Pollable::Ready)?)
    }
    fn drop(&mut self, rep: Resource<InputStream>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl<P: Platform> State<P> {
    fn out_write(
        &mut self,
        s: &Resource<OutputStream>,
        bytes: &[u8],
    ) -> Result<Result<(), streams::StreamError>> {
        let req = self.table.get(s)?.request;
        let req: Resource<OutgoingRequest> = Resource::new_borrow(req);
        match self.table.get_mut(&req) {
            Ok(r) => {
                r.body.extend_from_slice(bytes);
                Ok(Ok(()))
            }
            // handle 後の書き込み（ストリーミング送信）は未対応
            Err(_) => Ok(Err(streams::StreamError::Closed)),
        }
    }
}

impl<P: Platform> streams::HostOutputStream for State<P> {
    fn check_write(
        &mut self,
        _s: Resource<OutputStream>,
    ) -> Result<Result<u64, streams::StreamError>> {
        Ok(Ok(1 << 20))
    }
    fn write(
        &mut self,
        s: Resource<OutputStream>,
        contents: Vec<u8>,
    ) -> Result<Result<(), streams::StreamError>> {
        self.out_write(&s, &contents)
    }
    fn blocking_write_and_flush(
        &mut self,
        s: Resource<OutputStream>,
        contents: Vec<u8>,
    ) -> Result<Result<(), streams::StreamError>> {
        self.out_write(&s, &contents)
    }
    fn flush(&mut self, _s: Resource<OutputStream>) -> Result<Result<(), streams::StreamError>> {
        Ok(Ok(()))
    }
    fn blocking_flush(
        &mut self,
        _s: Resource<OutputStream>,
    ) -> Result<Result<(), streams::StreamError>> {
        Ok(Ok(()))
    }
    fn subscribe(&mut self, _s: Resource<OutputStream>) -> Result<Resource<Pollable>> {
        Ok(self.table.push(Pollable::Ready)?)
    }
    fn write_zeroes(
        &mut self,
        s: Resource<OutputStream>,
        len: u64,
    ) -> Result<Result<(), streams::StreamError>> {
        let zeros = alloc::vec![0u8; len as usize];
        self.out_write(&s, &zeros)
    }
    fn blocking_write_zeroes_and_flush(
        &mut self,
        s: Resource<OutputStream>,
        len: u64,
    ) -> Result<Result<(), streams::StreamError>> {
        self.write_zeroes(s, len)
    }
    fn splice(
        &mut self,
        _s: Resource<OutputStream>,
        _src: Resource<InputStream>,
        _len: u64,
    ) -> Result<Result<u64, streams::StreamError>> {
        unsupported("output-stream.splice")
    }
    fn blocking_splice(
        &mut self,
        _s: Resource<OutputStream>,
        _src: Resource<InputStream>,
        _len: u64,
    ) -> Result<Result<u64, streams::StreamError>> {
        unsupported("output-stream.blocking-splice")
    }
    fn drop(&mut self, rep: Resource<OutputStream>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}
impl<P: Platform> streams::Host for State<P> {}

// --- wasi:http ---------------------------------------------------------------

fn valid_name(n: &str) -> bool {
    !n.is_empty()
        && n.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

impl<P: Platform> ht::HostFields for State<P> {
    fn new(&mut self) -> Result<Resource<Fields>> {
        Ok(self.table.push(Fields {
            entries: Vec::new(),
            immutable: false,
        })?)
    }
    fn from_list(
        &mut self,
        entries: Vec<(String, Vec<u8>)>,
    ) -> Result<Result<Resource<Fields>, ht::HeaderError>> {
        if entries.iter().any(|(k, _)| !valid_name(k)) {
            return Ok(Err(ht::HeaderError::InvalidSyntax));
        }
        let entries = entries
            .into_iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v))
            .collect();
        Ok(Ok(self.table.push(Fields {
            entries,
            immutable: false,
        })?))
    }
    fn get(&mut self, f: Resource<Fields>, name: String) -> Result<Vec<Vec<u8>>> {
        let n = name.to_ascii_lowercase();
        Ok(self
            .table
            .get(&f)?
            .entries
            .iter()
            .filter(|(k, _)| *k == n)
            .map(|(_, v)| v.clone())
            .collect())
    }
    fn has(&mut self, f: Resource<Fields>, name: String) -> Result<bool> {
        let n = name.to_ascii_lowercase();
        Ok(self.table.get(&f)?.entries.iter().any(|(k, _)| *k == n))
    }
    fn set(
        &mut self,
        f: Resource<Fields>,
        name: String,
        value: Vec<Vec<u8>>,
    ) -> Result<Result<(), ht::HeaderError>> {
        let fl = self.table.get_mut(&f)?;
        if fl.immutable {
            return Ok(Err(ht::HeaderError::Immutable));
        }
        if !valid_name(&name) {
            return Ok(Err(ht::HeaderError::InvalidSyntax));
        }
        let n = name.to_ascii_lowercase();
        fl.entries.retain(|(k, _)| *k != n);
        fl.entries.extend(value.into_iter().map(|v| (n.clone(), v)));
        Ok(Ok(()))
    }
    fn delete(&mut self, f: Resource<Fields>, name: String) -> Result<Result<(), ht::HeaderError>> {
        let fl = self.table.get_mut(&f)?;
        if fl.immutable {
            return Ok(Err(ht::HeaderError::Immutable));
        }
        let n = name.to_ascii_lowercase();
        fl.entries.retain(|(k, _)| *k != n);
        Ok(Ok(()))
    }
    fn append(
        &mut self,
        f: Resource<Fields>,
        name: String,
        value: Vec<u8>,
    ) -> Result<Result<(), ht::HeaderError>> {
        let fl = self.table.get_mut(&f)?;
        if fl.immutable {
            return Ok(Err(ht::HeaderError::Immutable));
        }
        if !valid_name(&name) {
            return Ok(Err(ht::HeaderError::InvalidSyntax));
        }
        fl.entries.push((name.to_ascii_lowercase(), value));
        Ok(Ok(()))
    }
    fn entries(&mut self, f: Resource<Fields>) -> Result<Vec<(String, Vec<u8>)>> {
        Ok(self.table.get(&f)?.entries.clone())
    }
    fn clone(&mut self, f: Resource<Fields>) -> Result<Resource<Fields>> {
        let entries = self.table.get(&f)?.entries.clone();
        Ok(self.table.push(Fields {
            entries,
            immutable: false,
        })?)
    }
    fn drop(&mut self, rep: Resource<Fields>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl<P: Platform> ht::HostIncomingRequest for State<P> {
    fn method(&mut self, _: Resource<Unsupported>) -> Result<ht::Method> {
        unsupported("incoming-request")
    }
    fn path_with_query(&mut self, _: Resource<Unsupported>) -> Result<Option<String>> {
        unsupported("incoming-request")
    }
    fn scheme(&mut self, _: Resource<Unsupported>) -> Result<Option<ht::Scheme>> {
        unsupported("incoming-request")
    }
    fn authority(&mut self, _: Resource<Unsupported>) -> Result<Option<String>> {
        unsupported("incoming-request")
    }
    fn headers(&mut self, _: Resource<Unsupported>) -> Result<Resource<Fields>> {
        unsupported("incoming-request")
    }
    fn consume(&mut self, _: Resource<Unsupported>) -> Result<Result<Resource<IncomingBody>, ()>> {
        unsupported("incoming-request")
    }
    fn drop(&mut self, rep: Resource<Unsupported>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl<P: Platform> ht::HostOutgoingRequest for State<P> {
    fn new(&mut self, headers: Resource<Fields>) -> Result<Resource<OutgoingRequest>> {
        let fields = self.table.delete(headers)?;
        Ok(self.table.push(OutgoingRequest {
            method: ht::Method::Get,
            scheme: None,
            authority: None,
            path_with_query: None,
            headers: fields.entries,
            body: Vec::new(),
            body_taken: false,
        })?)
    }
    fn body(&mut self, r: Resource<OutgoingRequest>) -> Result<Result<Resource<OutgoingBody>, ()>> {
        let rep = r.rep();
        let req = self.table.get_mut(&r)?;
        if req.body_taken {
            return Ok(Err(()));
        }
        req.body_taken = true;
        Ok(Ok(self.table.push(OutgoingBody {
            request: rep,
            stream_taken: false,
        })?))
    }
    fn method(&mut self, r: Resource<OutgoingRequest>) -> Result<ht::Method> {
        Ok(self.table.get(&r)?.method.clone())
    }
    fn set_method(
        &mut self,
        r: Resource<OutgoingRequest>,
        m: ht::Method,
    ) -> Result<Result<(), ()>> {
        self.table.get_mut(&r)?.method = m;
        Ok(Ok(()))
    }
    fn path_with_query(&mut self, r: Resource<OutgoingRequest>) -> Result<Option<String>> {
        Ok(self.table.get(&r)?.path_with_query.clone())
    }
    fn set_path_with_query(
        &mut self,
        r: Resource<OutgoingRequest>,
        p: Option<String>,
    ) -> Result<Result<(), ()>> {
        self.table.get_mut(&r)?.path_with_query = p;
        Ok(Ok(()))
    }
    fn scheme(&mut self, r: Resource<OutgoingRequest>) -> Result<Option<ht::Scheme>> {
        Ok(self.table.get(&r)?.scheme.clone())
    }
    fn set_scheme(
        &mut self,
        r: Resource<OutgoingRequest>,
        s: Option<ht::Scheme>,
    ) -> Result<Result<(), ()>> {
        self.table.get_mut(&r)?.scheme = s;
        Ok(Ok(()))
    }
    fn authority(&mut self, r: Resource<OutgoingRequest>) -> Result<Option<String>> {
        Ok(self.table.get(&r)?.authority.clone())
    }
    fn set_authority(
        &mut self,
        r: Resource<OutgoingRequest>,
        a: Option<String>,
    ) -> Result<Result<(), ()>> {
        self.table.get_mut(&r)?.authority = a;
        Ok(Ok(()))
    }
    fn headers(&mut self, r: Resource<OutgoingRequest>) -> Result<Resource<Fields>> {
        // 本来は子リソースだが、ここでは複製を immutable で返す
        let entries = self.table.get(&r)?.headers.clone();
        Ok(self.table.push(Fields {
            entries,
            immutable: true,
        })?)
    }
    fn drop(&mut self, rep: Resource<OutgoingRequest>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl<P: Platform> ht::HostRequestOptions for State<P> {
    fn new(&mut self) -> Result<Resource<RequestOptions>> {
        Ok(self.table.push(RequestOptions)?)
    }
    fn connect_timeout(&mut self, _: Resource<RequestOptions>) -> Result<Option<u64>> {
        Ok(None)
    }
    fn set_connect_timeout(
        &mut self,
        _: Resource<RequestOptions>,
        _: Option<u64>,
    ) -> Result<Result<(), ()>> {
        Ok(Ok(()))
    }
    fn first_byte_timeout(&mut self, _: Resource<RequestOptions>) -> Result<Option<u64>> {
        Ok(None)
    }
    fn set_first_byte_timeout(
        &mut self,
        _: Resource<RequestOptions>,
        _: Option<u64>,
    ) -> Result<Result<(), ()>> {
        Ok(Ok(()))
    }
    fn between_bytes_timeout(&mut self, _: Resource<RequestOptions>) -> Result<Option<u64>> {
        Ok(None)
    }
    fn set_between_bytes_timeout(
        &mut self,
        _: Resource<RequestOptions>,
        _: Option<u64>,
    ) -> Result<Result<(), ()>> {
        Ok(Ok(()))
    }
    fn drop(&mut self, rep: Resource<RequestOptions>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl<P: Platform> ht::HostResponseOutparam for State<P> {
    fn set(
        &mut self,
        _: Resource<Unsupported>,
        _: Result<Resource<Unsupported>, ht::ErrorCode>,
    ) -> Result<()> {
        unsupported("response-outparam")
    }
    fn drop(&mut self, rep: Resource<Unsupported>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl<P: Platform> ht::HostIncomingResponse for State<P> {
    fn status(&mut self, r: Resource<IncomingResponse>) -> Result<u16> {
        Ok(self.table.get(&r)?.status)
    }
    fn headers(&mut self, r: Resource<IncomingResponse>) -> Result<Resource<Fields>> {
        let entries = self.table.get(&r)?.headers.clone();
        Ok(self.table.push(Fields {
            entries,
            immutable: true,
        })?)
    }
    fn consume(
        &mut self,
        r: Resource<IncomingResponse>,
    ) -> Result<Result<Resource<IncomingBody>, ()>> {
        match self.table.get_mut(&r)?.body.take() {
            Some(data) => Ok(Ok(self.table.push(IncomingBody { data: Some(data) })?)),
            None => Ok(Err(())),
        }
    }
    fn drop(&mut self, rep: Resource<IncomingResponse>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl<P: Platform> ht::HostIncomingBody for State<P> {
    fn stream(&mut self, b: Resource<IncomingBody>) -> Result<Result<Resource<InputStream>, ()>> {
        match self.table.get_mut(&b)?.data.take() {
            Some(data) => Ok(Ok(self.table.push(InputStream { data, pos: 0 })?)),
            None => Ok(Err(())),
        }
    }
    fn finish(&mut self, b: Resource<IncomingBody>) -> Result<Resource<FutureTrailers>> {
        self.table.delete(b)?;
        Ok(self.table.push(FutureTrailers)?)
    }
    fn drop(&mut self, rep: Resource<IncomingBody>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl<P: Platform> ht::HostFutureTrailers for State<P> {
    fn subscribe(&mut self, _: Resource<FutureTrailers>) -> Result<Resource<Pollable>> {
        Ok(self.table.push(Pollable::Ready)?)
    }
    #[allow(clippy::type_complexity)]
    fn get(
        &mut self,
        _: Resource<FutureTrailers>,
    ) -> Result<Option<Result<Result<Option<Resource<Fields>>, ht::ErrorCode>, ()>>> {
        Ok(Some(Ok(Ok(None))))
    }
    fn drop(&mut self, rep: Resource<FutureTrailers>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl<P: Platform> ht::HostOutgoingResponse for State<P> {
    fn new(&mut self, _: Resource<Fields>) -> Result<Resource<Unsupported>> {
        unsupported("outgoing-response")
    }
    fn status_code(&mut self, _: Resource<Unsupported>) -> Result<u16> {
        unsupported("outgoing-response")
    }
    fn set_status_code(&mut self, _: Resource<Unsupported>, _: u16) -> Result<Result<(), ()>> {
        unsupported("outgoing-response")
    }
    fn headers(&mut self, _: Resource<Unsupported>) -> Result<Resource<Fields>> {
        unsupported("outgoing-response")
    }
    fn body(&mut self, _: Resource<Unsupported>) -> Result<Result<Resource<OutgoingBody>, ()>> {
        unsupported("outgoing-response")
    }
    fn drop(&mut self, rep: Resource<Unsupported>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl<P: Platform> ht::HostOutgoingBody for State<P> {
    fn write(&mut self, b: Resource<OutgoingBody>) -> Result<Result<Resource<OutputStream>, ()>> {
        let body = self.table.get_mut(&b)?;
        if body.stream_taken {
            return Ok(Err(()));
        }
        body.stream_taken = true;
        let request = body.request;
        Ok(Ok(self.table.push(OutputStream { request })?))
    }
    fn finish(
        &mut self,
        b: Resource<OutgoingBody>,
        _trailers: Option<Resource<Fields>>,
    ) -> Result<Result<(), ht::ErrorCode>> {
        self.table.delete(b)?;
        Ok(Ok(()))
    }
    fn drop(&mut self, rep: Resource<OutgoingBody>) -> Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl<P: Platform> ht::HostFutureIncomingResponse for State<P> {
    fn subscribe(&mut self, f: Resource<FutureIncomingResponse>) -> Result<Resource<Pollable>> {
        let h = self.table.get(&f)?.handle;
        Ok(self.table.push(Pollable::Http(h))?)
    }
    #[allow(clippy::type_complexity)]
    fn get(
        &mut self,
        f: Resource<FutureIncomingResponse>,
    ) -> Result<Option<Result<Result<Resource<IncomingResponse>, ht::ErrorCode>, ()>>> {
        let fut = self.table.get(&f)?;
        if fut.taken {
            return Ok(Some(Err(())));
        }
        let h = fut.handle;
        let Some(result) = self.p.http_take(h) else {
            return Ok(None);
        };
        self.table.get_mut(&f)?.taken = true;
        self.pending_http.retain(|p| p.0 != h);
        Ok(Some(Ok(match result {
            Ok(resp) => Ok(self.table.push(IncomingResponse {
                status: resp.status,
                headers: resp.headers,
                body: Some(resp.body),
            })?),
            Err(msg) => Err(ht::ErrorCode::InternalError(Some(msg))),
        })))
    }
    fn drop(&mut self, rep: Resource<FutureIncomingResponse>) -> Result<()> {
        let fut = self.table.delete(rep)?;
        if !fut.taken {
            self.p.http_cancel(fut.handle);
            self.pending_http.retain(|p| p.0 != fut.handle);
        }
        Ok(())
    }
}

impl<P: Platform> ht::Host for State<P> {
    fn http_error_code(&mut self, _err: Resource<IoError>) -> Result<Option<ht::ErrorCode>> {
        Ok(None)
    }
}

impl<P: Platform> outgoing_handler::Host for State<P> {
    fn handle(
        &mut self,
        request: Resource<OutgoingRequest>,
        _options: Option<Resource<RequestOptions>>,
    ) -> Result<Result<Resource<FutureIncomingResponse>, ht::ErrorCode>> {
        let req = self.table.delete(request)?;
        let scheme = match &req.scheme {
            None | Some(ht::Scheme::Https) => "https".to_string(),
            Some(ht::Scheme::Http) => "http".to_string(),
            Some(ht::Scheme::Other(s)) => s.clone(),
        };
        let Some(authority) = req.authority.clone() else {
            return Ok(Err(ht::ErrorCode::HttpRequestUriInvalid));
        };
        let url = alloc::format!(
            "{scheme}://{authority}{}",
            req.path_with_query.as_deref().unwrap_or("/")
        );
        let method = method_name(&req.method);
        self.log(&alloc::format!("http: {method} {url}"));
        let handle = self.p.http_start(HttpRequest {
            method,
            url,
            headers: req.headers,
            body: req.body,
        });
        self.pending_http.push((handle, false));
        Ok(Ok(self.table.push(FutureIncomingResponse {
            handle,
            taken: false,
        })?))
    }
}
