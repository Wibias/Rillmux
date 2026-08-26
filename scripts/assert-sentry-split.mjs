import { readdir, readFile } from "node:fs/promises";

const assets = await readdir("dist/assets");
const sentryChunks = assets.filter((name) => /^sentry-sdk-.*\.js$/.test(name));

if (sentryChunks.length === 0) {
  throw new Error("Expected a separate Sentry SDK JS chunk in a DSN-enabled build");
}

const html = await readFile("dist/index.html", "utf8");
for (const chunk of sentryChunks) {
  if (html.includes(chunk)) {
    throw new Error(`Sentry SDK chunk ${chunk} is eagerly referenced by index.html`);
  }
}

console.log(`Verified lazy Sentry SDK chunk(s): ${sentryChunks.join(", ")}`);
