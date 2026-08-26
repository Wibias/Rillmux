import { readdir, readFile } from "node:fs/promises";

const assets = await readdir("dist/assets");
const sentryChunks = assets.filter((name) => /^sentry-.*\.js$/.test(name));

if (sentryChunks.length === 0) {
  throw new Error("Expected a separate Sentry JS chunk in a DSN-enabled build");
}

const html = await readFile("dist/index.html", "utf8");
for (const chunk of sentryChunks) {
  if (html.includes(chunk)) {
    throw new Error(`Sentry chunk ${chunk} is eagerly referenced by index.html`);
  }
}

console.log(`Verified lazy Sentry chunk(s): ${sentryChunks.join(", ")}`);
