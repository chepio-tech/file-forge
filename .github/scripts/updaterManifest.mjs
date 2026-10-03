// Writes `installers/latest.json`, the manifest installed apps read to find updates (ADR-0013). Refuses to write it
// when installed apps would reject the update: every artifact must carry a valid signature by the key in
// `plugins.updater.pubkey` of `src-tauri/tauri.conf.json`, bound to the release version. `tauri build` only warns
// about a mismatched key, so without this check a release could strand every installed copy.
//
// Input from the build jobs, per updater target: `updates/<target>.asset` (release asset name in `installers/`) and
// `updates/<target>.sig` (the signature `tauri build` wrote next to that artifact). Environment: TAG,
// GITHUB_REPOSITORY.

// Core
import { createHash, createPublicKey, verify } from "node:crypto";
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

function fail(message) {
  console.log(`::error::${message}`);
  process.exit(1);
}

/** A minisign public key, as Tauri stores it: base64 of the text file `tauri signer generate` writes. */
function readPublicKey(configured) {
  const lines = Buffer.from(configured, "base64").toString("utf8").trim().split("\n");
  const bytes = Buffer.from(lines.at(-1) ?? "", "base64");
  if (bytes.length !== 42 || bytes.toString("latin1", 0, 2) !== "Ed") {
    throw new Error("plugins.updater.pubkey is not a minisign public key");
  }
  const x = bytes.subarray(10).toString("base64url");
  return { keyId: bytes.subarray(2, 10), key: createPublicKey({ key: { kty: "OKP", crv: "Ed25519", x }, format: "jwk" }) };
}

/** Checks what the updater plugin checks before installing: key, file signature, trusted comment and version. */
function verifySignature(data, signature, publicKey, version) {
  const [, signatureLine = "", trustedLine = "", globalLine = ""] = Buffer.from(signature, "base64")
    .toString("utf8")
    .split("\n");
  const bytes = Buffer.from(signatureLine, "base64");
  if (bytes.length !== 74) throw new Error("the signature is malformed");
  const keyId = bytes.subarray(2, 10);
  if (!keyId.equals(publicKey.keyId)) {
    const signer = Buffer.from(keyId).reverse().toString("hex").toUpperCase();
    const trusted = Buffer.from(publicKey.keyId).reverse().toString("hex").toUpperCase();
    throw new Error(`signed by key ${signer}, but the app trusts key ${trusted} (plugins.updater.pubkey)`);
  }
  // minisign: "ED" (what Tauri writes) signs the BLAKE2b-512 hash of the file, legacy "Ed" the file itself.
  const prehashed = bytes.toString("latin1", 0, 2) === "ED";
  const message = prehashed ? createHash("blake2b512").update(data).digest() : data;
  const fileSignature = bytes.subarray(10);
  if (!verify(null, message, publicKey.key, fileSignature)) throw new Error("the signature does not match the file");

  const comment = trustedLine.replace(/^trusted comment: /, "");
  const global = Buffer.from(globalLine, "base64");
  if (!verify(null, Buffer.concat([fileSignature, Buffer.from(comment, "utf8")]), publicKey.key, global)) {
    throw new Error("the trusted comment is not signed");
  }
  const signedVersion = comment.split("\t").find((field) => field.startsWith("version:"))?.slice("version:".length);
  if (signedVersion !== version) {
    throw new Error(`signed for version ${signedVersion ?? "(none)"}, but the release is ${version}`);
  }
}

const { TAG, GITHUB_REPOSITORY } = process.env;
if (!TAG || !GITHUB_REPOSITORY) fail("TAG and GITHUB_REPOSITORY are required");
const version = TAG.replace(/^v/, "");

let publicKey;
try {
  const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
  publicKey = readPublicKey(config.plugins?.updater?.pubkey ?? "");
} catch (error) {
  fail(`Cannot read the updater public key: ${error.message}`);
}

const targets = readdirSync("updates")
  .filter((name) => name.endsWith(".asset"))
  .map((name) => name.slice(0, -".asset".length))
  .sort();
if (targets.length === 0) fail("No updater artifacts were collected");

const platforms = {};
for (const target of targets) {
  const asset = readFileSync(join("updates", `${target}.asset`), "utf8").trim();
  try {
    const signature = readFileSync(join("updates", `${target}.sig`), "utf8").trim();
    verifySignature(readFileSync(join("installers", asset)), signature, publicKey, version);
    const url = `https://github.com/${GITHUB_REPOSITORY}/releases/download/${TAG}/${encodeURIComponent(asset)}`;
    platforms[target] = { url, signature };
  } catch (error) {
    fail(`Updater artifact ${target} (${asset}) would be rejected by installed apps: ${error.message}`);
  }
}

writeFileSync(join("installers", "latest.json"), `${JSON.stringify({ version, platforms }, null, 2)}\n`);
console.log(`Wrote installers/latest.json for ${version}: ${targets.join(", ")}`);
