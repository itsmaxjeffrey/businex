// Runs inside the isolated build container as "node assets/build.mjs".
// Stage 1: type-check and compile the project with the pinned compiler.
// Stage 2: execute the app's own handler against a fixture request.
// Every result is written to /workspace/out/result.json; the process
// always exits 0 so the runner reads structured output instead of
// scraping logs.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";

const OUT = "/workspace/out";

mkdirSync(OUT + "/dist", { recursive: true });

function finish(result) {
  writeFileSync(OUT + "/result.json", JSON.stringify(result));
  process.exit(0);
}

function parseDiagnostics(text) {
  const found = [];
  for (const line of text.split("\n")) {
    const match = /^(.+?)\((\d+),(\d+)\): error (TS\d+): (.*)$/.exec(line.trim());
    if (match) {
      found.push({
        file: match[1],
        line: Number(match[2]),
        column: Number(match[3]),
        code: match[4],
        message: match[5],
      });
    }
  }
  return found;
}

function makeStub(records) {
  const store = records.map(function (record, index) {
    return {
      id: record.id || "rec-" + (index + 1),
      entity: record.entity || "item",
      data: record.data || record,
    };
  });
  return {
    records: {
      async list(entity) {
        return store.filter(function (record) { return record.entity === entity; });
      },
      async get(entity, id) {
        return store.find(function (record) {
          return record.entity === entity && record.id === id;
        });
      },
      async create(entity, data) {
        const record = { id: "rec-" + (store.length + 1), entity: entity, data: data };
        store.push(record);
        return record;
      },
      async update(entity, id, data) {
        const record = store.find(function (item) {
          return item.entity === entity && item.id === id;
        });
        if (record) {
          record.data = data;
        }
        return record;
      },
      async remove(entity, id) {
        const index = store.findIndex(function (item) {
          return item.entity === entity && item.id === id;
        });
        if (index >= 0) {
          store.splice(index, 1);
        }
      },
    },
    log() {},
  };
}

const tsc = spawnSync("tsc", ["-p", "/workspace/assets/tsconfig.json"], {
  encoding: "utf8",
});
const log = ((tsc.stdout || "") + (tsc.stderr || "")).trim();
if (tsc.status !== 0) {
  const diagnostics = parseDiagnostics(log);
  if (diagnostics.length === 0) {
    finish({ ok: false, stage: "compile", diagnostics: [], log: log.slice(0, 4000) });
  }
  finish({ ok: false, stage: "compile", diagnostics: diagnostics, log: "" });
}

writeFileSync(OUT + "/dist/package.json", JSON.stringify({ type: "module" }));

if (!existsSync("/workspace/assets/input.json")) {
  finish({ ok: true, stage: "compiled", diagnostics: [], run: null });
}

// Execution stage: the app's own logic runs here, against fixture records
// through the same SDK shape the platform runtime provides.
const run = {};
try {
  const input = JSON.parse(readFileSync("/workspace/assets/input.json", "utf8"));
  globalThis.businex = makeStub(input.records || []);
  const mod = await import(OUT + "/dist/app.js");
  const app = mod.default;
  if (!app || typeof app.handle !== "function") {
    throw new Error("app.ts must default-export an object with a handle(req) function");
  }
  run.ok = true;
  run.response = await app.handle(input.request);
  run.error = null;
} catch (error) {
  run.ok = false;
  run.response = null;
  run.error = String((error && error.message) || error);
}
finish({ ok: true, stage: "done", diagnostics: [], run: run });
