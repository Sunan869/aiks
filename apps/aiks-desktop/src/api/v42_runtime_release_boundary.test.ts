import { describe, expect, it } from "vitest";
import releaseScript from "../../../../scripts/build-release.ps1?raw";
import setupScript from "../../../../scripts/setup-siyuan.ps1?raw";
import fetcher from "../../../../scripts/fetch-siyuan-runtime.py?raw";

const LOCKED_MANIFEST_FIELDS = [
  "workbenchVersion",
  "siyuanBaseVersion",
  "upstreamCommit",
  "forkRepository",
  "forkCommit",
  "profile",
  "platform",
  "bridgeProtocol",
] as const;

describe("V4.2 release runtime identity boundary", () => {
  it("requires the prepared runtime manifest to match the full version lock", () => {
    expect(releaseScript).toContain("aiks-runtime.json");
    expect(releaseScript).toContain("Assert-RuntimeManifestIdentity");
    expect(releaseScript).toContain("Test-SiyuanRuntime -ExpectedVersion $SiyuanVersion -Config $cfg");

    for (const field of LOCKED_MANIFEST_FIELDS) {
      expect(releaseScript).toContain(field);
    }
  });

  it("downloads the locked runtime asset directly and verifies cross-platform identity", () => {
    expect(setupScript).toContain("fetch-siyuan-runtime.py");
    expect(fetcher).not.toContain("api.github.com/repos/");
    expect(fetcher).toContain("https://github.com/{repo}/releases/download/{tag}");
    expect(fetcher).toContain("sha256");
    for (const platform of [
      "windows-x64",
      "macos-x64",
      "macos-arm64",
      "linux-x64",
      "linux-arm64",
    ]) {
      expect(fetcher).toContain(platform);
    }
  });
});
