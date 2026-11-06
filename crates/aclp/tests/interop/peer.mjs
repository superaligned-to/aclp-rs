import { readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const [mode, sdk, url] = process.argv.slice(2);

const { createClient, createServer, defineResult, discover } = await import(pathToFileURL(join(sdk, "dist/index.js")).href);

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

const client = { name: "interop", version: "1.0.0", harness: { name: "node", version: "26" } };

const discovery = { endpoint: "https://aclp.example.com/events", name: "Interop" };

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
    callbacks[method] = { handle: (_params, ctx) => emit({ received: method, client: ctx.client ?? null }) };
  }

  callbacks["input.submitted"] = {
    result: context,
    handle: ({ content }, ctx) => {
      emit({ received: "input.submitted", client: ctx.client ?? null });
      return { context: [`ts:${content.length}`] };
    },
  };

  callbacks["session.started"] = {
    result: strict,
    handle: (_params, ctx) => {
      emit({ received: "session.started", client: ctx.client ?? null });
      return {};
    },
  };

  const server = createServer(callbacks, {
    authorization,
    onError: quiet,
    hosts: ["aclp.example.com"],
    discovery,
  });

  server.listen(0, "127.0.0.1", () => emit({ port: server.address().port }));
  process.stdin.on("end", () => server.close());
  process.stdin.resume();
} else if (mode === "discover") {
  emit(await discover(url));
} else {
  const sender = createClient({ url, authorization, onError: quiet, client });

  const failures = [];

  for (let start = 0; start < examples.length; start += 16) {
    for (const [method, p] of examples.slice(start, start + 16)) {
      sender.notify(method, p);
    }

    const failed = await failure(() => sender.flush());

    if (failed !== null) {
      failures.push(...failed);
    }
  }

  const flushed = failures.length === 0 ? null : failures;

  const reply = await sender.request("input.submitted", params("input.submitted"), context);

  const invalidResult = await failure(() => sender.request("session.started", params("session.started"), strict));

  const notificationOnly = await failure(() => sender.request("tool.finished", params("tool.finished"), context));

  const intruder = createClient({ url, authorization: "Bearer wrong", onError: quiet });

  intruder.notify("session.ended", params("session.ended"));

  const unauthorized = await failure(() => intruder.flush());

  emit({ flushed, reply, invalidResult, notificationOnly, unauthorized });
}
