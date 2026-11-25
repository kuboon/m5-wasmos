import { css, type Handle } from "@remix-run/component";

import { Simulator } from "../islands/simulator.tsx";
import { manifestPath } from "../routes.ts";
import { color } from "../tokens.ts";

export const title = "Simulator — m5-wasmos";
export const description =
  "M5Stack Tab5 用 wasm アプリをブラウザで動かすシミュレータ";

export interface SimProps {
  /** 最初に起動するアプリ（無ければランチャー） */
  app?: string;
}

export default function SimPage(handle: Handle<SimProps>) {
  return () => (
    <>
      <h1>Simulator</h1>
      <p mix={leadStyle}>
        実機と同じ WIT（<code>wit/wasmos.wit</code>）をブラウザで実装し、 同じ
        wasm コンポーネントを jco で JS
        に変換して動かします。マウスかタッチで操作できます
        （マルチタッチはタッチ端末で）。
      </p>
      <Simulator
        manifestUrl={manifestPath}
        initialApp={handle.props.app ?? ""}
      />
    </>
  );
}

const leadStyle = css({ color: color.muted });
