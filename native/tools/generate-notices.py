#!/usr/bin/env python3
"""Generate the shipped license inventory from the macOS runner's locked graph.

Run after `nsh build packaging` has generated the release configuration's runner:
  python3 native/tools/generate-notices.py
This generates notices only; packaging remains `nsh package packaging --target macos`.
"""
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
manifest = ROOT / "packaging/.nsh/rust/macos-app/Cargo.toml"
metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--manifest-path", str(manifest), "--locked",
    "--offline", "--filter-platform", "aarch64-apple-darwin", "--format-version", "1",
], text=True))
packages = {p["id"]: p for p in metadata["packages"]}
nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
seen = set()
pending = [metadata["resolve"]["root"]]
while pending:
    package_id = pending.pop()
    if package_id in seen:
        continue
    seen.add(package_id)
    for dep in nodes[package_id]["deps"]:
        if any(k["kind"] != "dev" for k in dep["dep_kinds"]):
            pending.append(dep["pkg"])
dependencies = sorted(
    (packages[i] for i in seen if packages[i]["source"]),
    key=lambda p: (p["name"].lower(), p["version"]),
)
lines = [
    "Haiku — 第三方声明",
    "",
    "应用自身许可证尚未声明。以下声明仅适用于第三方组件。",
    "",
    "Rust 依赖许可证清单",
    "来源：NSH macOS release runner 的 Cargo.lock / cargo metadata。",
    "平台：aarch64-apple-darwin；包括构建依赖，不包括仅用于测试的依赖。",
    "SPDX 表达式照录依赖的 Cargo 元数据，不代替各项目的完整许可证条款。",
    f"共 {len(dependencies)} 个第三方 Rust 包（同名不同版本分别列出）。",
    "",
]
for p in dependencies:
    license_name = p["license"]
    if not license_name and p["license_file"]:
        license_name = "自定义许可证：" + p["license_file"]
    if not license_name:
        raise SystemExit(f"Missing license metadata: {p['name']} {p['version']}")
    lines.append(f"{p['name']} {p['version']} — {license_name}")
    if p["repository"] or p["homepage"]:
        lines.append(p["repository"] or p["homepage"])
    lines.append("")
lines += [
    "Iconoir icons — MIT",
    (ROOT / "assets/icons/LICENSE").read_text(),
    "unDraw illustrations",
    (ROOT / "assets/illustrations/LICENSE.txt").read_text(),
]
output = ROOT / "native/app/assets/THIRD_PARTY_NOTICES.txt"
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text("\n".join(lines) + "\n")
print(f"Generated {output.relative_to(ROOT)}: {len(dependencies)} Rust packages")
