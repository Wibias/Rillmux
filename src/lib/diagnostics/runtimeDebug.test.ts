import { describe, expect, it } from "vitest";
import { formatDebugFields } from "./runtimeDebug";

describe("formatDebugFields", () => {
  it("keeps compact primitive correlation fields", () => {
    expect(
      formatDebugFields({ channel: "forsen", session: "abc", ready: true, status: 204 }),
    ).toBe("channel=forsen session=abc ready=true status=204");
  });

  it("drops sensitive, free-form error, or structured fields before IPC", () => {
    const formatted = formatDebugFields({
      token: "oauth-secret",
      cookie: "auth-cookie",
      authorization: "OAuth secret",
      payload: { private: true },
      rewardInput: "do not log this",
      deviceId: "device-secret",
      reason: "request failed https://example.invalid/?oauth=secret",
      errorMessage: "upstream returned private material",
      channel: "forsen\nspoofed",
    });
    expect(formatted).toBe("channel=forsen spoofed");
    expect(formatted).not.toContain("secret");
    expect(formatted).not.toContain("do not log");
    expect(formatted).not.toContain("private material");
  });

  it("redacts claim identifiers and query hashes before IPC", () => {
    const claimId = "abcdef1234567890";
    const queryHash =
      "1530a003a7d374b0380b79db0be0534f30ff46e61cffa2bc0e2468a909fbc024";
    const persistedQueryHash =
      "9988086babc615a918a1e9a722ff41d98847acac822645209ac737e9ecb27152";
    const formatted = formatDebugFields({ claimId, queryHash, persistedQueryHash });

    expect(formatted).toBe(
      "claimId=abcdef…7890 queryHash=1530a003… persistedQueryHash=9988086b…",
    );
    expect(formatted).not.toContain(claimId);
    expect(formatted).not.toContain(queryHash);
    expect(formatted).not.toContain(persistedQueryHash);
  });

  it("redacts snake-case identifier aliases too", () => {
    expect(
      formatDebugFields({
        claim_id: "short-id",
        query_hash: "abcdefgh12345678",
        persisted_query_hash: "87654321abcdefgh",
      }),
    ).toBe(
      "claim_id=*** query_hash=abcdefgh… persisted_query_hash=87654321…",
    );
  });
});
