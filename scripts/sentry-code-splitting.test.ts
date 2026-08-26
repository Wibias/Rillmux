import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";

async function readRepo(path: string): Promise<string> {
  return readFile(new URL(`../${path}`, import.meta.url), "utf8");
}

describe("Sentry code splitting", () => {
  it("keeps the Sentry SDK out of the initial frontend graph", async () => {
    const source = await readRepo("src/lib/sentry.tsx");

    expect(source).not.toMatch(/import\s+\*\s+as\s+Sentry\s+from\s+["']@sentry\/react["']/);
    expect(source).not.toMatch(/from\s+["']@sentry\/react["']/);
    expect(source).toContain('import("@sentry/react")');
  });

  it("uses the local error boundary instead of statically importing Sentry in App", async () => {
    const source = await readRepo("src/App.tsx");

    expect(source).not.toMatch(/from\s+["']@sentry\/react["']/);
    expect(source).not.toContain("<Sentry.ErrorBoundary");
    expect(source).toContain("<AppErrorBoundary");
  });

  it("does not load the SDK just to keep telemetry disabled", async () => {
    const source = await readRepo("src/lib/sentry.tsx");

    expect(source).toContain("if (!sdkPromise) return;");
    expect(source).toContain("loadSentrySdk");
  });
});
