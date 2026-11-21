// A local HTTP server for the end-to-end tests that send requests. It runs
// in the WDIO worker on 127.0.0.1 with a free port, and keeps what reached
// it, so a spec can tell whether a request was sent at all.

import { mkdirSync, writeFileSync } from "node:fs";
import {
  createServer,
  type IncomingMessage,
  type OutgoingHttpHeaders,
  type ServerResponse,
} from "node:http";
import type { AddressInfo } from "node:net";
import { dirname, join } from "node:path";
import { writeProject } from "./helpers.js";

/** A request that reached the server. */
export interface Received {
  method: string;
  /** The path without the query. */
  path: string;
  headers: Record<string, string>;
  /** The body bytes read so far: all of them once it was answered. */
  bodyBytes: number;
}

export interface TestServer {
  /** `path` on this server, such as `/json`. */
  url(path: string): string;
  /** The requests that reached it, in order; those to `path` if given. */
  received(path?: string): Received[];
  /** The requests to `path` whose connection closed before the answer. */
  aborted(path: string): number;
  stop(): Promise<void>;
}

/** The shipment of the Requests design. */
export const SHIPMENT_JSON =
  '{"id":"shp_8KQ2M4","status":"in_transit","origin":{"city":"Jakarta","hub":"CGK-02"},"destination":{"city":"Singapore","hub":"SIN-01"},"carrier":"Pacific Freight","weight_kg":12.4,"insured":true,"eta":"2026-11-05T09:30:00Z","events":[{"at":"2026-11-03T02:14:00Z","code":"picked_up"},{"at":"2026-11-03T08:40:00Z","code":"departed_hub"}]}';

/** The same shipment as nested XML, on one line. */
export const SHIPMENT_XML =
  '<?xml version="1.0" encoding="UTF-8"?>' +
  '<shipment id="shp_8KQ2M4"><status>in_transit</status>' +
  "<origin><city>Jakarta</city><hub>CGK-02</hub></origin>" +
  "<destination><city>Singapore</city><hub>SIN-01</hub></destination>" +
  "<events>" +
  '<event code="picked_up"><at>2026-11-03T02:14:00Z</at></event>' +
  '<event code="departed_hub"><at>2026-11-03T08:40:00Z</at></event>' +
  "</events></shipment>";

/** A page that would reach the server again if its script or image ran. */
export const HIT_HTML =
  "<p>hi</p><script>fetch('/hit')</script><img src=\"/hit-img\">";

const PIXEL_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
  "base64",
);

/** How fast `/upload` reads a body, about 20 MiB/s. */
const UPLOAD_BYTES_PER_SECOND = 20 * 1024 * 1024;

/** The largest body `/json-size` and `/json-line-size` make. */
const MAX_GENERATED = 256 * 1024 * 1024;

/** The width an element of a generated JSON array aims for. */
const ITEM_WIDTH = 64;

/** A PDF of one page that says `Hello PDF`, with its offsets right. */
function helloPdf(): Buffer {
  const text = "BT /F1 24 Tf 40 50 Td (Hello PDF) Tj ET";
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 120] " +
      "/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    `<< /Length ${text.length} >>\nstream\n${text}\nendstream`,
  ];
  let pdf = "%PDF-1.4\n";
  const offsets: number[] = [];
  objects.forEach((body, i) => {
    offsets.push(pdf.length);
    pdf += `${i + 1} 0 obj\n${body}\nendobj\n`;
  });
  const xref = pdf.length;
  pdf += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  for (const offset of offsets)
    pdf += `${String(offset).padStart(10, "0")} 00000 n \n`;
  pdf += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\n`;
  pdf += `startxref\n${xref}\n%%EOF\n`;
  return Buffer.from(pdf, "latin1");
}

const HELLO_PDF = helloPdf();

/**
 * A JSON array of exactly `bytes` bytes: one element per line, or all on
 * one line. Too small for an element, it is an empty array padded with
 * spaces.
 */
function jsonArray(bytes: number, oneLine: boolean): string {
  const open = oneLine ? "[" : "[\n";
  const close = oneLine ? "]" : "\n]";
  const sep = oneLine ? "," : ",\n";
  const item = (i: number, pad: number) =>
    `{"i":${i},"s":"${"x".repeat(pad)}"}`;
  let budget = bytes - open.length - close.length;
  if (budget < item(0, 0).length) return `[${" ".repeat(bytes - 2)}]`;
  const items: string[] = [];
  for (let i = 0; budget > 0; i++) {
    const bare = (i === 0 ? 0 : sep.length) + item(i, 0).length;
    let pad = Math.max(0, Math.min(ITEM_WIDTH - bare, budget - bare));
    let rest = budget - bare - pad;
    // What another element could not fit goes into this one.
    if (rest > 0 && rest < sep.length + item(i + 1, 0).length) {
      pad += rest;
      rest = 0;
    }
    items.push(item(i, pad));
    budget = rest;
  }
  return open + items.join(sep) + close;
}

function flatHeaders(req: IncomingMessage): Record<string, string> {
  const headers: Record<string, string> = {};
  for (const [name, value] of Object.entries(req.headers))
    if (value !== undefined)
      headers[name] = Array.isArray(value) ? value.join(", ") : value;
  return headers;
}

function reply(
  res: ServerResponse,
  status: number,
  body: string | Buffer = "",
  headers: OutgoingHttpHeaders = {},
): void {
  res.writeHead(status, {
    ...headers,
    "Content-Length": Buffer.byteLength(body),
  });
  res.end(body);
}

function json(res: ServerResponse, value: unknown): void {
  reply(res, 200, JSON.stringify(value), {
    "Content-Type": "application/json",
  });
}

/** A whole number from a path, or null. */
function count(text: string | undefined): number | null {
  const n = Number(text);
  return Number.isSafeInteger(n) && n >= 0 ? n : null;
}

/**
 * Answers a request once its body was read. `onClose` takes what stops a
 * timer when the connection closes first.
 */
function route(
  url: URL,
  res: ServerResponse,
  record: Received,
  onClose: (stop: () => void) => void,
): void {
  const path = url.pathname;
  const parts = path.split("/").slice(1);
  switch (path) {
    case "/json":
      return reply(res, 200, SHIPMENT_JSON, {
        "Content-Type": "application/json",
      });
    case "/xml":
      return reply(res, 200, SHIPMENT_XML, {
        "Content-Type": "application/xml",
      });
    case "/html":
      return reply(res, 200, HIT_HTML, {
        "Content-Type": "text/html; charset=utf-8",
      });
    case "/image.png":
      return reply(res, 200, PIXEL_PNG, { "Content-Type": "image/png" });
    case "/doc.pdf":
      return reply(res, 200, HELLO_PDF, { "Content-Type": "application/pdf" });
    case "/cookies/set": {
      const cookies = [...url.searchParams].map(
        ([name, value]) => `${name}=${value}; Path=/`,
      );
      return reply(
        res,
        200,
        JSON.stringify({ set: Object.fromEntries(url.searchParams) }),
        { "Content-Type": "application/json", "Set-Cookie": cookies },
      );
    }
    case "/echo":
      return json(res, {
        headers: record.headers,
        bodyBytes: record.bodyBytes,
      });
    // An empty answer, so the response view stays light while it is timed.
    case "/upload":
      return reply(res, 200);
    case "/hit":
    case "/hit-img":
      return reply(res, 204);
  }
  const [name, first, second] = parts;
  if (name === "delay" && parts.length === 2) {
    const ms = count(first);
    if (ms === null) return reply(res, 400);
    const timer = setTimeout(() => json(res, { delayedMs: ms }), ms);
    return onClose(() => clearTimeout(timer));
  }
  if (name === "sse" && parts.length === 3) {
    const events = count(first);
    const gapMs = count(second);
    if (events === null || gapMs === null) return reply(res, 400);
    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
    });
    res.flushHeaders();
    let sent = 0;
    let timer: NodeJS.Timeout | undefined;
    const tick = () => {
      if (sent >= events) {
        res.end();
        return;
      }
      sent++;
      res.write(`id: ${sent}\nevent: tick\ndata: ${sent}\n\n`);
      timer = setTimeout(tick, gapMs);
    };
    timer = setTimeout(tick, gapMs);
    return onClose(() => clearTimeout(timer));
  }
  if (
    (name === "json-size" || name === "json-line-size") &&
    parts.length === 2
  ) {
    const bytes = count(first);
    if (bytes === null || bytes < 2 || bytes > MAX_GENERATED)
      return reply(res, 400);
    return reply(res, 200, jsonArray(bytes, name === "json-line-size"), {
      "Content-Type": "application/json",
    });
  }
  reply(res, 404);
}

/** Starts a server on 127.0.0.1 with a free port. */
export async function startTestServer(): Promise<TestServer> {
  const received: Received[] = [];
  const aborted = new Map<string, number>();
  const server = createServer((req, res) => {
    const url = new URL(req.url ?? "/", "http://127.0.0.1");
    const record: Received = {
      method: req.method ?? "",
      path: url.pathname,
      headers: flatHeaders(req),
      bodyBytes: 0,
    };
    received.push(record);
    // A client that goes away is expected here, not an error.
    req.on("error", () => {});
    res.on("error", () => {});
    const stops: (() => void)[] = [];
    res.on("close", () => {
      for (const stop of stops) stop();
      if (!res.writableFinished)
        aborted.set(record.path, (aborted.get(record.path) ?? 0) + 1);
    });
    const rate = record.path === "/upload" ? UPLOAD_BYTES_PER_SECOND : Infinity;
    const started = Date.now();
    req.on("data", (chunk: Buffer) => {
      record.bodyBytes += chunk.length;
      const wait = started + (record.bodyBytes / rate) * 1000 - Date.now();
      if (wait > 0) {
        req.pause();
        setTimeout(() => req.resume(), wait);
      }
    });
    req.on("end", () => route(url, res, record, (stop) => stops.push(stop)));
  });
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const { port } = server.address() as AddressInfo;
  return {
    url: (path) => `http://127.0.0.1:${port}${path}`,
    received: (path) =>
      received.filter((r) => path === undefined || r.path === path),
    aborted: (path) => aborted.get(path) ?? 0,
    stop: () =>
      new Promise<void>((resolve, reject) => {
        server.closeAllConnections();
        server.close((e) => (e ? reject(e) : resolve()));
      }),
  };
}

/**
 * Writes a project (see writeProject) whose staging and production send to
 * `server` through `baseUrl`, with the collection Shipments: `Shipment by
 * ID` (GET /json), `Create shipment` (POST /echo with a JSON body), and
 * `requests`, more request files there by file name.
 */
export function writeServerProject(
  name: string,
  parent: string,
  server: TestServer,
  requests: Record<string, string> = {},
): string {
  const root = writeProject(name, parent);
  for (const [env, production] of [
    ["staging", false],
    ["production", true],
  ] as const)
    writeFileSync(
      join(root, "environments", `${env}.yaml`),
      `name: ${env}\nproduction: ${production}\nvariables:\n` +
        `  - name: baseUrl\n    value: "${server.url("")}"\n`,
    );
  const files: Record<string, string> = {
    "_folder.yaml": "name: Shipments\n",
    "shipment-by-id.yaml":
      "name: Shipment by ID\nmethod: GET\nurl: '{{baseUrl}}/json'\n",
    "create-shipment.yaml":
      "name: Create shipment\nmethod: POST\nurl: '{{baseUrl}}/echo'\n" +
      "body:\n  mode: json\n" +
      `  text: '{"carrier": "Pacific Freight", "weight_kg": 12.4}'\n`,
    ...requests,
  };
  for (const [file, text] of Object.entries(files)) {
    const path = join(root, "requests", "shipments", file);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, text);
  }
  return root;
}
