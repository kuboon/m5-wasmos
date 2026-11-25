import { css, type Handle } from "@remix-run/component";

import { manifestPath, routes } from "../routes.ts";
import { buttonStyle, cardStyle } from "../theme.ts";
import { color, font, radius } from "../tokens.ts";

export const title = "m5-wasmos";
export const description =
  "M5Stack Tab5 で wasm アプリをファームウェア焼き直し無しで差し替える OS";

/** サーバが manifest から渡す、ページに出すぶんのアプリ情報 */
export interface AppSummary {
  id: string;
  name: string;
  version: string;
  description: string;
  color: string;
  hidden: boolean;
  wasm: string;
  size: number;
}

export interface HomeProps {
  apps: AppSummary[];
  abi: string;
}

export default function Home(handle: Handle<HomeProps>) {
  return () => {
    const { apps, abi } = handle.props;
    const appsBase = manifestPath.replace(/manifest\.json$/, "");
    return (
      <>
        <h1>m5-wasmos</h1>
        <p>
          M5Stack Tab5（ESP32-P4）用の小さな OS。アプリは全部 wasm
          コンポーネントで、ランチャーもその 1
          つ。本体の更新ボタンでこのサイトの{" "}
          <a href={manifestPath}>
            <code>apps/manifest.json</code>
          </a>{" "}
          を読み、新しい wasm を取ってきて差し替えます。
        </p>
        <p>
          <a mix={buttonStyle} href={routes.sim.href()}>
            ブラウザで試す（シミュレータ）
          </a>
        </p>

        <section mix={cardStyle}>
          <h2>アプリ</h2>
          <ul mix={appListStyle}>
            {apps.map((app) => (
              <li key={app.id} mix={appStyle}>
                <span
                  mix={tileStyle}
                  style={{ background: app.color }}
                  aria-hidden="true"
                >
                  {app.name.slice(0, 1)}
                </span>
                <span>
                  <strong>{app.name}</strong>{" "}
                  <span mix={metaStyle}>
                    {app.id} {app.version} · {(app.size / 1024).toFixed(1)} KiB
                    {app.hidden ? " · システム" : ""}
                  </span>
                  <br />
                  {app.description}
                  <br />
                  <a href={routes.simApp.href({ app: app.id })}>
                    シミュレータで開く
                  </a>
                  {" · "}
                  <a href={`${appsBase}${app.wasm}`}>.wasm</a>
                </span>
              </li>
            ))}
          </ul>
          <p mix={metaStyle}>ABI: {abi}</p>
        </section>

        <section mix={cardStyle}>
          <h2>しくみ</h2>
          <ul>
            <li>
              <strong>WIT</strong>（<code>wit/wasmos.wit</code>）— ホスト API。
              描画コマンドリスト（<code>gfx</code>）、イベント（タッチ /
              フレーム / タイマー / io）、アプリ起動（<code>system</code>）、
              そして <code>wasi:http</code>。ファイルシステムは無し。
            </li>
            <li>
              <strong>SDK</strong>（<code>crates/sdk</code>）— Rust 用。
              ボタン、スクロール、非ブロッキング HTTP クライアントなど、
              ファームを焼き直さずに直せるものはこちらに置く。
            </li>
            <li>
              <strong>シミュレータ</strong>{" "}
              — 同じ WIT をブラウザで実装（canvas / pointer events / fetch）。
            </li>
            <li>
              <strong>ファームウェア</strong>{" "}
              — ESP-IDF + wasm ランタイム（これから）。
            </li>
          </ul>
        </section>
      </>
    );
  };
}

const appListStyle = css({
  listStyle: "none",
  padding: 0,
  display: "grid",
  gap: "1rem",
});

const appStyle = css({
  display: "flex",
  gap: "1rem",
  alignItems: "flex-start",
});

const tileStyle = css({
  flex: "none",
  display: "grid",
  placeItems: "center",
  width: "3.5rem",
  height: "3.5rem",
  borderRadius: radius.lg,
  color: "#fff",
  fontWeight: 700,
  fontSize: "1.6rem",
  fontFamily: font.sans,
});

const metaStyle = css({ color: color.muted, fontSize: "0.85rem" });
