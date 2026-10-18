#!/usr/bin/env node
// Puts adb.exe (+ its DLLs and license notice) in
// apps/desktop/src-tauri/resources/adb, bundled with the Windows installer
// for the USB connection. Copies from a local Android SDK when there is one
// (ANDROID_HOME / ANDROID_SDK_ROOT / %LOCALAPPDATA%\Android\Sdk), otherwise
// downloads Google's platform-tools. Does nothing when already in place.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "apps/desktop/src-tauri/resources/adb");
const FILES = ["adb.exe", "AdbWinApi.dll", "AdbWinUsbApi.dll", "NOTICE.txt"];
const URL = "https://dl.google.com/android/repository/platform-tools-latest-windows.zip";

if (FILES.every((f) => existsSync(join(out, f)))) process.exit(0);
mkdirSync(out, { recursive: true });

const sdks = [process.env.ANDROID_HOME, process.env.ANDROID_SDK_ROOT, process.env.LOCALAPPDATA && join(process.env.LOCALAPPDATA, "Android", "Sdk")]
  .filter(Boolean)
  .map((sdk) => join(sdk, "platform-tools"));
let source = sdks.find((dir) => FILES.every((f) => existsSync(join(dir, f))));

let temp;
if (!source) {
  temp = mkdtempSync(join(tmpdir(), "micnexus-adb-"));
  const zip = join(temp, "platform-tools.zip");
  console.log(`downloading ${URL}`);
  execFileSync("curl", ["-fsSL", "--retry", "3", "-o", zip, URL], { stdio: "inherit" });
  execFileSync("tar", ["-xf", zip, "-C", temp], { stdio: "inherit" });
  source = join(temp, "platform-tools");
}
for (const f of FILES) copyFileSync(join(source, f), join(out, f));
if (temp) rmSync(temp, { recursive: true, force: true });
console.log(`adb ready in ${out} (from ${temp ? "download" : source})`);
