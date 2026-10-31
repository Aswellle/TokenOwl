import { describe, expect, it } from "vitest";

/**
 * Cross-stack contract test: the TypeScript bridge and the Rust IPC surface
 * must agree on command names, argument keys and event names.
 *
 * `command_registration.rs` already proves Rust commands are registered. This
 * test closes the other two gaps that a typed `invoke()` call cannot see:
 * a renamed command still compiles on both sides, and a camelCase argument
 * that does not match its snake_case Rust parameter fails only at runtime in
 * the packaged app. It parses the two source trees the same way a reviewer
 * would, which is deliberate — there is no generated schema to compare yet.
 *
 * Sources are loaded through Vite's raw globs rather than `node:fs`: the test
 * runs in the same pipeline as the app, needs no Node type definitions, and
 * cannot accidentally read a stale build artifact.
 */
const rustSources = import.meta.glob("/src-tauri/src/**/*.rs", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;
const frontendSources = import.meta.glob(["/src/**/*.ts", "/src/**/*.tsx", "!/src/test/**"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const RUST = Object.values(rustSources).join("\n");
const FRONTEND = Object.values(frontendSources).join("\n");
const APP = rustSources["/src-tauri/src/app/mod.rs"] ?? "";

interface RustCommand {
  name: string;
  params: string[];
}

function rustCommands(source: string): RustCommand[] {
  const lines = source.split("\n");
  const commands: RustCommand[] = [];
  for (let index = 0; index < lines.length; index += 1) {
    if (lines[index].trim() !== "#[tauri::command]") continue;
    // The signature may start on the next line and span several lines.
    let cursor = index + 1;
    while (cursor < lines.length && !lines[cursor].includes("fn ")) cursor += 1;
    if (cursor >= lines.length) continue;
    const name = lines[cursor].split("fn ")[1].trimStart().split(/[<(]/)[0];
    let signature = lines[cursor];
    while (!signature.includes("{") && cursor + 1 < lines.length) {
      cursor += 1;
      signature += ` ${lines[cursor]}`;
    }
    const parameterBlock = signature.slice(signature.indexOf("(") + 1, signature.lastIndexOf(")"));
    const params = parameterBlock
      .split(",")
      .map((parameter) => parameter.trim())
      .filter((parameter) => parameter.length > 0)
      .map((parameter) => parameter.split(":")[0].trim())
      .filter((parameter) => !["state", "app", "handle", "window"].includes(parameter));
    commands.push({ name, params });
  }
  return commands;
}

function invokedCommands(source: string): Array<{ name: string; keys: string[] }> {
  const pattern = /invoke(?:<[^>]*>)?\(\s*"([a-z0-9_]+)"\s*(?:,\s*(\{[\s\S]*?\}))?/g;
  const found: Array<{ name: string; keys: string[] }> = [];
  for (const match of source.matchAll(pattern)) {
    const object = match[2] ?? "";
    // Dynamically assembled argument objects are checked by name only.
    const keys =
      object.includes("Object.keys") || object.includes("params")
        ? []
        : Array.from(object.matchAll(/([a-zA-Z_][a-zA-Z0-9_]*)\s*[:,}]/g)).map(
            (key) => key[1],
          );
    found.push({ name: match[1], keys: Array.from(new Set(keys)) });
  }
  return found;
}

function listenedEvents(source: string): string[] {
  return Array.from(
    source.matchAll(/listen(?:<[^>]*>)?\(\s*"([a-z0-9:_-]+)"/g),
    (match) => match[1],
  );
}

function emittedEvents(source: string): string[] {
  return Array.from(
    source.matchAll(/\.emit(?:_to|_filter)?\(\s*"([a-z0-9:_-]+)"/g),
    (match) => match[1],
  );
}

/** Field names of a Rust struct block (`struct Name { ... }`). */
function rustStructFields(source: string, name: string): string[] {
  const start = source.indexOf(`struct ${name} {`);
  if (start === -1) return [];
  const end = source.indexOf("}", start);
  const block = source.slice(start, end);
  return Array.from(block.matchAll(/^\s*([a-z_][a-z0-9_]*)\s*:/gm), (match) => match[1]);
}

/** Field names of a TypeScript interface block (`interface Name { ... }`). */
function tsInterfaceFields(source: string, name: string): string[] {
  const start = source.indexOf(`interface ${name} {`);
  if (start === -1) return [];
  const end = source.indexOf("}", start);
  const block = source.slice(start, end);
  return Array.from(block.matchAll(/^\s*([a-zA-Z_][a-zA-Z0-9_]*)\??\s*:/gm), (match) => match[1]);
}

const snake = (value: string): string =>
  value.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`);

const registeredBlock = APP.split("generate_handler![")[1]?.split("])")[0] ?? "";

describe("frontend ↔ Rust IPC contract", () => {
  const commands = rustCommands(RUST);
  const commandNames = new Set(commands.map((command) => command.name));
  const invoked = invokedCommands(FRONTEND);

  it("parses a plausible command surface", () => {
    expect(commands.length).toBeGreaterThanOrEqual(15);
    expect(invoked.length).toBeGreaterThanOrEqual(15);
  });

  it("every invoked command exists in Rust and is registered", () => {
    for (const call of invoked) {
      expect(commandNames, `invoke("${call.name}") has no Rust command`).toContain(call.name);
      expect(
        new RegExp(`\\b${call.name}\\b`).test(registeredBlock),
        `invoke("${call.name}") is not in generate_handler!`,
      ).toBe(true);
    }
  });

  it("every invoked argument key maps to a Rust parameter", () => {
    for (const call of invoked) {
      if (call.keys.length === 0) continue;
      const command = commands.find((candidate) => candidate.name === call.name);
      expect(command, `missing Rust command ${call.name}`).toBeDefined();
      for (const key of call.keys) {
        expect(
          command!.params,
          `${call.name} passes "${key}" but Rust declares ${command!.params.join(", ")}`,
        ).toContain(snake(key));
      }
    }
  });

  it("every listened event is emitted by Rust and vice versa", () => {
    const listened = new Set(listenedEvents(FRONTEND));
    const emitted = new Set(emittedEvents(RUST));
    expect(listened.size).toBeGreaterThanOrEqual(5);
    for (const event of listened) {
      expect(emitted, `frontend listens for "${event}" but Rust never emits it`).toContain(event);
    }
    for (const event of emitted) {
      expect(listened, `Rust emits "${event}" but no frontend listener exists`).toContain(event);
    }
  });

  it("the morph handoff events carry the same anchor fields on both sides", () => {
    // The window morph's whole contract is this payload: Rust serializes
    // `MorphAnchor` (camelCase), the webviews read `MorphAnchorPayload`.
    // A renamed field would silently deliver `undefined` anchors.
    const emitted = emittedEvents(APP);
    for (const event of ["window:morph-to-bubble", "window:morph-from-bubble"]) {
      expect(emitted, `Rust must emit ${event}`).toContain(event);
      expect(
        listenedEvents(FRONTEND),
        `the webviews must listen for ${event}`,
      ).toContain(event);
    }
    const rustFields = rustStructFields(APP, "MorphAnchor");
    expect(rustFields).toContain("x");
    expect(rustFields).toContain("y");
    expect(rustFields).toContain("diameter");
    const morphTs = Object.entries(frontendSources)
      .filter(([path]) => path.endsWith("lib/morph.ts"))
      .map(([, source]) => source)
      .join("\n");
    const tsFields = tsInterfaceFields(morphTs, "MorphAnchorPayload");
    expect(tsFields).toEqual(rustFields);
    // The Rust side must keep the camelCase wire casing the TS side reads.
    expect(APP).toMatch(/#\[serde\(rename_all = "camelCase"\)\]\s*struct MorphAnchor/);
  });
});
