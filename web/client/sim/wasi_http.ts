/**
 * wasi:http@0.2.3 の outgoing 側を fetch() で実装する（シミュレータ用）。
 *
 * ゲストは非ブロッキングに使う前提（`future.get()` をポーリング）。fetch が終わったら
 * `onProgress` を呼び、ホストがアプリに `event.io` を送る。ボディは全部読んでから
 * future を完了させるので、stream.read は常に即座に返る。
 *
 * jco の約束: result の err は `{ payload }` を持つ値を throw する。variant は `{ tag, val }`。
 */

type Tagged = { tag: string; val?: unknown };

function fail(payload: unknown): never {
  throw { payload };
}

class IoError {
  constructor(private message: string) {}
  toDebugString(): string {
    return this.message;
  }
}

export class InputStream {
  #chunks: Uint8Array;
  #pos = 0;
  constructor(bytes: Uint8Array) {
    this.#chunks = bytes;
  }
  read(len: bigint): Uint8Array {
    if (this.#pos >= this.#chunks.length) fail({ tag: "closed" });
    const end = Math.min(this.#chunks.length, this.#pos + Number(len));
    const out = this.#chunks.slice(this.#pos, end);
    this.#pos = end;
    return out;
  }
  blockingRead(len: bigint): Uint8Array {
    return this.read(len);
  }
  skip(len: bigint): bigint {
    return BigInt(this.read(len).length);
  }
  blockingSkip(len: bigint): bigint {
    return this.skip(len);
  }
  subscribe(): Pollable {
    return new Pollable(() => true);
  }
}

export class OutputStream {
  constructor(private sink: Uint8Array[]) {}
  checkWrite(): bigint {
    return 1n << 20n;
  }
  write(contents: Uint8Array): void {
    this.sink.push(contents.slice());
  }
  blockingWriteAndFlush(contents: Uint8Array): void {
    this.write(contents);
  }
  flush(): void {}
  blockingFlush(): void {}
  subscribe(): Pollable {
    return new Pollable(() => true);
  }
}

export class Pollable {
  constructor(private isReady: () => boolean) {}
  ready(): boolean {
    return this.isReady();
  }
  block(): void {
    // ブラウザのメインスレッドでは待てない。ゲストは block を使わないこと
    if (!this.isReady()) {
      throw new Error("wasi:io/poll block() is not supported in the simulator");
    }
  }
}

export class Fields {
  #entries: [string, Uint8Array][] = [];
  static fromList(entries: [string, Uint8Array][]): Fields {
    const f = new Fields();
    f.#entries = entries.map(([k, v]) => [k.toLowerCase(), v]);
    return f;
  }
  get(name: string): Uint8Array[] {
    const n = name.toLowerCase();
    return this.#entries.filter(([k]) => k === n).map(([, v]) => v);
  }
  has(name: string): boolean {
    return this.get(name).length > 0;
  }
  set(name: string, values: Uint8Array[]): void {
    this.delete(name);
    for (const v of values) this.append(name, v);
  }
  delete(name: string): void {
    const n = name.toLowerCase();
    this.#entries = this.#entries.filter(([k]) => k !== n);
  }
  append(name: string, value: Uint8Array): void {
    this.#entries.push([name.toLowerCase(), value]);
  }
  entries(): [string, Uint8Array][] {
    return this.#entries.map(([k, v]) => [k, v]);
  }
  clone(): Fields {
    return Fields.fromList(this.entries());
  }
}

export class OutgoingBody {
  chunks: Uint8Array[] = [];
  finished = false;
  #taken = false;
  write(): OutputStream {
    if (this.#taken) fail(undefined);
    this.#taken = true;
    return new OutputStream(this.chunks);
  }
  static finish(body: OutgoingBody, _trailers: Fields | undefined): void {
    body.finished = true;
  }
}

export class OutgoingRequest {
  method: Tagged = { tag: "get" };
  scheme: Tagged | undefined = { tag: "https" };
  authority: string | undefined;
  pathWithQuery: string | undefined;
  #body = new OutgoingBody();
  #bodyTaken = false;
  constructor(public headersFields: Fields) {}
  body(): OutgoingBody {
    if (this.#bodyTaken) fail(undefined);
    this.#bodyTaken = true;
    return this.#body;
  }
  get outgoingBody(): OutgoingBody {
    return this.#body;
  }
  getMethod() {
    return this.method;
  }
  setMethod(m: Tagged) {
    this.method = m;
  }
  getPathWithQuery() {
    return this.pathWithQuery;
  }
  setPathWithQuery(p: string | undefined) {
    this.pathWithQuery = p;
  }
  getScheme() {
    return this.scheme;
  }
  setScheme(s: Tagged | undefined) {
    this.scheme = s;
  }
  getAuthority() {
    return this.authority;
  }
  setAuthority(a: string | undefined) {
    this.authority = a;
  }
  headers(): Fields {
    return this.headersFields;
  }
}

export class RequestOptions {
  connectTimeout() {
    return undefined;
  }
  setConnectTimeout() {}
  firstByteTimeout() {
    return undefined;
  }
  setFirstByteTimeout() {}
  betweenBytesTimeout() {
    return undefined;
  }
  setBetweenBytesTimeout() {}
}

export class IncomingBody {
  #streamTaken = false;
  constructor(private bytes: Uint8Array) {}
  stream(): InputStream {
    if (this.#streamTaken) fail(undefined);
    this.#streamTaken = true;
    return new InputStream(this.bytes);
  }
  static finish(_body: IncomingBody): FutureTrailers {
    return new FutureTrailers();
  }
}

export class FutureTrailers {
  subscribe(): Pollable {
    return new Pollable(() => true);
  }
  get() {
    return { tag: "ok", val: { tag: "ok", val: undefined } };
  }
}

export class IncomingResponse {
  #consumed = false;
  constructor(
    private statusCode: number,
    private fields: Fields,
    private bytes: Uint8Array,
  ) {}
  status(): number {
    return this.statusCode;
  }
  headers(): Fields {
    return this.fields;
  }
  consume(): IncomingBody {
    if (this.#consumed) fail(undefined);
    this.#consumed = true;
    return new IncomingBody(this.bytes);
  }
}

export class FutureIncomingResponse {
  result: { tag: "ok" | "err"; val: unknown } | undefined;
  #taken = false;
  subscribe(): Pollable {
    return new Pollable(() => this.result !== undefined);
  }
  get() {
    if (this.result === undefined) return undefined;
    if (this.#taken) return { tag: "err", val: undefined };
    this.#taken = true;
    return { tag: "ok", val: this.result };
  }
}

const methodName = (m: Tagged) =>
  m.tag === "other" ? String(m.val) : m.tag.toUpperCase();

/** 1 アプリ分の wasi:http / wasi:io の import オブジェクトを作る */
export function createWasiHttp(
  onProgress: () => void,
  log: (m: string) => void,
) {
  const controllers = new Set<AbortController>();

  function handle(
    req: OutgoingRequest,
    _options: RequestOptions | undefined,
  ): FutureIncomingResponse {
    const scheme = req.scheme?.tag === "other"
      ? String(req.scheme.val)
      : (req.scheme?.tag ?? "https").toLowerCase();
    const url = `${scheme}://${req.authority ?? ""}${req.pathWithQuery ?? "/"}`;
    const headers = new Headers();
    const decoder = new TextDecoder();
    for (const [k, v] of req.headersFields.entries()) {
      headers.append(k, decoder.decode(v));
    }
    const chunks = req.outgoingBody.chunks;
    const body = chunks.length ? new Blob(chunks as BlobPart[]) : undefined;

    const future = new FutureIncomingResponse();
    const controller = new AbortController();
    controllers.add(controller);
    log(`http: ${methodName(req.method)} ${url}`);

    fetch(url, {
      method: methodName(req.method),
      headers,
      body,
      signal: controller.signal,
    })
      .then(async (res) => {
        const bytes = new Uint8Array(await res.arrayBuffer());
        const encoder = new TextEncoder();
        const fields = Fields.fromList(
          [...res.headers].map(([k, v]) => [k, encoder.encode(v)]),
        );
        future.result = {
          tag: "ok",
          val: new IncomingResponse(res.status, fields, bytes),
        };
      })
      .catch((e: unknown) => {
        // CORS 拒否もここに来る（ブラウザは理由を教えてくれない）
        log(`http: ${url} failed: ${e}`);
        future.result = {
          tag: "err",
          val: { tag: "internal-error", val: `fetch failed (CORS?): ${e}` },
        };
      })
      .finally(() => {
        controllers.delete(controller);
        if (!controller.signal.aborted) onProgress();
      });

    return future;
  }

  const imports = {
    "wasi:http/outgoing-handler": { handle },
    "wasi:http/types": {
      Fields,
      FutureIncomingResponse,
      FutureTrailers,
      IncomingBody,
      IncomingResponse,
      OutgoingBody,
      OutgoingRequest,
      RequestOptions,
      // 以下はサーバ側（incoming-request 等）で、アプリでは使わない
    },
    "wasi:io/error": { Error: IoError },
    "wasi:io/streams": { InputStream, OutputStream },
    "wasi:io/poll": {
      Pollable,
      poll(list: Pollable[]): Uint32Array {
        const ready = list.flatMap((p, i) => (p.ready() ? [i] : []));
        if (!ready.length) {
          throw new Error("wasi:io/poll poll() would block in the simulator");
        }
        return new Uint32Array(ready);
      },
    },
    "wasi:clocks/monotonic-clock": {
      now: () => BigInt(Math.round(performance.now() * 1e6)),
      resolution: () => 1000n,
      subscribeInstant: () => new Pollable(() => true),
      subscribeDuration: () => new Pollable(() => true),
    },
  };

  return {
    imports,
    abortAll() {
      for (const c of controllers) c.abort();
      controllers.clear();
    },
  };
}
