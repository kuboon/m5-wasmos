/**
 * ブラウザ上の m5-wasmos ホスト。実機ファームウェアと同じ WIT（wit/wasmos.wit）を
 * canvas とポインタイベントで実装し、jco で JS に変換したアプリを動かす。
 *
 * - 前面のアプリは常に 1 つ。launch / exit はコールバックが返ってから切り替える
 * - 画面上端からの下スワイプはシステムジェスチャ（ランチャーへ戻る）でアプリには渡さない
 * - アプリが trap したらログを出してランチャーへ戻る
 */

import { createWasiHttp } from "./wasi_http.ts";

export const SCREEN_W = 720;
export const SCREEN_H = 1280;
/** システムジェスチャの開始帯（px）と、戻るのに必要な下方向の移動量 */
const GESTURE_EDGE = 24;
const GESTURE_DISTANCE = 160;

export interface ManifestApp {
  id: string;
  name: string;
  version: string;
  description: string;
  color: string;
  hidden: boolean;
  wasm: string;
  sha256: string;
  size: number;
  sim: string;
}

export interface Manifest {
  abi: string;
  launcher: string;
  generatedAt: string;
  apps: ManifestApp[];
}

type Tagged<T = unknown> = { tag: string; val: T };
type Rect = { x: number; y: number; w: number; h: number };
type TouchPhase = "down" | "move" | "up" | "cancel";
type Touch = { id: number; phase: TouchPhase; x: number; y: number };
type AppEvent =
  | { tag: "touch"; val: Touch[] }
  | { tag: "frame"; val: bigint }
  | { tag: "timer"; val: number }
  | { tag: "io" }
  | { tag: "image-ready"; val: number };

interface AppExports {
  init(args: string): void;
  onEvent(event: AppEvent): void;
}

type JcoModule = {
  instantiate(
    getCoreModule: (path: string) => Promise<WebAssembly.Module>,
    imports: Record<string, unknown>,
  ): Promise<AppExports>;
};

export interface HostOptions {
  canvas: HTMLCanvasElement;
  manifestUrl: string;
  log?: (line: string, level?: "info" | "error") => void;
  onAppChange?: (app: ManifestApp) => void;
}

const css = (c: number) =>
  `rgba(${(c >>> 16) & 255},${(c >>> 8) & 255},${c & 255},${
    ((c >>> 24) & 255) / 255
  })`;

const parseColor = (hex: string) =>
  (0xff000000 | parseInt(hex.replace("#", ""), 16)) >>> 0;

const FONT =
  `"Hiragino Kaku Gothic ProN", "Hiragino Sans", "Noto Sans JP", system-ui, sans-serif`;

/** 1 回の起動ぶんの状態。アプリを切り替えたら丸ごと捨てる */
class Session {
  exports: AppExports | null = null;
  images = new Map<
    number,
    CanvasImageSource & { width: number; height: number }
  >();
  nextImage = 1;
  timers = new Map<number, number>();
  nextTimer = 1;
  frameRequested = false;
  alive = true;
  http: ReturnType<typeof createWasiHttp>;

  constructor(
    public app: ManifestApp,
    onIo: () => void,
    log: (m: string) => void,
  ) {
    this.http = createWasiHttp(onIo, log);
  }

  dispose() {
    this.alive = false;
    for (const t of this.timers.values()) clearTimeout(t);
    this.timers.clear();
    this.images.clear();
    this.http.abortAll();
  }
}

export class Host {
  readonly canvas: HTMLCanvasElement;
  readonly ctx: CanvasRenderingContext2D;
  manifest!: Manifest;
  #manifestUrl: string;
  #log: NonNullable<HostOptions["log"]>;
  #onAppChange?: HostOptions["onAppChange"];
  #session: Session | null = null;
  #pending: { id: string; args: string } | null = null;
  #dispatching = false;
  #queue: AppEvent[] = [];

  // pointerId → touch id（0..）
  #pointerIds = new Map<number, number>();
  #gesture = new Map<number, number>(); // pointerId → 開始 y
  #pendingTouches: Touch[] = [];
  #touchFlushScheduled = false;

  constructor(opts: HostOptions) {
    this.canvas = opts.canvas;
    this.canvas.width = SCREEN_W;
    this.canvas.height = SCREEN_H;
    this.ctx = this.canvas.getContext("2d")!;
    this.#manifestUrl = new URL(opts.manifestUrl, location.href).href;
    this.#log = opts.log ?? ((l) => console.log(l));
    this.#onAppChange = opts.onAppChange;
    this.#bindPointer();
  }

  async boot(initial?: string) {
    const res = await fetch(this.#manifestUrl, { cache: "no-cache" });
    this.manifest = await res.json();
    this.#log(
      `manifest: ${this.manifest.apps.length} apps (abi ${this.manifest.abi})`,
    );
    const id = initial && this.#find(initial)
      ? initial
      : this.manifest.launcher;
    await this.#start(id, "");
  }

  /** ホームボタン / システムジェスチャ */
  home() {
    if (this.#session?.app.id === this.manifest.launcher) return;
    this.#log("system: home");
    this.#switch(this.manifest.launcher, "");
  }

  get currentApp(): ManifestApp | undefined {
    return this.#session?.app;
  }

  #find(id: string) {
    return this.manifest.apps.find((a) => a.id === id);
  }

  #switch(id: string, args: string) {
    this.#pending = { id, args };
    if (!this.#dispatching) this.#applyPending();
  }

  #applyPending() {
    const next = this.#pending;
    if (!next) return;
    this.#pending = null;
    queueMicrotask(() => this.#start(next.id, next.args));
  }

  async #start(id: string, args: string): Promise<void> {
    const app = this.#find(id);
    if (!app) {
      this.#log(`launch: unknown app "${id}"`, "error");
      if (id !== this.manifest.launcher) {
        return this.#start(this.manifest.launcher, "");
      }
      return;
    }
    this.#session?.dispose();
    this.#queue = [];
    this.#pointerIds.clear();

    const session = new Session(
      app,
      () => this.#post(session, { tag: "io" }),
      (m) => this.#log(`[${app.id}] ${m}`),
    );
    this.#session = session;
    this.#onAppChange?.(app);
    this.ctx.reset();
    this.ctx.fillStyle = "#000";
    this.ctx.fillRect(0, 0, SCREEN_W, SCREEN_H);
    this.#log(`launch: ${app.id} ${app.version}`);

    try {
      const moduleUrl = new URL(app.sim, this.#manifestUrl).href;
      const mod = (await import(moduleUrl)) as JcoModule;
      const exports = await mod.instantiate(
        async (path) => {
          const r = await fetch(new URL(path, moduleUrl));
          return WebAssembly.compile(await r.arrayBuffer());
        },
        this.#imports(session),
      );
      if (!session.alive) return;
      session.exports = exports;
      this.#call(session, () => exports.init(args));
    } catch (e) {
      this.#crash(session, e);
    }
  }

  #crash(session: Session, e: unknown) {
    this.#log(`[${session.app.id}] trap: ${e}`, "error");
    console.error(e);
    if (
      session.app.id !== this.manifest.launcher && this.#session === session
    ) {
      this.#switch(this.manifest.launcher, "");
    }
  }

  #call(session: Session, f: () => void) {
    if (!session.alive) return;
    this.#dispatching = true;
    try {
      f();
    } catch (e) {
      this.#crash(session, e);
    } finally {
      this.#dispatching = false;
    }
    if (this.#pending) this.#applyPending();
  }

  #post(session: Session, event: AppEvent) {
    if (!session.alive || this.#session !== session) return;
    this.#queue.push(event);
    if (this.#queue.length === 1) queueMicrotask(() => this.#drain(session));
  }

  #drain(session: Session) {
    while (this.#queue.length && session.alive && session.exports) {
      const event = this.#queue.shift()!;
      this.#call(session, () => session.exports!.onEvent(event));
    }
  }

  // --- imports ---------------------------------------------------------------

  #imports(s: Session) {
    const ctx = this.ctx;
    const log = (m: string) => this.#log(`[${s.app.id}] ${m}`);

    const gfx = {
      screenSize: () => [SCREEN_W, SCREEN_H],
      measureText: (text: string, size: number) => {
        ctx.font = `${size}px ${FONT}`;
        return Math.ceil(ctx.measureText(text).width);
      },
      loadImage: (data: Uint8Array) => {
        const id = s.nextImage++;
        createImageBitmap(new Blob([data.slice()]))
          .then((bmp) => {
            if (s.alive) s.images.set(id, bmp);
          })
          .catch((e) => log(`load-image ${id}: ${e}`))
          .finally(() => this.#post(s, { tag: "image-ready", val: id }));
        return id;
      },
      imageFromRgba: (w: number, h: number, px: Uint8Array) => {
        const id = s.nextImage++;
        const c = new OffscreenCanvas(w, h);
        c.getContext("2d")!.putImageData(
          new ImageData(new Uint8ClampedArray(px), w, h),
          0,
          0,
        );
        s.images.set(id, c);
        return id;
      },
      imageSize: (id: number) => {
        const img = s.images.get(id);
        return img ? [img.width, img.height] : undefined;
      },
      dropImage: (id: number) => {
        s.images.delete(id);
      },
      submit: (cmds: Tagged<never>[]) => {
        if (this.#session === s) this.#render(s, cmds);
      },
    };

    const system = {
      log,
      nowMs: () => BigInt(Math.floor(performance.now())),
      wallMs: () => BigInt(Date.now()),
      // ブラウザには電池の電圧・電流が無い
      battery: () => undefined,
      requestFrame: () => {
        if (s.frameRequested) return;
        s.frameRequested = true;
        requestAnimationFrame((t) => {
          s.frameRequested = false;
          this.#post(s, { tag: "frame", val: BigInt(Math.floor(t)) });
        });
      },
      setTimer: (ms: number) => {
        const id = s.nextTimer++;
        s.timers.set(
          id,
          setTimeout(() => {
            s.timers.delete(id);
            this.#post(s, { tag: "timer", val: id });
          }, ms),
        );
        return id;
      },
      cancelTimer: (id: number) => {
        clearTimeout(s.timers.get(id));
        s.timers.delete(id);
      },
      installed: () =>
        this.manifest.apps
          .filter((a) => !a.hidden)
          .map((a) => ({
            id: a.id,
            name: a.name,
            version: a.version,
            description: a.description,
            color: parseColor(a.color),
          })),
      launch: (id: string, args: string) => {
        log(`launch ${id}`);
        this.#switch(id, args);
      },
      exit: () => this.#switch(this.manifest.launcher, ""),
      // シミュレータでは manifest を読み直してランチャーから起動し直す
      update: () => {
        log("update");
        setTimeout(() => this.boot().catch((e) => log(`update: ${e}`)), 0);
      },
    };

    // localStorage にアプリごとに置く（キーの検査と上限は実機の host と同じ）
    const storageKey = (key: string) => {
      if (!/^[a-z0-9_-]{1,16}$/.test(key)) return null;
      return `wasmos:${s.app.id}:${key}`;
    };
    const storage = {
      get: (key: string) => {
        const k = storageKey(key);
        const v = k ? localStorage.getItem(k) : null;
        return v ? Uint8Array.fromBase64(v) : undefined;
      },
      set: (key: string, value: Uint8Array) => {
        const k = storageKey(key);
        if (!k) throw { payload: `invalid key: ${key}` };
        if (value.length > 256 * 1024) {
          throw { payload: `too large: ${value.length} bytes` };
        }
        try {
          if (value.length === 0) localStorage.removeItem(k);
          else localStorage.setItem(k, value.toBase64());
        } catch (e) {
          throw { payload: String(e) };
        }
      },
    };

    const all: Record<string, unknown> = {
      ...s.http.imports,
      "wasmos:os/types": {},
      "wasmos:os/gfx": gfx,
      "wasmos:os/system": system,
      "wasmos:os/storage": storage,
    };
    // jco はバージョン付き / 無しのどちらの名前でも引くことがあるので両方置く
    for (const [k, v] of Object.entries({ ...all })) {
      all[`${k}@${k.startsWith("wasmos:") ? "0.4.0" : "0.2.3"}`] = v;
    }
    return all;
  }

  // --- 描画 ------------------------------------------------------------------

  #render(s: Session, cmds: Tagged<never>[]) {
    const ctx = this.ctx;
    let clips = 0;
    for (const cmd of cmds) {
      const v = cmd.val as Record<string, never>;
      switch (cmd.tag) {
        case "clear":
          ctx.fillStyle = css(cmd.val);
          ctx.fillRect(0, 0, SCREEN_W, SCREEN_H);
          break;
        case "fill-rect": {
          const r = v.rect as Rect;
          ctx.fillStyle = css(v.color);
          ctx.beginPath();
          ctx.roundRect(r.x, r.y, r.w, r.h, v.radius);
          ctx.fill();
          break;
        }
        case "stroke-rect": {
          const r = v.rect as Rect;
          const w = v.width as number;
          ctx.strokeStyle = css(v.color);
          ctx.lineWidth = w;
          ctx.beginPath();
          ctx.roundRect(r.x + w / 2, r.y + w / 2, r.w - w, r.h - w, v.radius);
          ctx.stroke();
          break;
        }
        case "line":
          ctx.strokeStyle = css(v.color);
          ctx.lineWidth = v.width;
          ctx.lineCap = "round";
          ctx.beginPath();
          ctx.moveTo(v.x0, v.y0);
          ctx.lineTo(v.x1, v.y1);
          ctx.stroke();
          break;
        case "fill-circle":
          ctx.fillStyle = css(v.color);
          ctx.beginPath();
          ctx.arc(v.x, v.y, v.r, 0, Math.PI * 2);
          ctx.fill();
          break;
        case "text":
          ctx.fillStyle = css(v.color);
          ctx.font = `${v.size}px ${FONT}`;
          ctx.textBaseline = "top";
          ctx.textAlign = v.align;
          ctx.fillText(v.text, v.x, v.y);
          break;
        case "image": {
          const img = s.images.get(v.id);
          if (!img) break;
          const d = v.dst as Rect;
          const src = v.src as Rect | undefined;
          ctx.globalAlpha = (v.alpha as number) / 255;
          if (src) {
            ctx.drawImage(img, src.x, src.y, src.w, src.h, d.x, d.y, d.w, d.h);
          } else ctx.drawImage(img, d.x, d.y, d.w, d.h);
          ctx.globalAlpha = 1;
          break;
        }
        case "push-clip": {
          const r = cmd.val as Rect;
          ctx.save();
          ctx.beginPath();
          ctx.rect(r.x, r.y, r.w, r.h);
          ctx.clip();
          clips++;
          break;
        }
        case "pop-clip":
          if (clips > 0) {
            ctx.restore();
            clips--;
          }
          break;
      }
    }
    // 閉じ忘れの clip は次のフレームに持ち越さない
    while (clips-- > 0) ctx.restore();
  }

  // --- 入力 ------------------------------------------------------------------

  #bindPointer() {
    const c = this.canvas;
    c.style.touchAction = "none";
    const pos = (e: PointerEvent) => {
      const r = c.getBoundingClientRect();
      return {
        x: Math.round(((e.clientX - r.left) / r.width) * SCREEN_W),
        y: Math.round(((e.clientY - r.top) / r.height) * SCREEN_H),
      };
    };

    c.addEventListener("pointerdown", (e) => {
      c.setPointerCapture(e.pointerId);
      const { x, y } = pos(e);
      if (y < GESTURE_EDGE) {
        this.#gesture.set(e.pointerId, y);
        return;
      }
      const used = new Set(this.#pointerIds.values());
      let id = 0;
      while (used.has(id)) id++;
      if (id > 9) return; // GT911 は 5 点だが余裕を見る
      this.#pointerIds.set(e.pointerId, id);
      this.#touch({ id, phase: "down", x, y });
    });
    c.addEventListener("pointermove", (e) => {
      const { x, y } = pos(e);
      const start = this.#gesture.get(e.pointerId);
      if (start !== undefined) {
        if (y - start > GESTURE_DISTANCE) {
          this.#gesture.delete(e.pointerId);
          this.home();
        }
        return;
      }
      const id = this.#pointerIds.get(e.pointerId);
      if (id !== undefined) this.#touch({ id, phase: "move", x, y });
    });
    const end = (phase: TouchPhase) => (e: PointerEvent) => {
      this.#gesture.delete(e.pointerId);
      const id = this.#pointerIds.get(e.pointerId);
      if (id === undefined) return;
      this.#pointerIds.delete(e.pointerId);
      const { x, y } = pos(e);
      this.#touch({ id, phase, x, y });
    };
    c.addEventListener("pointerup", end("up"));
    c.addEventListener("pointercancel", end("cancel"));
  }

  /** 1 フレームぶんまとめて送る（move の洪水でホスト呼び出しが増えないように） */
  #touch(t: Touch) {
    const last = this.#pendingTouches.at(-1);
    if (
      last && last.id === t.id && last.phase === "move" && t.phase === "move"
    ) {
      this.#pendingTouches[this.#pendingTouches.length - 1] = t;
    } else {
      this.#pendingTouches.push(t);
    }
    if (this.#touchFlushScheduled) return;
    this.#touchFlushScheduled = true;
    requestAnimationFrame(() => {
      this.#touchFlushScheduled = false;
      const touches = this.#pendingTouches;
      this.#pendingTouches = [];
      if (this.#session && touches.length) {
        this.#post(this.#session, { tag: "touch", val: touches });
      }
    });
  }
}
