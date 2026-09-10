import { describe, expect, it } from "vitest";
import { isSecretKey, maskValue } from "./mask";

describe("mask", () => {
  it("flags secret-looking keys", () => {
    expect(isSecretKey("GITHUB_PERSONAL_ACCESS_TOKEN")).toBe(true);
    expect(isSecretKey("Authorization")).toBe(true);
    expect(isSecretKey("CODEX_HOME")).toBe(false);
    expect(isSecretKey("allowed-tools")).toBe(false);
  });
  it("keeps env interpolations visible", () => {
    const interp = ["$", "{GITHUB_PERSONAL_ACCESS_TOKEN}"].join("");
    expect(maskValue(interp)).toBe(interp);
    expect(maskValue("ghp_abcdef")).toBe("••••••••••");
    expect(maskValue("")).toBe("");
  });
});
