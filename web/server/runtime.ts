/** クライアントランタイムの URL と preload を一度だけ解決する */

import { assets } from "./assets.ts";
import type { ClientRuntime } from "../client/layout.tsx";

const entry = await assets.getScriptEntry("hydration.ts");

export const clientRuntime: ClientRuntime = {
  src: entry.href,
  preloads: entry.preloads,
};
