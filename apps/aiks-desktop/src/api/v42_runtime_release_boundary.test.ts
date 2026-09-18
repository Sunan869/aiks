import { describe, expect, it } from "vitest";
import releaseScript from "../../../../scripts/build-release.ps1?raw";
import setupScript from "../../../../scripts/setup-siyuan.ps1?raw";

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

  it("downloads the locked runtime asset directly without resolving it through the GitHub REST API", () => {
    expect(setupScript).not.toContain("api.github.com/repos/$repo/releases/tags/$tag");
    expect(setupScript).toContain("https://github.com/$repo/releases/download/$tag/$assetName");
    expect(setupScript).toContain("DownloadFile($downloadUrl, $archivePath)");
  });
});
