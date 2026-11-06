import { readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const [mode, sdk, url] = process.argv.slice(2);

const { createClient, createServer, defineResult } = await import(pathToFileURL(join(sdk, "dist/index.js")).href);

const examples = JSON.parse(readFileSync(new URL("../fixtures/event-examples.json", import.meta.url), "utf8"));

const params = (method) => examples.find(([m]) => m === method)[1];

const context = defineResult({
  type: "object",
  properties: { context: { type: "array", items: { type: "string" } } },
  required: ["context"],
  additionalProperties: false,
});

const strict = defineResult({
  type: "object",
  properties: { ok: { type: "boolean" } },
  required: ["ok"],
});

const authorization = "Bearer interop";

const emit = (val) => process.stdout.write(`${JSON.stringify(val)}\n`);

const quiet = () => {};

const failure = async (fn) => {
  try {
    await fn();
    return null;
  } catch (err) {
    return err instanceof AggregateError ? err.errors.map((e) => e.message) : err.message;
  }
};

if (mode === "server") {
  const callbacks = {};

  for (const [method] of examples) {
    callbacks[method] = { handle: () => emit({ received: method }) };
  }

  callbacks["input.submitted"] = {
    result: context,
    handle: ({ content }) => {
      emit({ received: "input.submitted" });
      return { context: [`ts:${content.length}`] };
    },
  };

  callbacks["session.started"] = {
    result: strict,
    handle: () => {
      emit({ received: "session.started" });
      return {};
    },
  };

  const server = createServer(callbacks, { authorization, onError: quiet });

  server.listen(0, "127.0.0.1", () => emit({ port: server.address().port }));
  process.stdin.on("end", () => server.close());
  process.stdin.resume();
} else {
  const client = createClient({ url, authorization, onError: quiet });

  for (const [method, p] of examples) {
    client.notify(method, p);
  }

  const flushed = await failure(() => client.flush());

  const reply = await client.request("input.submitted", params("input.submitted"), context);

  const invalidResult = await failure(() => client.request("session.started", params("session.started"), strict));

  const notificationOnly = await failure(() => client.request("tool.finished", params("tool.finished"), context));

  const intruder = createClient({ url, authorization: "Bearer wrong", onError: quiet });

  intruder.notify("session.ended", params("session.ended"));

  const unauthorized = await failure(() => intruder.flush());

  emit({ flushed, reply, invalidResult, notificationOnly, unauthorized });
}
