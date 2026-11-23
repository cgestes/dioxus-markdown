import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));

export function buildCheckbox() {
  const log = execFileSync("cargo", [
    "build", "--lib", "--locked", "--release", "--target", "wasm32-unknown-unknown", "--message-format=json"
  ], { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
  for (const line of log.trim().split("\n")) {
    const artifact = JSON.parse(line);
    const wasm = artifact.filenames?.find(path => path.endsWith(".wasm"));
    if (wasm) return readFileSync(wasm);
  }
  throw new Error("Cargo did not produce the checkbox WebAssembly module.");
}

export function checkboxNotices() {
  const graph = JSON.parse(execFileSync("cargo", ["metadata", "--locked", "--format-version", "1",
    "--filter-platform", "wasm32-unknown-unknown"], { cwd: root, encoding: "utf8", maxBuffer: 32 * 1024 * 1024 }));
  const seen = new Set(), notices = [];
  const nodes = new Map(graph.resolve.nodes.map(n => [n.id, n]));
  const packages = new Map(graph.packages.map(p => [p.id, p]));
  function visit(id) {
    if (seen.has(id)) return;
    seen.add(id);
    if (id !== graph.resolve.root) {
      const pkg = packages.get(id);
      const license = ["LICENSE-MIT", "LICENSE", "LICENSE.md", "COPYING"].map(f => join(dirname(pkg.manifest_path), f)).find(existsSync);
      if (!license) throw new Error("Missing distribution notice for " + pkg.name);
      notices.push(pkg.name + " " + pkg.version + "\n" + readFileSync(license, "utf8"));
    }
    for (const dep of nodes.get(id).deps) if (dep.dep_kinds.some(k => k.kind == null)) visit(dep.pkg);
  }
  visit(graph.resolve.root);
  return notices;
}
