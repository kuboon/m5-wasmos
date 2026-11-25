/**
 * サイトのルーター。`deno serve` でそのまま開発サーバになり、`@remix-kbn/ssg` が同じものを
 * fetch() でクロールして GitHub Pages 用の静的ファイルを書き出す（kuboon/deno-remix-tmpl と同じ）。
 *
 * アプリ一式は `deno task build:apps`（scripts/build_apps.ts）が web/build/apps に置き、
 * ここが `${base}/apps/*` として配信する。リンクを辿っても見つからないファイル
 * （シミュレータの .js / .wasm、manifest.json）は全部 `entryPoints` に挙げる。
 */

import { createRouter, type RouterContext } from "@remix-run/fetch-router";
import { render } from "@remix-run/render-middleware";
import { createFileTree, githubPages } from "@remix-kbn/ssg/site";
import { stripBase } from "@remix-kbn/ssg/base";
import type { FileServerBehavior } from "@remix-kbn/ssg/site";

import { assets, assetsPath } from "./assets.ts";
import { clientRuntime } from "./runtime.ts";
import { base } from "../client/base.ts";
import { Layout } from "../client/layout.tsx";
import { routes } from "../client/routes.ts";
import * as Home from "../client/pages/index.tsx";
import * as Sim from "../client/pages/sim.tsx";

export { base };

export const fileServer: FileServerBehavior = githubPages();

const appsDir = `${import.meta.dirname}/../build/apps`;

const manifest = JSON.parse(
  await Deno.readTextFile(`${appsDir}/manifest.json`).catch(() => {
    throw new Error(
      "web/build/apps/manifest.json がありません。先に `deno task build:apps` を実行してください",
    );
  }),
) as { abi: string; apps: Home.AppSummary[] };

const staticFiles = await createFileTree({
  rootDir: `${import.meta.dirname}/../client/static`,
  basePath: `${base}/static`,
  cacheControl: "public, max-age=3600",
});

const appFiles = await createFileTree({
  rootDir: appsDir,
  basePath: `${base}/apps`,
});

const router = createRouter({ middleware: [render({ assets })] });

export type AppContext = RouterContext<typeof router>;

declare module "@remix-run/fetch-router" {
  interface RouterTypes {
    context: AppContext;
  }
}

router.map(routes.home, (context: AppContext) =>
  context.render(
    <Layout title={Home.title} description={Home.description} script={null}>
      <Home.default apps={manifest.apps} abi={manifest.abi} />
    </Layout>,
  ));

router.map(routes.sim, (context: AppContext) =>
  context.render(
    <Layout
      title={Sim.title}
      description={Sim.description}
      script={clientRuntime}
    >
      <Sim.default />
    </Layout>,
  ));

router.map(routes.simApp, (context) =>
  context.render(
    <Layout
      title={`${context.params.app} — ${Sim.title}`}
      description={Sim.description}
      script={clientRuntime}
    >
      <Sim.default app={context.params.app} />
    </Layout>,
  ));

router.get(`${base}/static/*path`, ({ request }) => staticFiles.fetch(request));
router.get(`${base}/apps/*path`, ({ request }) => appFiles.fetch(request));
router.get(`${assetsPath}/*path`, ({ request }) => assets.fetch(request));

export const entryPoints: readonly string[] = [
  "/",
  ...[...appFiles.paths()].map((p) => stripBase(p, base)),
];

export default router;
