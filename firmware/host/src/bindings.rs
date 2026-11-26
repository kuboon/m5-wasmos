//! wit/wasmos.wit の app world をホスト側から見たバインディング
wasmtime::component::bindgen!({
    path: "../../wit",
    world: "wasmos:os/app",
    // 未対応の関数は Err を返して trap させたいので全部 trappable にする
    imports: { default: trappable },
    with: {
        "wasi:http/types.fields": crate::http::Fields,
        "wasi:http/types.outgoing-request": crate::http::OutgoingRequest,
        "wasi:http/types.outgoing-body": crate::http::OutgoingBody,
        "wasi:http/types.request-options": crate::http::RequestOptions,
        "wasi:http/types.future-incoming-response": crate::http::FutureIncomingResponse,
        "wasi:http/types.incoming-response": crate::http::IncomingResponse,
        "wasi:http/types.incoming-body": crate::http::IncomingBody,
        "wasi:http/types.future-trailers": crate::http::FutureTrailers,
        "wasi:http/types.incoming-request": crate::http::Unsupported,
        "wasi:http/types.outgoing-response": crate::http::Unsupported,
        "wasi:http/types.response-outparam": crate::http::Unsupported,
        "wasi:io/streams.input-stream": crate::http::InputStream,
        "wasi:io/streams.output-stream": crate::http::OutputStream,
        "wasi:io/poll.pollable": crate::http::Pollable,
        "wasi:io/error.error": crate::http::IoError,
    },
});
