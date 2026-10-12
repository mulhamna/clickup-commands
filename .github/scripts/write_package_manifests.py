#!/usr/bin/env python3
"""Write the Homebrew formula + Scoop manifest, or refresh winget manifests.

Usage (env: VERSION, MACOS_ARM, MACOS_X86, LINUX_ARM, LINUX_X86, WIN_X86,
WIN_ARM, RELEASE_DATE):

    write_package_manifests.py OUT_DIR            # -> OUT_DIR/clickupc.rb, clickupc.json
    write_package_manifests.py --winget DIR       # patch DIR/clickupc*.yaml in place
"""

import json
import os
import re
import sys
from pathlib import Path

REPO = "mulhamna/clickup-commands"
DESC = "Unofficial ClickUp CLI and MCP server"


def url(version: str, asset: str) -> str:
    return f"https://github.com/{REPO}/releases/download/v{version}/{asset}"


def formula(e: dict) -> str:
    v = e["VERSION"]

    def block(asset: str, sha: str) -> str:
        return f'      url "{url(v, asset)}"\n      sha256 "{sha}"'

    return f'''class Clickupc < Formula
  desc "{DESC}"
  homepage "https://github.com/{REPO}"
  version "{v}"
  license "MIT"

  on_macos do
    on_arm do
{block("clickupc-macos-aarch64.tar.gz", e["MACOS_ARM"])}
    end
    on_intel do
{block("clickupc-macos-x86_64.tar.gz", e["MACOS_X86"])}
    end
  end

  on_linux do
    on_arm do
{block("clickupc-linux-aarch64.tar.gz", e["LINUX_ARM"])}
    end
    on_intel do
{block("clickupc-linux-x86_64.tar.gz", e["LINUX_X86"])}
    end
  end

  def install
    bin.install Dir["clickupc-*"].first => "clickupc"
  end

  test do
    assert_match version.to_s, shell_output("#{{bin}}/clickupc --version")
  end
end
'''


def scoop(e: dict) -> str:
    v = e["VERSION"]
    arch = {
        "64bit": ("clickupc-windows-x86_64", e["WIN_X86"]),
        "arm64": ("clickupc-windows-aarch64", e["WIN_ARM"]),
    }
    manifest = {
        "version": v,
        "description": DESC,
        "homepage": f"https://github.com/{REPO}",
        "license": "MIT",
        "architecture": {
            k: {"url": url(v, f"{n}.zip"), "hash": sha, "bin": [[f"{n}.exe", "clickupc"]]}
            for k, (n, sha) in arch.items()
        },
        "checkver": "github",
        "autoupdate": {
            "architecture": {
                k: {"url": f"https://github.com/{REPO}/releases/download/v$version/{n}.zip"}
                for k, (n, _) in arch.items()
            }
        },
    }
    return json.dumps(manifest, indent=2) + "\n"


def sub_once(pattern: str, repl: str, text: str, path: Path) -> str:
    out, n = re.subn(pattern, repl, text, count=1, flags=re.MULTILINE)
    if n != 1:
        raise SystemExit(f"{path}: expected one match for {pattern!r}")
    return out


def winget(directory: Path, e: dict) -> None:
    v = e["VERSION"]
    for path in sorted(directory.glob("clickupc*.yaml")):
        text = sub_once(r"^PackageVersion: .*$", f"PackageVersion: {v}", path.read_text(), path)
        if path.name.endswith(".installer.yaml"):
            text = sub_once(r"^ReleaseDate: .*$", f"ReleaseDate: {e['RELEASE_DATE']}", text, path)
            for asset, sha in [
                ("clickupc-windows-x86_64.zip", e["WIN_X86"]),
                ("clickupc-windows-aarch64.zip", e["WIN_ARM"]),
            ]:
                text = sub_once(
                    rf"^(\s+InstallerUrl: ).*/{re.escape(asset)}\n(\s+InstallerSha256: ).*$",
                    rf"\g<1>{url(v, asset)}\n\g<2>{sha}",
                    text,
                    path,
                )
        path.write_text(text)
        print(f"updated {path}")


def main() -> None:
    e = dict(os.environ)
    missing = [k for k in ("VERSION",) if not e.get(k)]
    if missing:
        raise SystemExit(f"missing env: {missing}")
    if len(sys.argv) == 3 and sys.argv[1] == "--winget":
        winget(Path(sys.argv[2]), e)
        return
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    for key in ("MACOS_ARM", "MACOS_X86", "LINUX_ARM", "LINUX_X86", "WIN_X86", "WIN_ARM"):
        if not re.fullmatch(r"[0-9a-f]{64}", e.get(key, "")):
            raise SystemExit(f"{key} is not a sha256: {e.get(key)!r}")
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    (out / "clickupc.rb").write_text(formula(e))
    (out / "clickupc.json").write_text(scoop(e))
    print(f"wrote {out}/clickupc.rb and {out}/clickupc.json")


if __name__ == "__main__":
    main()
