/**
 * apps/* をビルドして web/build/apps/ に並べる。
 *
 *   web/build/apps/
 *     manifest.json            実機とシミュレータが読む一覧
 *     <id>/<id>.wasm           wasm コンポーネント（実機が取得する本体）
 *     <id>/sim/<id>.js ...     jco で JS に変換したもの（ブラウザシミュレータ用）
 *
 * アプリは `[package.metadata.wasmos]` を持つ Cargo パッケージ。manifest の URL は
 * manifest.json からの相対パスなので、PR プレビューのサブパスでもそのまま動く。
 */

import { componentNew, transpile } from "@bytecodealliance/jco";
import { encodeHex } from "@std/encoding/hex";
import { dirname, fromFileUrl, join } from "@std/path";

const root = fromFileUrl(new URL("../", import.meta.url));
const outDir = join(root, "web/build/apps");
const target = "wasm32-unknown-unknown";

/** WIT の package。manifest に書き、ホストが互換性を判定する */
const ABI = "wasmos:os@0.1.0";

interface WasmosMeta {
  name: string;
  description?: string;
  color?: string;
  hidden?: boolean;
}

interface CargoPackage {
  name: string;
  version: string;
  manifest_path: string;
  metadata: { wasmos?: WasmosMeta } | null;
}

export interface ManifestApp {
  id: string;
  name: string;
  version: string;
  description: string;
  /** "#rrggbb" */
  color: string;
  hidden: boolean;
  /** manifest.json からの相対 URL */
  wasm: string;
  sha256: string;
  size: number;
  /** シミュレータ用 ES module（manifest.json からの相対 URL） */
  sim: string;
}

export interface Manifest {
  abi: string;
  launcher: string;
  generatedAt: string;
  apps: ManifestApp[];
}

async function cargo(...args: string[]): Promise<string> {
  const out = await new Deno.Command("cargo", {
    args,
    cwd: root,
    stdout: "piped",
    stderr: "inherit",
  }).output();
  if (!out.success) throw new Error(`cargo ${args.join(" ")} failed`);
  return new TextDecoder().decode(out.stdout);
}

const metadata = JSON.parse(
  await cargo("metadata", "--format-version", "1", "--no-deps"),
) as { packages: CargoPackage[] };

const apps = metadata.packages
  .filter((p) => p.metadata?.wasmos)
  .sort((a, b) => a.name.localeCompare(b.name));

await cargo(
  "build",
  "--release",
  "--target",
  target,
  ...apps.flatMap((p) => ["-p", p.name]),
);

await Deno.remove(outDir, { recursive: true }).catch(() => {});
await Deno.mkdir(outDir, { recursive: true });

const entries: ManifestApp[] = [];
for (const pkg of apps) {
  const meta = pkg.metadata!.wasmos!;
  const id = pkg.name;
  const core = await Deno.readFile(
    join(root, "target", target, "release", `${id.replaceAll("-", "_")}.wasm`),
  );
  const component = await componentNew(core, []);

  const appDir = join(outDir, id);
  await Deno.mkdir(join(appDir, "sim"), { recursive: true });
  await Deno.writeFile(join(appDir, `${id}.wasm`), component);

  const { files } = await transpile(component, {
    name: id,
    instantiation: "async",
    nodejsCompat: false,
    typescript: false,
    quiet: true,
  });
  for (
    const [name, source] of Object.entries(
      files as Record<string, Uint8Array | string>,
    )
  ) {
    const path = join(appDir, "sim", name);
    await Deno.mkdir(dirname(path), { recursive: true });
    if (typeof source === "string") await Deno.writeTextFile(path, source);
    else await Deno.writeFile(path, source);
  }

  const digest = await crypto.subtle.digest("SHA-256", component);
  entries.push({
    id,
    name: meta.name,
    version: pkg.version,
    description: meta.description ?? "",
    color: meta.color ?? "#64748b",
    hidden: meta.hidden ?? false,
    wasm: `${id}/${id}.wasm`,
    sha256: encodeHex(new Uint8Array(digest)),
    size: component.byteLength,
    sim: `${id}/sim/${id}.js`,
  });
  console.log(`  ${id}: ${(component.byteLength / 1024).toFixed(1)} KiB`);
}

const manifest: Manifest = {
  abi: ABI,
  launcher: "launcher",
  generatedAt: new Date().toISOString(),
  apps: entries,
};
await Deno.writeTextFile(
  join(outDir, "manifest.json"),
  JSON.stringify(manifest, null, 2) + "\n",
);
console.log(`wrote ${join(outDir, "manifest.json")}`);
