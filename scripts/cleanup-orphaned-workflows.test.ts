import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";

type Route = {
  method?: string;
  path: string;
  status?: number;
  body?: unknown;
};

type CleanupResult = {
  totalWorkflowRecords: number;
  activeWorkflowFiles: number;
  orphanCandidates: number;
  approvedWorkflows: number;
  plannedRuns: number;
  deletedRuns: number;
  capped: boolean;
  defaultBranchGeneration: string;
};

type CleanupModule = {
  cleanupOrphanedWorkflowRuns(options: {
    token: string;
    repository: string;
    fetchImpl: typeof fetch;
    maxDeletions?: number;
    log: (message: string) => void;
  }): Promise<CleanupResult>;
};

const REPOSITORY = "Wibias/Rillmux";
const SHA_A = "a".repeat(40);
const SHA_B = "b".repeat(40);

async function loadCleanup(): Promise<CleanupModule> {
  return import("./cleanup-orphaned-workflows.mjs") as Promise<CleanupModule>;
}

function mockFetch(routes: Route[], calls: string[]): typeof fetch {
  return (async (input: URL | RequestInfo, init: RequestInit = {}) => {
    const url = new URL(typeof input === "string" ? input : input.url);
    const method = init.method ?? "GET";
    const key = `${method} ${url.pathname}${url.search}`;
    calls.push(key);

    const index = routes.findIndex(
      (route) =>
        (route.method ?? "GET") === method &&
        route.path === `${url.pathname}${url.search}`,
    );
    if (index < 0) {
      return new Response(`Unexpected request: ${key}`, { status: 500 });
    }

    const [route] = routes.splice(index, 1);
    const status = route.status ?? 200;
    if (status === 204) return new Response(null, { status });
    return Response.json(route.body ?? {}, { status });
  }) as typeof fetch;
}

function baseRoutes(extra: Route[]): Route[] {
  return [
    { path: "/repos/Wibias/Rillmux", body: { default_branch: "main" } },
    {
      path: "/repos/Wibias/Rillmux/git/ref/heads/main",
      body: { object: { sha: SHA_A } },
    },
    {
      path: "/repos/Wibias/Rillmux/contents/.github/workflows?ref=main",
      body: [
        { type: "file", path: ".github/workflows/ci.yml" },
        { type: "file", path: ".github/workflows/codeql.yml" },
        { type: "file", path: ".github/workflows/release.yml" },
        {
          type: "file",
          path: ".github/workflows/cleanup-orphaned-workflows.yml",
        },
      ],
    },
    ...extra,
  ];
}

async function runWith(routes: Route[], maxDeletions?: number) {
  const calls: string[] = [];
  const { cleanupOrphanedWorkflowRuns } = await loadCleanup();
  const result = await cleanupOrphanedWorkflowRuns({
    token: "test-token",
    repository: REPOSITORY,
    fetchImpl: mockFetch(routes, calls),
    maxDeletions,
    log: () => undefined,
  });
  return { result, calls };
}

describe("orphaned workflow cleanup workflow", () => {
  it("is scheduled, least-privilege, bounded, pinned, and default-branch-only", async () => {
    const source = await readFile(
      new URL("../.github/workflows/cleanup-orphaned-workflows.yml", import.meta.url),
      "utf8",
    );

    expect(source).toMatch(/schedule:\s*\n\s*- cron: "0 6 \* \* 1"/);
    expect(source).toMatch(/push:\s*\n\s*branches:\s*\n\s*- main/);
    expect(source).not.toMatch(/workflow_dispatch/);
    expect(source).toMatch(/permissions:\s*\n\s*actions: write\s*\n\s*contents: read/);
    expect(source).toMatch(/timeout-minutes: 10/);
    expect(source).toContain(
      "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1",
    );
    expect(source).toContain(
      "actions/setup-node@820762786026740c76f36085b0efc47a31fe5020",
    );
    expect(source).toContain("persist-credentials: false");
    expect(source).toMatch(/node-version: 24/);
    expect(source).toContain("run: node scripts/cleanup-orphaned-workflows.mjs");
  });
});

describe("cleanupOrphanedWorkflowRuns", () => {
  it("deletes only local workflow histories absent from main", async () => {
    const routes = baseRoutes([
      {
        path: "/repos/Wibias/Rillmux/actions/workflows?per_page=100&page=1",
        body: {
          workflows: [
            { id: 1, name: "CI", path: ".github/workflows/ci.yml" },
            {
              id: 2,
              name: "Temporary helper",
              path: ".github/workflows/tmp-helper.yml",
            },
            {
              id: 3,
              name: "Dependabot",
              path: "dynamic/dependabot/dependabot-updates",
            },
          ],
        },
      },
      {
        path: "/repos/Wibias/Rillmux/actions/workflows/2/runs?per_page=100&page=1",
        body: { workflow_runs: [{ id: 201, status: "completed" }] },
      },
      {
        path: "/repos/Wibias/Rillmux/git/ref/heads/main",
        body: { object: { sha: SHA_A } },
      },
      {
        method: "DELETE",
        path: "/repos/Wibias/Rillmux/actions/runs/201",
        status: 204,
      },
    ]);

    const { result, calls } = await runWith(routes);
    expect(result.orphanCandidates).toBe(1);
    expect(result.deletedRuns).toBe(1);
    expect(calls.some((call) => call.includes("dynamic/dependabot"))).toBe(false);
    expect(routes).toHaveLength(0);
  });

  it("preserves an orphan while a live run branch still contains the workflow", async () => {
    const headSha = "c".repeat(40);
    const routes = baseRoutes([
      {
        path: "/repos/Wibias/Rillmux/actions/workflows?per_page=100&page=1",
        body: {
          workflows: [
            {
              id: 2,
              name: "Temporary helper",
              path: ".github/workflows/tmp-helper.yml",
            },
          ],
        },
      },
      {
        path: "/repos/Wibias/Rillmux/actions/workflows/2/runs?per_page=100&page=1",
        body: {
          workflow_runs: [
            {
              id: 202,
              status: "completed",
              head_branch: "feature/live-helper",
              head_repository: { full_name: REPOSITORY },
            },
          ],
        },
      },
      {
        path: "/repos/Wibias/Rillmux/git/ref/heads/feature/live-helper",
        body: { object: { sha: headSha } },
      },
      {
        path: `/repos/Wibias/Rillmux/contents/.github/workflows/tmp-helper.yml?ref=${headSha}`,
        body: { type: "file", path: ".github/workflows/tmp-helper.yml" },
      },
    ]);

    const { result, calls } = await runWith(routes);
    expect(result.approvedWorkflows).toBe(0);
    expect(result.deletedRuns).toBe(0);
    expect(calls.some((call) => call.startsWith("DELETE "))).toBe(false);
    expect(routes).toHaveLength(0);
  });

  it("ignores an already-deleted historical run branch during preflight", async () => {
    const routes = baseRoutes([
      {
        path: "/repos/Wibias/Rillmux/actions/workflows?per_page=100&page=1",
        body: {
          workflows: [
            {
              id: 2,
              name: "Temporary helper",
              path: ".github/workflows/tmp-helper.yml",
            },
          ],
        },
      },
      {
        path: "/repos/Wibias/Rillmux/actions/workflows/2/runs?per_page=100&page=1",
        body: {
          workflow_runs: [
            {
              id: 208,
              status: "completed",
              head_branch: "feature/deleted-helper",
              head_repository: { full_name: REPOSITORY },
            },
          ],
        },
      },
      {
        path: "/repos/Wibias/Rillmux/git/ref/heads/feature/deleted-helper",
        status: 404,
        body: { message: "Not Found" },
      },
      {
        path: "/repos/Wibias/Rillmux/git/ref/heads/main",
        body: { object: { sha: SHA_A } },
      },
      {
        method: "DELETE",
        path: "/repos/Wibias/Rillmux/actions/runs/208",
        status: 204,
      },
    ]);

    const { result, calls } = await runWith(routes);
    expect(result.approvedWorkflows).toBe(1);
    expect(result.deletedRuns).toBe(1);
    expect(calls).toContain("DELETE /repos/Wibias/Rillmux/actions/runs/208");
    expect(routes).toHaveLength(0);
  });

  it("never deletes an orphan with a non-completed run", async () => {
    const routes = baseRoutes([
      {
        path: "/repos/Wibias/Rillmux/actions/workflows?per_page=100&page=1",
        body: {
          workflows: [
            {
              id: 2,
              name: "Temporary helper",
              path: ".github/workflows/tmp-helper.yml",
            },
          ],
        },
      },
      {
        path: "/repos/Wibias/Rillmux/actions/workflows/2/runs?per_page=100&page=1",
        body: { workflow_runs: [{ id: 203, status: "in_progress" }] },
      },
    ]);

    const { result, calls } = await runWith(routes);
    expect(result.approvedWorkflows).toBe(0);
    expect(result.deletedRuns).toBe(0);
    expect(calls.some((call) => call.startsWith("DELETE "))).toBe(false);
  });

  it("preflights every candidate before the first destructive request", async () => {
    const routes = baseRoutes([
      {
        path: "/repos/Wibias/Rillmux/actions/workflows?per_page=100&page=1",
        body: {
          workflows: [
            { id: 2, name: "Old A", path: ".github/workflows/tmp-a.yml" },
            { id: 3, name: "Old B", path: ".github/workflows/tmp-b.yml" },
          ],
        },
      },
      {
        path: "/repos/Wibias/Rillmux/actions/workflows/2/runs?per_page=100&page=1",
        body: { workflow_runs: [{ id: 204, status: "completed" }] },
      },
      {
        path: "/repos/Wibias/Rillmux/actions/workflows/3/runs?per_page=100&page=1",
        status: 503,
        body: { message: "Service unavailable" },
      },
    ]);
    const calls: string[] = [];
    const { cleanupOrphanedWorkflowRuns } = await loadCleanup();

    await expect(
      cleanupOrphanedWorkflowRuns({
        token: "test-token",
        repository: REPOSITORY,
        fetchImpl: mockFetch(routes, calls),
        log: () => undefined,
      }),
    ).rejects.toThrow(/HTTP 503/);
    expect(calls.some((call) => call.startsWith("DELETE "))).toBe(false);
  });

  it("caps deletions and clears smaller histories first", async () => {
    const routes = baseRoutes([
      {
        path: "/repos/Wibias/Rillmux/actions/workflows?per_page=100&page=1",
        body: {
          workflows: [
            { id: 2, name: "Large", path: ".github/workflows/tmp-large.yml" },
            { id: 3, name: "Small", path: ".github/workflows/tmp-small.yml" },
          ],
        },
      },
      {
        path: "/repos/Wibias/Rillmux/actions/workflows/2/runs?per_page=100&page=1",
        body: {
          workflow_runs: [
            { id: 205, status: "completed" },
            { id: 206, status: "completed" },
          ],
        },
      },
      {
        path: "/repos/Wibias/Rillmux/actions/workflows/3/runs?per_page=100&page=1",
        body: { workflow_runs: [{ id: 207, status: "completed" }] },
      },
      {
        path: "/repos/Wibias/Rillmux/git/ref/heads/main",
        body: { object: { sha: SHA_A } },
      },
      {
        method: "DELETE",
        path: "/repos/Wibias/Rillmux/actions/runs/207",
        status: 204,
      },
    ]);

    const { result, calls } = await runWith(routes, 1);
    expect(result.deletedRuns).toBe(1);
    expect(result.capped).toBe(true);
    expect(calls.at(-1)).toBe("DELETE /repos/Wibias/Rillmux/actions/runs/207");
  });

  it("stops later deletions if main moves during cleanup", async () => {
    const routes: Route[] = [
      { path: "/repos/Wibias/Rillmux", body: { default_branch: "main" } },
      {
        path: "/repos/Wibias/Rillmux/git/ref/heads/main",
        body: { object: { sha: SHA_A } },
      },
      {
        path: "/repos/Wibias/Rillmux/contents/.github/workflows?ref=main",
        body: [
          { type: "file", path: ".github/workflows/ci.yml" },
          {
            type: "file",
            path: ".github/workflows/cleanup-orphaned-workflows.yml",
          },
        ],
      },
      {
        path: "/repos/Wibias/Rillmux/actions/workflows?per_page=100&page=1",
        body: {
          workflows: [
            { id: 2, name: "Old", path: ".github/workflows/tmp.yml" },
          ],
        },
      },
      {
        path: "/repos/Wibias/Rillmux/actions/workflows/2/runs?per_page=100&page=1",
        body: {
          workflow_runs: [
            { id: 301, status: "completed" },
            { id: 302, status: "completed" },
          ],
        },
      },
      {
        path: "/repos/Wibias/Rillmux/git/ref/heads/main",
        body: { object: { sha: SHA_A } },
      },
      {
        method: "DELETE",
        path: "/repos/Wibias/Rillmux/actions/runs/301",
        status: 204,
      },
      {
        path: "/repos/Wibias/Rillmux/git/ref/heads/main",
        body: { object: { sha: SHA_B } },
      },
    ];
    const calls: string[] = [];
    const { cleanupOrphanedWorkflowRuns } = await loadCleanup();

    await expect(
      cleanupOrphanedWorkflowRuns({
        token: "test-token",
        repository: REPOSITORY,
        fetchImpl: mockFetch(routes, calls),
        log: () => undefined,
      }),
    ).rejects.toThrow(/default_branch_moved_during_cleanup/);
    expect(calls.filter((call) => call.startsWith("DELETE "))).toHaveLength(1);
    expect(calls).not.toContain("DELETE /repos/Wibias/Rillmux/actions/runs/302");
  });

  it("fails closed if a captured run branch disappears during cleanup", async () => {
    const headSha = "d".repeat(40);
    const routes = baseRoutes([
      {
        path: "/repos/Wibias/Rillmux/actions/workflows?per_page=100&page=1",
        body: {
          workflows: [
            {
              id: 2,
              name: "Temporary helper",
              path: ".github/workflows/tmp-helper.yml",
            },
          ],
        },
      },
      {
        path: "/repos/Wibias/Rillmux/actions/workflows/2/runs?per_page=100&page=1",
        body: {
          workflow_runs: [
            {
              id: 303,
              status: "completed",
              head_branch: "feature/racy-helper",
              head_repository: { full_name: REPOSITORY },
            },
          ],
        },
      },
      {
        path: "/repos/Wibias/Rillmux/git/ref/heads/feature/racy-helper",
        body: { object: { sha: headSha } },
      },
      {
        path: `/repos/Wibias/Rillmux/contents/.github/workflows/tmp-helper.yml?ref=${headSha}`,
        status: 404,
        body: { message: "Not Found" },
      },
      {
        path: "/repos/Wibias/Rillmux/git/ref/heads/main",
        body: { object: { sha: SHA_A } },
      },
      {
        path: "/repos/Wibias/Rillmux/git/ref/heads/feature/racy-helper",
        status: 404,
        body: { message: "Not Found" },
      },
    ]);
    const calls: string[] = [];
    const { cleanupOrphanedWorkflowRuns } = await loadCleanup();

    await expect(
      cleanupOrphanedWorkflowRuns({
        token: "test-token",
        repository: REPOSITORY,
        fetchImpl: mockFetch(routes, calls),
        log: () => undefined,
      }),
    ).rejects.toThrow(/HTTP 404/);
    expect(calls.some((call) => call.startsWith("DELETE "))).toBe(false);
  });
});
