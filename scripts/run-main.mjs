import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "build", "main-run");
const tar = join(root, "build", "main-run.tar");
const npm = process.platform === "win32" ? "npm.cmd" : "npm";

try {
  execFileSync("git", ["rev-parse", "--verify", "--quiet", "main^{commit}"], { cwd: root, stdio: "ignore" });
} catch {
  console.error("Main Run: the 'main' branch has no commit yet, so there is nothing to run. Commit to main first, or use Dev Run.");
  process.exit(1);
}

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
execFileSync("git", ["archive", "--format=tar", "-o", tar, "main"], { cwd: root, stdio: "inherit" });
execFileSync("tar", ["-xf", tar, "-C", out], { stdio: "inherit" });
rmSync(tar, { force: true });

const env = { ...process.env, NOTIFI_NO_FOCUS: "1" };
for (const args of [["ci"], ["run", "dev"]]) {
  const r = spawnSync(npm, args, { cwd: out, env, stdio: "inherit", shell: process.platform === "win32" });
  if (r.status !== 0) process.exit(r.status ?? 1);
}
