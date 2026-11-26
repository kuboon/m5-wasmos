/**
 * サイトの URL。`server/router.tsx` がこれをコントローラに対応付ける。
 *
 * `base` は GitHub Pages のサブパス（PR プレビュー含む）。アプリ一式
 * （`${base}/apps/manifest.json` ほか）は `server/router.tsx` が web/build/apps から配信する。
 */

import { get, route } from "@remix-run/fetch-router/routes";

import { base } from "./base.ts";

export const routes = route(base, {
  home: get("/"),
  sim: get("/sim"),
  /** 指定アプリから起動するシミュレータ（アプリごとに静的生成される） */
  simApp: get("/sim/:app"),
  /** 実機への書き込みと Wi-Fi 設定（Web Serial） */
  flash: get("/flash"),
});

/** 実機とシミュレータが読む manifest の URL（サイト内パス） */
export const manifestPath = `${base}/apps/manifest.json`;

/** 実機用ファームウェア（scripts/fetch_firmware.ts が置く） */
export const firmwarePath = `${base}/firmware/m5-wasmos.bin`;
