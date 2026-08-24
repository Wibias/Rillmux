import { describe, expect, it } from "vitest";
import {
  chatterinoShouldCloseOnEmpty,
  chatterinoShouldSkipSync,
  chatterinoSyncKey,
} from "./chatterinoSync";

describe("chatterinoSyncKey", () => {
  it("sorts and dedupes so slot order does not relaunch", () => {
    expect(chatterinoSyncKey(["Forsen", "xqc"])).toBe("forsen,xqc");
    expect(chatterinoSyncKey(["xqc", "forsen"])).toBe("forsen,xqc");
    expect(chatterinoSyncKey(["Forsen", "forsen", ""])).toBe("forsen");
  });
});

describe("chatterinoShouldSkipSync", () => {
  it("skips an in-flight open and a successful same-channel open", () => {
    expect(chatterinoShouldSkipSync("forsen", "forsen", "")).toBe(true);
    expect(chatterinoShouldSkipSync("forsen", "", "forsen")).toBe(true);
    expect(chatterinoShouldSkipSync("forsen,xqc", "forsen", "")).toBe(false);
    expect(chatterinoShouldSkipSync("", "forsen", "")).toBe(false);
  });
});

describe("chatterinoShouldCloseOnEmpty", () => {
  it("does not close while an open is in flight", () => {
    expect(chatterinoShouldCloseOnEmpty("forsen")).toBe(false);
    expect(chatterinoShouldCloseOnEmpty("")).toBe(true);
  });

  it("closes when the last stream is gone even if an open is in flight", () => {
    expect(chatterinoShouldCloseOnEmpty("forsen", false)).toBe(true);
  });
});
