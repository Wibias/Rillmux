import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

function blockBody(source: string, signature: string): string {
  const start = source.indexOf(signature);
  if (start < 0) throw new Error(`missing block: ${signature}`);
  const open = source.indexOf("{", start);
  if (open < 0) throw new Error(`missing block body: ${signature}`);
  let depth = 0;
  for (let i = open; i < source.length; i += 1) {
    if (source[i] === "{") depth += 1;
    if (source[i] === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(open + 1, i);
    }
  }
  throw new Error(`unterminated block: ${signature}`);
}

describe("prediction initial hydration", () => {
  test("uses Twitch's verified ChannelPointsPredictionContext snapshot operation", () => {
    const operations = readFileSync(
      "src-tauri/src/twitch_gql_operations.rs",
      "utf8",
    );
    const channelPoints = readFileSync("src-tauri/src/channel_points.rs", "utf8");

    expect(operations).toContain("CHANNEL_POINTS_PREDICTION_CONTEXT_HASH");
    expect(operations).toContain(
      "beb846598256b75bd7c1fe54a80431335996153e358ca9c7837ce7bb83d7d383",
    );
    expect(channelPoints).toContain("fn prediction_context_payload(");

    const fetchPrediction = blockBody(channelPoints, "async fn fetch_prediction(");
    expect(fetchPrediction).toContain("prediction_context_payload(");
  });

  test("parses active and locked predictions from the community channel snapshot", () => {
    const source = readFileSync("src-tauri/src/channel_points.rs", "utf8");
    const parsePrediction = blockBody(source, "fn parse_prediction(body: &Value)");

    expect(parsePrediction).toContain(
      '"/data/community/channel/activePredictionEvents"',
    );
    expect(parsePrediction).toContain(
      '"/data/community/channel/lockedPredictionEvents"',
    );
  });
});
