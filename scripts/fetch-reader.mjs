import { createHash } from "node:crypto";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve, dirname } from "node:path";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const version = "2026.08.19";
const expected =
  "66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a";
const destination = resolve(root, "vendor/yt-dlp.exe");
await mkdir(resolve(root, "vendor"), { recursive: true });
let bytes = await readFile(destination).catch(() => null);
if (!bytes || createHash("sha256").update(bytes).digest("hex") !== expected) {
  const response = await fetch(
    `https://github.com/yt-dlp/yt-dlp/releases/download/${version}/yt-dlp.exe`,
  );
  if (!response.ok)
    throw new Error(`yt-dlp download failed: HTTP ${response.status}`);
  bytes = Buffer.from(await response.arrayBuffer());
  if (createHash("sha256").update(bytes).digest("hex") !== expected)
    throw new Error("yt-dlp checksum mismatch");
  await writeFile(destination, bytes);
}
for (const name of ["LICENSE", "THIRD_PARTY_LICENSES.txt"]) {
  const path = resolve(root, `vendor/yt-dlp-${name}.txt`);
  if (await readFile(path).catch(() => null)) continue;
  const response = await fetch(
    `https://raw.githubusercontent.com/yt-dlp/yt-dlp/${version}/${name}`,
  );
  if (!response.ok) throw new Error(`Could not fetch yt-dlp license: ${name}`);
  await writeFile(path, await response.text());
}
console.log(`Bundled reader ready: yt-dlp ${version} (SHA-256 verified)`);
