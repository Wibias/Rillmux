import { describe, expect, it } from "vitest";
import { formatDebugFields } from "./runtimeDebug";

describe("formatDebugFields", () => {
  it("keeps compact primitive correlation fields", () => {
    expect(
      formatDebugFields({ channel: "forsen", session: "abc", ready: true, status: 204 }),
    ).toBe("channel=forsen session=abc ready=true status=204");
  });

  it("drops sensitive or structured fields before IPC", () => {
    const formatted = formatDebugFields({
      token: "oauth-secret",
      cookie: "auth-cookie",
      authorization: "OAuth secret",
      payload: { private: true },
      rewardInput: "do not log this",
      deviceId: "device-secret",
      channel: "forsen\nspoofed",
    });
    expect(formatted).toBe("channel=forsen spoofed");
    expect(formatted).not.toContain("secret");
    expect(formatted).not.toContain("do not log");
  });
});
