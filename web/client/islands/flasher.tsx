/**
 * ブラウザから Tab5 に書き込み、USB シリアルで設定する（Web Serial: Chrome / Edge）。
 *
 * 1. 書き込み: esptool-js で `firmware/m5-wasmos.bin`（merge-bin 済み）を 0x0 に書く
 * 2. コンソール: 起動ログを表示し、Wi-Fi（2 か所）の登録や update を送る
 *
 * 2 つは同じシリアルポートを使うので同時には開けない。
 */

import {
  clientEntry,
  css,
  type Handle,
  on,
  type SerializableValue,
} from "@remix-run/component";

import { color, font, radius } from "../tokens.ts";

export interface FlasherProps {
  firmwareUrl: string;
  /** null ならまだ公開されていない */
  info: string | null;
  [key: string]: SerializableValue;
}

// Web Serial の最小限の型（lib.dom に無いので）
interface SerialPortLike {
  open(o: { baudRate: number }): Promise<void>;
  close(): Promise<void>;
  readable: ReadableStream<Uint8Array> | null;
  writable: WritableStream<Uint8Array> | null;
}
type SerialLike = { requestPort(): Promise<SerialPortLike> };
const serial = () => (navigator as unknown as { serial?: SerialLike }).serial;

type Mode = "idle" | "flashing" | "console";

export const Flasher = clientEntry(
  import.meta.url,
  function Flasher(handle: Handle<FlasherProps>) {
    let mode: Mode = "idle";
    let progress = 0;
    let status = "";
    const lines: string[] = [];
    let port: SerialPortLike | null = null;
    let reader: ReadableStreamDefaultReader<Uint8Array> | null = null;
    let slot = "0";
    let ssid = "";
    let pass = "";
    let command = "";

    const log = (text: string) => {
      lines.push(...text.split("\n"));
      if (lines.length > 400) lines.splice(0, lines.length - 400);
      handle.update();
    };

    async function flash() {
      const s = serial();
      if (!s) {
        return log(
          "このブラウザは Web Serial に対応していません（Chrome / Edge を使ってください）",
        );
      }
      await closeConsole();
      mode = "flashing";
      progress = 0;
      status = "ポートを選んでください";
      handle.update();
      try {
        const { ESPLoader, Transport } = await import("esptool-js");
        const p = await s.requestPort();
        // deno-lint-ignore no-explicit-any
        const transport = new Transport(p as any, false);
        const loader = new ESPLoader({
          transport,
          baudrate: 921600,
          terminal: {
            clean() {},
            writeLine: (d: string) => log(d),
            write: (d: string) => log(d),
          },
        });
        status = "接続中…";
        handle.update();
        const chip = await loader.main();
        status = `${chip} に書き込み中…`;
        handle.update();
        const res = await fetch(handle.props.firmwareUrl);
        if (!res.ok) throw new Error(`firmware: HTTP ${res.status}`);
        const data = new Uint8Array(await res.arrayBuffer());
        await loader.writeFlash({
          fileArray: [{ data, address: 0 }],
          flashMode: "keep",
          flashFreq: "keep",
          flashSize: "keep",
          eraseAll: false,
          compress: true,
          reportProgress: (_i: number, written: number, total: number) => {
            progress = Math.round((written / total) * 100);
            handle.update();
          },
        });
        await loader.after("hard_reset");
        await transport.disconnect();
        status =
          "書き込みました。本体が再起動します。下の「コンソールを開く」で Wi-Fi を設定できます";
      } catch (e) {
        status = `失敗しました: ${e}`;
      } finally {
        mode = "idle";
        handle.update();
      }
    }

    async function openConsole() {
      const s = serial();
      if (!s) return log("このブラウザは Web Serial に対応していません");
      try {
        port = await s.requestPort();
        await port.open({ baudRate: 115200 });
        mode = "console";
        status = "コンソールに接続しました";
        handle.update();
        readLoop();
      } catch (e) {
        status = `開けませんでした: ${e}`;
        handle.update();
      }
    }

    async function readLoop() {
      if (!port?.readable) return;
      reader = port.readable.getReader();
      const dec = new TextDecoder();
      let buf = "";
      try {
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          buf += dec.decode(value, { stream: true });
          const parts = buf.split(/\r?\n/);
          buf = parts.pop() ?? "";
          if (parts.length) log(parts.join("\n"));
        }
      } catch (e) {
        log(`(切断: ${e})`);
      } finally {
        reader?.releaseLock();
        reader = null;
      }
    }

    async function closeConsole() {
      try {
        await reader?.cancel();
        await port?.close();
      } catch { /* 既に閉じている */ }
      port = null;
      if (mode === "console") mode = "idle";
      handle.update();
    }

    async function send(line: string) {
      if (!port?.writable) return log("コンソールが開いていません");
      const w = port.writable.getWriter();
      await w.write(new TextEncoder().encode(line + "\n"));
      w.releaseLock();
    }

    const value = (e: Event) => (e.currentTarget as HTMLInputElement).value;

    return () => {
      const { info } = handle.props;
      const busy = mode === "flashing";
      return (
        <div mix={wrapStyle}>
          <section mix={boxStyle}>
            <h2 mix={h2Style}>1. 書き込み</h2>
            {info ? <p mix={mutedStyle}>{info}</p> : (
              <p mix={mutedStyle}>
                まだファームウェアが公開されていません（GitHub Actions の
                firmware が成功すると出ます）
              </p>
            )}
            <p>
              Tab5 を USB-C で PC
              につないで「書き込む」を押し、出てきたポート（USB JTAG/serial
              debug unit）を選びます。 うまく接続できないときは、本体の BOOT
              を押しながら RESET
              を押して書き込みモードにしてから試してください。
            </p>
            <button
              type="button"
              disabled={!info || busy}
              mix={[buttonStyle, primaryStyle, on("click", flash)]}
            >
              書き込む
            </button>
            {busy || progress > 0
              ? (
                <progress mix={progressStyle} max={100} value={progress}>
                </progress>
              )
              : null}
            {status ? <p>{status}</p> : null}
          </section>

          <section mix={boxStyle}>
            <h2 mix={h2Style}>2. Wi-Fi とコンソール</h2>
            <p>
              Wi-Fi は 2 か所まで保存でき、つながる方へ自動で接続します（0
              を先に試します）。
              保存すると本体がつなぎ直し、つながるとアプリの更新を確認します。
            </p>
            <div mix={rowStyle}>
              {mode === "console"
                ? (
                  <button
                    type="button"
                    mix={[buttonStyle, on("click", closeConsole)]}
                  >
                    コンソールを閉じる
                  </button>
                )
                : (
                  <button
                    type="button"
                    disabled={busy}
                    mix={[buttonStyle, on("click", openConsole)]}
                  >
                    コンソールを開く
                  </button>
                )}
              <button
                type="button"
                mix={[buttonStyle, on("click", () => send("wifi"))]}
              >
                Wi-Fi の状態
              </button>
              <button
                type="button"
                mix={[buttonStyle, on("click", () => send("update"))]}
              >
                アプリを更新
              </button>
              <button
                type="button"
                mix={[buttonStyle, on("click", () => send("reboot"))]}
              >
                再起動
              </button>
            </div>
            <form
              mix={[
                formStyle,
                on("submit", (e) => {
                  e.preventDefault();
                  if (!ssid) return;
                  send(["wifi", slot, ssid, pass].join("\t"));
                }),
              ]}
            >
              <label>
                保存先
                <select
                  mix={[
                    inputStyle,
                    on("change", (e) => {
                      slot = value(e);
                    }),
                  ]}
                >
                  <option value="0">0（自宅など）</option>
                  <option value="1">1（iPhone テザリングなど）</option>
                </select>
              </label>
              <label>
                SSID
                <input
                  mix={[
                    inputStyle,
                    on("input", (e) => {
                      ssid = value(e);
                    }),
                  ]}
                  defaultValue=""
                  autocomplete="off"
                />
              </label>
              <label>
                パスワード
                <input
                  type="password"
                  mix={[
                    inputStyle,
                    on("input", (e) => {
                      pass = value(e);
                    }),
                  ]}
                  defaultValue=""
                  autocomplete="off"
                />
              </label>
              <button type="submit" mix={[buttonStyle, primaryStyle]}>
                保存
              </button>
            </form>
            <form
              mix={[
                rowStyle,
                on("submit", (e) => {
                  e.preventDefault();
                  if (command) send(command);
                }),
              ]}
            >
              <input
                mix={[
                  inputStyle,
                  cmdStyle,
                  on("input", (e) => {
                    command = value(e);
                  }),
                ]}
                defaultValue=""
                placeholder="コマンド（help）"
              />
              <button type="submit" mix={buttonStyle}>送信</button>
            </form>
            <pre mix={logStyle}>{lines.slice(-200).join("\n")}</pre>
          </section>
        </div>
      );
    };
  },
);

const wrapStyle = css({ display: "grid", gap: "1.25rem" });
const boxStyle = css({
  padding: "1.25rem",
  border: `1px solid ${color.border}`,
  borderRadius: radius.lg,
  background: color.card,
});
const h2Style = css({ marginTop: 0 });
const mutedStyle = css({ color: color.muted, fontSize: "0.9rem" });
const rowStyle = css({
  display: "flex",
  flexWrap: "wrap",
  gap: "0.5rem",
  marginBlock: "0.75rem",
});
const formStyle = css({
  display: "grid",
  gridTemplateColumns: "repeat(auto-fit, minmax(10rem, 1fr))",
  gap: "0.75rem",
  alignItems: "end",
  marginBlock: "1rem",
  "& label": { display: "grid", gap: "0.25rem", fontSize: "0.9rem" },
});
const buttonStyle = css({
  font: "inherit",
  cursor: "pointer",
  padding: "0.5rem 1rem",
  border: `1px solid ${color.border}`,
  borderRadius: radius.md,
  background: color.bg,
  color: color.fg,
  "&:disabled": { opacity: 0.5, cursor: "not-allowed" },
});
const primaryStyle = css({
  borderColor: color.accent,
  background: color.accent,
  color: color.onAccent,
});
const inputStyle = css({
  font: "inherit",
  padding: "0.45rem 0.6rem",
  border: `1px solid ${color.border}`,
  borderRadius: radius.md,
  background: color.bg,
  color: color.fg,
});
const cmdStyle = css({ flex: 1, minWidth: "12rem", fontFamily: font.mono });
const progressStyle = css({
  display: "block",
  width: "100%",
  marginTop: "0.75rem",
});
const logStyle = css({
  margin: 0,
  height: "20rem",
  overflow: "auto",
  padding: "0.75rem",
  borderRadius: radius.md,
  background: "#0b0f19",
  color: "#d1d5db",
  fontSize: "0.8rem",
  fontFamily: font.mono,
  whiteSpace: "pre-wrap",
});
