/**
 * ブラウザへ渡すモジュールを 1 つのグラフとしてバンドルする（テンプレと同じ）。
 */

import { createAssetServer } from "@remix-kbn/assets-deno";

import { base } from "../client/base.ts";

const clientDir = new URL("../client/", import.meta.url);

export const assetsPath = `${base}/assets`;

export const assets = await createAssetServer({
  rootDir: decodeURIComponent(clientDir.pathname),
  entrypoints: ["hydration.ts", "islands/*.tsx"],
  basePath: assetsPath,
  mode: "bundle",
  bundle: { sourcemap: "none" },
});
