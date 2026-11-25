/**
 * クライアントランタイム。island を hydrate する（テンプレの hydration.ts と同じ）。
 */

import { run } from "@remix-run/component";

import { guardBrowserNavigations } from "./navigation-guard.ts";

guardBrowserNavigations();

const app = run({
  loadModule: async (moduleUrl, exportName) => {
    const module = await import(moduleUrl) as Record<string, unknown>;
    const picked = module[exportName];
    if (typeof picked !== "function") {
      throw new Error(
        `Module "${moduleUrl}" has no function export "${exportName}".`,
      );
    }
    return picked;
  },
});

await app.ready();
