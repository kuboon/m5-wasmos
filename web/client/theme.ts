/** 複数のモジュールで使う `css(...)` mixin */

import { css } from "@remix-run/component";

import { color, radius } from "./tokens.ts";

/** 枠付きのブロック */
export const cardStyle = css({
  marginBlock: "2rem",
  padding: "1.25rem",
  border: `1px solid ${color.border}`,
  borderRadius: radius.lg,
  background: color.card,
  "& > :first-child": { marginTop: 0 },
  "& > :last-child": { marginBottom: 0 },
});

/** `<a href>` に付ける塗りのボタン */
export const buttonStyle = css({
  display: "inline-block",
  marginTop: "0.5rem",
  padding: "0.6rem 1rem",
  borderRadius: radius.md,
  background: color.accent,
  color: color.onAccent,
  textDecoration: "none",
  fontWeight: 600,
  "&:hover": { filter: "brightness(1.08)" },
});
