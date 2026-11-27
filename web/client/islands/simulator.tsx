/**
 * ブラウザシミュレータ（island）。`sim/host.ts` に canvas を渡して起動する。
 *
 * 起動アプリは `/sim/<id>` で選べる。Home ボタンは実機の「画面上端から下スワイプ」と同じ。
 */

import {
  clientEntry,
  css,
  type Handle,
  on,
  ref,
  type SerializableValue,
} from "@remix-run/component";

import { routes } from "../routes.ts";
import { Host } from "../sim/host.ts";
import { color, font, radius } from "../tokens.ts";

export interface SimulatorProps {
  manifestUrl: string;
  /** 空ならランチャー */
  initialApp: string;
  [key: string]: SerializableValue;
}

type LogLine = { text: string; level: "info" | "error" };

export const Simulator = clientEntry(
  import.meta.url,
  function Simulator(handle: Handle<SimulatorProps>) {
    let host: Host | null = null;
    let current = "";
    const lines: LogLine[] = [];
    let canvas: HTMLCanvasElement | null = null;

    const log = (text: string, level: "info" | "error" = "info") => {
      lines.push({ text, level });
      if (lines.length > 200) lines.splice(0, lines.length - 200);
      if (level === "error") console.error(text);
      handle.update();
    };

    handle.queueTask(() => {
      if (!canvas) return;
      host = new Host({
        canvas,
        manifestUrl: handle.props.manifestUrl,
        log,
        onAppChange: (app) => {
          current = app.id;
          history.replaceState(
            history.state,
            "",
            routes.simApp.href({ app: app.id }),
          );
          handle.update();
        },
      });
      const initial = handle.props.initialApp || undefined;
      host.boot(initial).catch((e) => log(`boot: ${e}`, "error"));
    });

    return () => (
      <div mix={wrapStyle}>
        <div mix={deviceStyle}>
          <canvas
            mix={[
              canvasStyle,
              on("contextmenu", (e) => e.preventDefault()),
              ref((node) => (canvas = node as HTMLCanvasElement)),
            ]}
            width={720}
            height={1280}
          />
        </div>
        <div mix={barStyle}>
          <button
            type="button"
            mix={[buttonStyle, on("click", () => host?.home())]}
          >
            Home
          </button>
          <span mix={currentStyle}>
            {current ? `running: ${current}` : "booting…"}
          </span>
          <span mix={hintStyle}>
            画面上端から下へスワイプでもランチャーに戻ります
          </span>
        </div>
        <pre mix={logStyle}>
          {lines.slice(-60).map((l, i) => (
            <div key={i} mix={l.level === "error" ? errorStyle : undefined}>
              {l.text}
            </div>
          ))}
        </pre>
      </div>
    );
  },
);

const wrapStyle = css({ display: "grid", gap: "0.75rem" });

// 縦置き。画面の高さに収まる幅にする
const deviceStyle = css({
  justifySelf: "center",
  padding: "18px",
  borderRadius: "28px",
  background: "#2b2f36",
  boxShadow: "0 10px 30px rgba(0,0,0,.35)",
});

const canvasStyle = css({
  display: "block",
  width: "min(calc(100vw - 4rem), calc(80vh * 9 / 16))",
  aspectRatio: "9 / 16",
  borderRadius: "6px",
  background: "#000",
  userSelect: "none",
});

const barStyle = css({
  display: "flex",
  alignItems: "center",
  gap: "1rem",
  flexWrap: "wrap",
});

const buttonStyle = css({
  font: "inherit",
  cursor: "pointer",
  padding: "0.4rem 1rem",
  border: `1px solid ${color.border}`,
  borderRadius: radius.md,
  background: color.card,
  color: color.fg,
  "&:hover": { borderColor: color.accent, color: color.accent },
});

const currentStyle = css({ fontFamily: font.mono, fontSize: "0.9rem" });
const hintStyle = css({ color: color.muted, fontSize: "0.85rem" });

const logStyle = css({
  margin: 0,
  maxHeight: "14rem",
  overflow: "auto",
  padding: "0.75rem",
  borderRadius: radius.md,
  background: color.card,
  border: `1px solid ${color.border}`,
  fontSize: "0.8rem",
  fontFamily: font.mono,
});

const errorStyle = css({ color: "#ef4444" });
