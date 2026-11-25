/**
 * ドキュメントの殻。kuboon/deno-remix-tmpl の layout.tsx を縮めたもの。
 *
 * `script` はクライアントランタイム（island を置くページだけ）。`server/runtime.ts` が解決して渡す。
 */

import { css, type Handle, type RemixNode } from "@remix-run/component";

import { base, BASE_META_NAME } from "./base.ts";
import { routes } from "./routes.ts";
import { color, contentWidth } from "./tokens.ts";

export interface ClientRuntime {
  src: string;
  preloads: readonly string[];
}

export interface LayoutProps {
  title: string;
  description?: string;
  script: ClientRuntime | null;
  children: RemixNode;
}

export function Layout(handle: Handle<LayoutProps>) {
  return () => {
    const props = handle.props;
    return (
      <html lang="ja">
        <head>
          <meta charset="utf-8" />
          <meta
            name="viewport"
            content="width=device-width, initial-scale=1"
          />
          <title>{props.title}</title>
          {props.description
            ? <meta name="description" content={props.description} />
            : null}
          <meta property="og:title" content={props.title} />
          <meta name={BASE_META_NAME} content={base} />
          <link rel="stylesheet" href={`${base}/static/app.css`} />
          <link
            rel="icon"
            type="image/svg+xml"
            href={`${base}/static/favicon.svg`}
          />
          {(props.script?.preloads ?? []).map((href) => (
            <link key={href} rel="modulepreload" href={href} />
          ))}
        </head>
        <body>
          <header mix={[bandStyle, headerStyle]}>
            <a mix={brandStyle} href={routes.home.href()}>m5-wasmos</a>
            <nav mix={navStyle}>
              <a href={routes.home.href()}>Home</a>
              <a href={routes.sim.href()}>Simulator</a>
              <a href="https://github.com/kuboon/m5-wasmos">GitHub</a>
            </nav>
          </header>
          <main mix={[bandStyle, mainStyle]}>{props.children}</main>
          <footer mix={[bandStyle, footerStyle]}>
            <p>
              M5Stack Tab5 用の wasm アプリ OS。Built with{" "}
              <a href="https://remix.run">Remix v3</a> on{" "}
              <a href="https://deno.com">Deno</a>.
            </p>
          </footer>
          {props.script
            ? <script type="module" src={props.script.src}></script>
            : null}
        </body>
      </html>
    );
  };
}

const bandStyle = css({
  width: "100%",
  maxWidth: contentWidth,
  marginInline: "auto",
  paddingInline: "1.25rem",
});

const headerStyle = css({
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: "1rem",
  flexWrap: "wrap",
  paddingBlock: "1.25rem",
  borderBottom: `1px solid ${color.border}`,
});

const brandStyle = css({
  fontWeight: 700,
  fontSize: "1.1rem",
  textDecoration: "none",
  color: color.fg,
});

const navStyle = css({ display: "flex", flexWrap: "wrap", gap: "1rem" });

const mainStyle = css({ paddingBlock: "2rem" });

const footerStyle = css({
  paddingBlock: "2rem",
  borderTop: `1px solid ${color.border}`,
  color: color.muted,
  fontSize: "0.9rem",
});
