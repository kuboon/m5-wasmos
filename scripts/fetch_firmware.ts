/**
 * 実機用ファームウェアを GitHub Release（firmware-latest）から取ってきて web/build/firmware に置く。
 * ファームは .github/workflows/firmware.yml が ESP-IDF でビルドしてリリースに上げる。
 * サイト（/flash）はこれを同じオリジンから読むので、ブラウザの CORS 制限を受けない。
 *
 * まだリリースが無ければ何も置かない（/flash は「まだありません」と出す）。
 */

import { fromFileUrl, join } from "@std/path";

const repo = "kuboon/m5-wasmos";
const tag = "firmware-latest";
const outDir = join(
  fromFileUrl(new URL("../", import.meta.url)),
  "web/build/firmware",
);

await Deno.remove(outDir, { recursive: true }).catch(() => {});
await Deno.mkdir(outDir, { recursive: true });

async function get(name: string): Promise<Uint8Array | null> {
  const url = `https://github.com/${repo}/releases/download/${tag}/${name}`;
  try {
    const res = await fetch(url);
    if (!res.ok) {
      console.log(`  ${name}: HTTP ${res.status}`);
      await res.body?.cancel();
      return null;
    }
    return new Uint8Array(await res.arrayBuffer());
  } catch (e) {
    console.log(`  ${name}: ${e}`);
    return null;
  }
}

const bin = await get("m5-wasmos.bin");
const info = await get("firmware.json");
if (bin && info) {
  await Deno.writeFile(join(outDir, "m5-wasmos.bin"), bin);
  await Deno.writeFile(join(outDir, "firmware.json"), info);
  console.log(`firmware: ${(bin.byteLength / 1024 / 1024).toFixed(2)} MiB`);
} else {
  console.log("firmware: not published yet");
}
