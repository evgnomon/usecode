/**
 * ╔══════════════════════════════════════════════════════════════════╗
 * ║          COMPREHENSIVE NODE.JS FEATURES REFERENCE               ║
 * ║          All major Node.js APIs & patterns in one file          ║
 * ╚══════════════════════════════════════════════════════════════════╝
 *
 * Run: node nodejs_features.js
 * Node.js version: 18+ recommended
 */

"use strict";

// ─────────────────────────────────────────────────────────────────
// 1. CORE MODULES (Built-in)
// ─────────────────────────────────────────────────────────────────

const fs = require("fs");
const fsPromises = require("fs/promises");
const path = require("path");
const os = require("os");
const url = require("url");
const querystring = require("querystring");
const util = require("util");
const events = require("events");
const stream = require("stream");
const http = require("http");
const https = require("https");
const crypto = require("crypto");
const zlib = require("zlib");
const child_process = require("child_process");
const cluster = require("cluster");
const worker_threads = require("worker_threads");
const timers = require("timers/promises");
const readline = require("readline");
const dns = require("dns");
const net = require("net");
const tls = require("tls");
const dgram = require("dgram");
const assert = require("assert");
const buffer = require("buffer");
const string_decoder = require("string_decoder");
const perf_hooks = require("perf_hooks");
const async_hooks = require("async_hooks");
const v8 = require("v8");
const vm = require("vm");
const punycode = require("punycode");

// ─────────────────────────────────────────────────────────────────
// 2. GLOBAL OBJECTS & VARIABLES
// ─────────────────────────────────────────────────────────────────

function demonstrateGlobals() {
  console.log("\n═══ 2. GLOBAL OBJECTS & VARIABLES ═══");

  // __dirname & __filename
  console.log("Current file:", __filename);
  console.log("Current directory:", __dirname);

  // process object
  console.log("Node version:", process.version);
  console.log("Platform:", process.platform);
  console.log("Architecture:", process.arch);
  console.log("PID:", process.pid);
  console.log("Uptime:", process.uptime(), "seconds");
  console.log("Memory usage:", process.memoryUsage());
  console.log("CPU usage:", process.cpuUsage());
  console.log("CWD:", process.cwd());
  console.log("Env HOME:", process.env.HOME || process.env.USERPROFILE);
  console.log("Argv:", process.argv);
  console.log("Exec path:", process.execPath);

  // global & globalThis
  console.log("global === globalThis:", global === globalThis);

  // queueMicrotask
  queueMicrotask(() => {
    // Runs before next tick of event loop
  });

  // structuredClone (Node 17+)
  const original = { a: 1, b: { c: 2 }, d: new Date() };
  const cloned = structuredClone(original);
  console.log("structuredClone:", cloned);

  // URL & URLSearchParams (global)
  const myUrl = new URL(
    "https://example.com:8080/path?name=node&ver=20#section",
  );
  console.log("URL hostname:", myUrl.hostname);
  console.log("URL pathname:", myUrl.pathname);
  console.log("URL search:", myUrl.searchParams.get("name"));

  // TextEncoder / TextDecoder
  const encoder = new TextEncoder();
  const decoder = new TextDecoder("utf-8");
  const encoded = encoder.encode("Hello Node.js!");
  const decoded = decoder.decode(encoded);
  console.log("TextEncoder/Decoder:", decoded);

  // AbortController
  const controller = new AbortController();
  const signal = controller.signal;
  signal.addEventListener("abort", () => console.log("Aborted!"));
  controller.abort();

  // console methods
  console.log("log");
  console.info("info");
  console.warn("warn");
  console.error("error");
  console.dir({ nested: { obj: true } }, { depth: null, colors: true });
  console.table([
    { name: "Node", version: 20 },
    { name: "Deno", version: 1 },
  ]);
  console.time("timer");
  console.timeLog("timer", "checkpoint");
  console.timeEnd("timer");
  console.count("counter");
  console.count("counter");
  console.countReset("counter");
  console.group("Group");
  console.log("Inside group");
  console.groupEnd();
}

// ─────────────────────────────────────────────────────────────────
// 3. BUFFER
// ─────────────────────────────────────────────────────────────────

function demonstrateBuffer() {
  console.log("\n═══ 3. BUFFER ═══");

  // Creating buffers
  const buf1 = Buffer.alloc(10); // Zero-filled
  const buf2 = Buffer.alloc(10, 0xff); // Filled with 0xFF
  const buf3 = Buffer.allocUnsafe(10); // Uninitialized (faster)
  const buf4 = Buffer.from([0x48, 0x65, 0x6c, 0x6c, 0x6f]); // From array
  const buf5 = Buffer.from("Hello World", "utf-8"); // From string
  const buf6 = Buffer.from("SGVsbG8=", "base64"); // From base64

  console.log("Buffer from array:", buf4.toString());
  console.log("Buffer from string:", buf5.toString("hex"));
  console.log("Buffer from base64:", buf6.toString());

  // Buffer operations
  console.log("Length:", buf5.length);
  console.log("Byte at 0:", buf5[0]);
  console.log("Slice:", buf5.subarray(0, 5).toString());
  console.log("Includes:", buf5.includes("World"));
  console.log("IndexOf:", buf5.indexOf("World"));

  // Concatenation
  const combined = Buffer.concat([buf4, Buffer.from(" "), buf5]);
  console.log("Concat:", combined.toString());

  // Comparison
  console.log("Compare:", Buffer.compare(buf4, buf5));
  console.log("Equals:", buf4.equals(buf5));

  // Encoding conversions
  const str = "Node.js Buffers 🚀";
  const utf8Buf = Buffer.from(str, "utf-8");
  console.log("UTF-8 hex:", utf8Buf.toString("hex"));
  console.log("Base64:", utf8Buf.toString("base64"));
  console.log("Base64url:", utf8Buf.toString("base64url"));

  // Typed array interop
  const uint8 = new Uint8Array(buf5);
  const arrayBuffer = buf5.buffer.slice(
    buf5.byteOffset,
    buf5.byteOffset + buf5.byteLength,
  );
  const fromArrayBuffer = Buffer.from(arrayBuffer);
  console.log("From ArrayBuffer:", fromArrayBuffer.toString());

  // Read/write integers
  const numBuf = Buffer.alloc(8);
  numBuf.writeUInt32BE(0xdeadbeef, 0);
  numBuf.writeUInt32LE(0xcafebabe, 4);
  console.log("ReadUInt32BE:", numBuf.readUInt32BE(0).toString(16));
  console.log("ReadUInt32LE:", numBuf.readUInt32LE(4).toString(16));

  // Float read/write
  const floatBuf = Buffer.alloc(8);
  floatBuf.writeFloatBE(3.14, 0);
  floatBuf.writeDoubleBE(2.718281828, 0);
  console.log("ReadDoubleBE:", floatBuf.readDoubleBE(0));

  // Buffer.isBuffer & isEncoding
  console.log("isBuffer:", Buffer.isBuffer(buf1));
  console.log("isEncoding:", Buffer.isEncoding("utf-8"));

  // Blob (Node 18+)
  const blob = new Blob(["Hello Blob"], { type: "text/plain" });
  console.log("Blob size:", blob.size);
}

// ─────────────────────────────────────────────────────────────────
// 4. FILE SYSTEM (fs)
// ─────────────────────────────────────────────────────────────────

async function demonstrateFS() {
  console.log("\n═══ 4. FILE SYSTEM ═══");

  const testDir = path.join(os.tmpdir(), "node_features_test");
  const testFile = path.join(testDir, "test.txt");
  const testJSON = path.join(testDir, "data.json");
  const copyFile = path.join(testDir, "copy.txt");
  const linkFile = path.join(testDir, "link.txt");

  // --- Synchronous API ---
  // mkdir
  if (!fs.existsSync(testDir)) {
    fs.mkdirSync(testDir, { recursive: true });
  }

  // writeFileSync
  fs.writeFileSync(testFile, "Hello from Node.js!\nLine 2\nLine 3\n", "utf-8");
  console.log("File written (sync)");

  // readFileSync
  const content = fs.readFileSync(testFile, "utf-8");
  console.log("Read (sync):", content.trim());

  // appendFileSync
  fs.appendFileSync(testFile, "Appended line\n");

  // statSync
  const stats = fs.statSync(testFile);
  console.log("File size:", stats.size, "bytes");
  console.log("Is file:", stats.isFile());
  console.log("Is directory:", stats.isDirectory());
  console.log("Created:", stats.birthtime);
  console.log("Modified:", stats.mtime);

  // copyFileSync
  fs.copyFileSync(testFile, copyFile);
  console.log("File copied");

  // renameSync
  const renamedFile = path.join(testDir, "renamed.txt");
  fs.renameSync(copyFile, renamedFile);

  // readdirSync
  const files = fs.readdirSync(testDir);
  console.log("Directory contents:", files);

  // readdirSync with options
  const entries = fs.readdirSync(testDir, { withFileTypes: true });
  entries.forEach((entry) => {
    console.log(`  ${entry.name} - isFile: ${entry.isFile()}`);
  });

  // --- Promises API ---
  // writeFile
  const jsonData = {
    name: "Node.js",
    version: 20,
    features: ["async", "streams", "buffers"],
  };
  await fsPromises.writeFile(testJSON, JSON.stringify(jsonData, null, 2));
  console.log("JSON written (async)");

  // readFile
  const jsonContent = await fsPromises.readFile(testJSON, "utf-8");
  console.log("JSON read:", JSON.parse(jsonContent).name);

  // access (check existence/permissions)
  try {
    await fsPromises.access(testFile, fs.constants.R_OK | fs.constants.W_OK);
    console.log("File is readable and writable");
  } catch {
    console.log("File access denied");
  }

  // chmod
  await fsPromises.chmod(testFile, 0o644);

  // stat
  const asyncStats = await fsPromises.stat(testFile);
  console.log("Async stat size:", asyncStats.size);

  // lstat (for symlinks)
  const lstatResult = await fsPromises.lstat(testFile);
  console.log("Is symlink:", lstatResult.isSymbolicLink());

  // symlink & readlink
  const symlinkPath = path.join(testDir, "symlink.txt");
  try {
    await fsPromises.symlink(testFile, symlinkPath);
    const linkTarget = await fsPromises.readlink(symlinkPath);
    console.log("Symlink target:", linkTarget);
  } catch (e) {
    console.log("Symlink:", e.message);
  }

  // truncate
  await fsPromises.truncate(renamedFile, 10);

  // realpath
  const realPath = await fsPromises.realpath(testFile);
  console.log("Real path:", realPath);

  // mkdtemp
  const tmpDir = await fsPromises.mkdtemp(path.join(os.tmpdir(), "node-"));
  console.log("Temp dir:", tmpDir);

  // --- Callback API ---
  fs.readFile(testFile, "utf-8", (err, data) => {
    if (err) throw err;
    console.log("Callback read:", data.substring(0, 20) + "...");
  });

  // --- Streams ---
  // Read stream
  const readStream = fs.createReadStream(testFile, {
    encoding: "utf-8",
    highWaterMark: 16,
  });
  readStream.on("data", (chunk) => {
    /* process chunk */
  });
  readStream.on("end", () => console.log("Read stream ended"));

  // Write stream
  const writeStream = fs.createWriteStream(
    path.join(testDir, "stream_out.txt"),
  );
  writeStream.write("Line 1\n");
  writeStream.write("Line 2\n");
  writeStream.end("Final line\n");
  writeStream.on("finish", () => console.log("Write stream finished"));

  // --- File Descriptors ---
  const fd = fs.openSync(testFile, "r");
  const fdBuffer = Buffer.alloc(10);
  fs.readSync(fd, fdBuffer, 0, 10, 0);
  console.log("FD read:", fdBuffer.toString());
  fs.closeSync(fd);

  // --- Watch ---
  const watcher = fs.watch(testDir, (eventType, filename) => {
    // console.log(`Watch: ${eventType} on ${filename}`);
  });
  setTimeout(() => watcher.close(), 100);

  // --- fs.glob (Node 22+) ---
  // await fsPromises.glob('**/*.txt', { cwd: testDir });

  // --- Cleanup ---
  await fsPromises.rm(testDir, { recursive: true, force: true });
  await fsPromises.rm(tmpDir, { recursive: true, force: true });
  console.log("Cleanup complete");
}

// ─────────────────────────────────────────────────────────────────
// 5. PATH MODULE
// ─────────────────────────────────────────────────────────────────

function demonstratePath() {
  console.log("\n═══ 5. PATH MODULE ═══");

  const filePath = "/home/user/documents/report.pdf";

  console.log("basename:", path.basename(filePath)); // report.pdf
  console.log("basename no ext:", path.basename(filePath, ".pdf")); // report
  console.log("dirname:", path.dirname(filePath)); // /home/user/documents
  console.log("extname:", path.extname(filePath)); // .pdf
  console.log("parse:", path.parse(filePath));
  console.log("format:", path.format({ dir: "/home/user", base: "file.txt" }));
  console.log("join:", path.join("/home", "user", "..", "docs", "file.txt"));
  console.log("resolve:", path.resolve("src", "index.js"));
  console.log("normalize:", path.normalize("/home/user/../user/./docs"));
  console.log("isAbsolute:", path.isAbsolute(filePath));
  console.log("relative:", path.relative("/home/user/docs", "/home/user/pics"));
  console.log("sep:", path.sep);
  console.log("delimiter:", path.delimiter);
  console.log("toNamespacedPath:", path.toNamespacedPath(filePath));
}

// ─────────────────────────────────────────────────────────────────
// 6. OS MODULE
// ─────────────────────────────────────────────────────────────────

function demonstrateOS() {
  console.log("\n═══ 6. OS MODULE ═══");

  console.log("Platform:", os.platform());
  console.log("Architecture:", os.arch());
  console.log("Hostname:", os.hostname());
  console.log("Type:", os.type());
  console.log("Release:", os.release());
  console.log("Version:", os.version());
  console.log("Home dir:", os.homedir());
  console.log("Temp dir:", os.tmpdir());
  console.log("EOL repr:", JSON.stringify(os.EOL));
  console.log("Endianness:", os.endianness());
  console.log("CPUs:", os.cpus().length, "cores");
  console.log("CPU model:", os.cpus()[0]?.model);
  console.log(
    "Total memory:",
    (os.totalmem() / 1024 / 1024 / 1024).toFixed(2),
    "GB",
  );
  console.log(
    "Free memory:",
    (os.freemem() / 1024 / 1024 / 1024).toFixed(2),
    "GB",
  );
  console.log("Uptime:", (os.uptime() / 3600).toFixed(2), "hours");
  console.log("Load avg:", os.loadavg());
  console.log("User info:", os.userInfo());

  // Network interfaces
  const nets = os.networkInterfaces();
  for (const [name, interfaces] of Object.entries(nets)) {
    for (const iface of interfaces) {
      if (iface.family === "IPv4") {
        console.log(`Network ${name}: ${iface.address}`);
      }
    }
  }

  // Priority constants
  console.log("Priority HIGH:", os.constants.priority.PRIORITY_HIGH);
}

// ─────────────────────────────────────────────────────────────────
// 7. EVENTS (EventEmitter)
// ─────────────────────────────────────────────────────────────────

function demonstrateEvents() {
  console.log("\n═══ 7. EVENTS ═══");

  // Basic EventEmitter
  class MyEmitter extends events.EventEmitter {}
  const emitter = new MyEmitter();

  // on / addListener
  emitter.on("data", (msg) => console.log("on:", msg));

  // once
  emitter.once("data", (msg) => console.log("once:", msg));

  // prependListener
  emitter.prependListener("data", (msg) => console.log("prepend:", msg));

  // emit
  emitter.emit("data", "Hello Events!");
  emitter.emit("data", "Second emit (once handler removed)");

  // Multiple arguments
  emitter.on("multi", (a, b, c) => console.log("Multi args:", a, b, c));
  emitter.emit("multi", 1, "two", { three: 3 });

  // removeListener / off
  const handler = () => console.log("Will be removed");
  emitter.on("temp", handler);
  emitter.off("temp", handler);
  emitter.emit("temp"); // No output

  // removeAllListeners
  emitter.on("all", () => {});
  emitter.on("all", () => {});
  emitter.removeAllListeners("all");

  // Listener count & names
  emitter.on("test", () => {});
  emitter.on("test", () => {});
  console.log("Listener count:", emitter.listenerCount("test"));
  console.log("Event names:", emitter.eventNames());
  console.log("Raw listeners:", emitter.rawListeners("test").length);

  // Max listeners
  emitter.setMaxListeners(20);
  console.log("Max listeners:", emitter.getMaxListeners());

  // Error event
  emitter.on("error", (err) => console.log("Error caught:", err.message));
  emitter.emit("error", new Error("Something went wrong"));

  // newListener / removeListener events
  emitter.on("newListener", (event) => {
    if (event === "tracked") console.log("New listener added for:", event);
  });
  emitter.on("tracked", () => {});

  // EventEmitter.once (promise-based)
  const asyncEmitter = new events.EventEmitter();
  setTimeout(() => asyncEmitter.emit("ready", "data"), 10);
  events.once(asyncEmitter, "ready").then(([val]) => {
    console.log("Async once:", val);
  });

  // on (async iterator) - Node 18+
  // for await (const [event] of events.on(emitter, 'stream')) { ... }

  // captureRejections
  const rejEmitter = new events.EventEmitter({ captureRejections: true });
  rejEmitter.on("event", async () => {
    throw new Error("async error");
  });
  rejEmitter[Symbol.for("nodejs.rejection")] = (err) => {
    console.log("Captured rejection:", err.message);
  };

  // Static: EventEmitter.listenerCount (deprecated but exists)
  console.log(
    "Static listenerCount:",
    events.EventEmitter.listenerCount(emitter, "test"),
  );
}

// ─────────────────────────────────────────────────────────────────
// 8. STREAMS
// ─────────────────────────────────────────────────────────────────

async function demonstrateStreams() {
  console.log("\n═══ 8. STREAMS ═══");

  // --- Readable Stream ---
  const readable = new stream.Readable({
    read(size) {
      this.push("chunk1 ");
      this.push("chunk2 ");
      this.push(null); // Signal end
    },
  });

  let readData = "";
  readable.on("data", (chunk) => {
    readData += chunk;
  });
  readable.on("end", () => console.log("Readable:", readData.trim()));

  // --- Writable Stream ---
  const chunks = [];
  const writable = new stream.Writable({
    write(chunk, encoding, callback) {
      chunks.push(chunk.toString());
      callback();
    },
    final(callback) {
      console.log("Writable received:", chunks.join(""));
      callback();
    },
  });
  writable.write("Hello ");
  writable.end("Writable!");

  // --- Duplex Stream ---
  const duplex = new stream.Duplex({
    read(size) {
      this.push("duplex data");
      this.push(null);
    },
    write(chunk, encoding, callback) {
      console.log("Duplex write:", chunk.toString());
      callback();
    },
  });

  // --- Transform Stream ---
  const upperTransform = new stream.Transform({
    transform(chunk, encoding, callback) {
      callback(null, chunk.toString().toUpperCase());
    },
    flush(callback) {
      this.push("\n--- END ---");
      callback();
    },
  });

  // --- PassThrough Stream ---
  const passThrough = new stream.PassThrough();

  // --- Pipeline (safe piping with cleanup) ---
  const { pipeline } = stream;
  const source = new stream.Readable({
    read() {
      this.push("hello streams ");
      this.push(null);
    },
  });
  const dest = new stream.Writable({
    write(chunk, enc, cb) {
      console.log("Pipeline result:", chunk.toString().trim());
      cb();
    },
  });

  await util.promisify(pipeline)(source, upperTransform, dest);

  // --- stream.Readable.from (iterable to stream) ---
  const fromIterable = stream.Readable.from(["a", "b", "c"]);
  const iterChunks = [];
  for await (const chunk of fromIterable) {
    iterChunks.push(chunk);
  }
  console.log("Readable.from:", iterChunks.join(","));

  // --- Async iteration on streams ---
  const asyncReadable = new stream.Readable({
    read() {
      this.push("async ");
      this.push("iter");
      this.push(null);
    },
  });
  let asyncResult = "";
  for await (const chunk of asyncReadable) {
    asyncResult += chunk;
  }
  console.log("Async iteration:", asyncResult);

  // --- Backpressure handling ---
  const slowWriter = new stream.Writable({
    highWaterMark: 16,
    write(chunk, enc, cb) {
      setTimeout(cb, 1); // Simulate slow write
    },
  });

  const canWrite = slowWriter.write("test");
  console.log("Can write (backpressure):", canWrite);
  slowWriter.end();

  // --- Object mode streams ---
  const objectStream = new stream.Transform({
    objectMode: true,
    transform(obj, enc, cb) {
      cb(null, { ...obj, processed: true });
    },
  });
  objectStream.write({ id: 1, name: "test" });
  objectStream.on("data", (obj) => console.log("Object stream:", obj));
  objectStream.end();

  // --- stream.compose (Node 18+) ---
  // const composed = stream.compose(transform1, transform2);

  // --- Web Streams API (Node 18+) ---
  const webReadable = new ReadableStream({
    start(controller) {
      controller.enqueue("web ");
      controller.enqueue("stream");
      controller.close();
    },
  });
  const reader = webReadable.getReader();
  let webResult = "";
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    webResult += value;
  }
  console.log("Web ReadableStream:", webResult);
}

// ─────────────────────────────────────────────────────────────────
// 9. HTTP / HTTPS
// ─────────────────────────────────────────────────────────────────

async function demonstrateHTTP() {
  console.log("\n═══ 9. HTTP / HTTPS ═══");

  // --- HTTP Server ---
  const server = http.createServer((req, res) => {
    const { method, url: reqUrl, headers } = req;

    // Request info
    console.log(`${method} ${reqUrl}`);

    // Collect body
    let body = "";
    req.on("data", (chunk) => {
      body += chunk;
    });
    req.on("end", () => {
      // Response headers
      res.writeHead(200, {
        "Content-Type": "application/json",
        "X-Custom-Header": "NodeJS",
        "Cache-Control": "no-cache",
      });

      // Response body
      res.end(
        JSON.stringify({
          message: "Hello from Node.js HTTP server!",
          method,
          url: reqUrl,
          headers: req.headers,
          body: body || undefined,
        }),
      );
    });
  });

  // Server events
  server.on("listening", () => console.log("Server listening"));
  server.on("connection", (socket) => {
    /* new TCP connection */
  });
  server.on("close", () => console.log("Server closed"));

  // Start server
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  console.log("Server on port:", port);

  // --- HTTP Client (http.request) ---
  const makeRequest = (options, postData) =>
    new Promise((resolve, reject) => {
      const req = http.request(options, (res) => {
        let data = "";
        res.on("data", (chunk) => {
          data += chunk;
        });
        res.on("end", () => {
          resolve({
            statusCode: res.statusCode,
            headers: res.headers,
            body: JSON.parse(data),
          });
        });
      });
      req.on("error", reject);
      if (postData) req.write(postData);
      req.end();
    });

  // GET request
  const getResult = await makeRequest({
    hostname: "127.0.0.1",
    port,
    path: "/api/data?key=value",
    method: "GET",
    headers: { Accept: "application/json" },
  });
  console.log("GET response:", getResult.statusCode);

  // POST request
  const postData = JSON.stringify({ name: "Node.js" });
  const postResult = await makeRequest(
    {
      hostname: "127.0.0.1",
      port,
      path: "/api/submit",
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Content-Length": Buffer.byteLength(postData),
      },
    },
    postData,
  );
  console.log("POST response:", postResult.body.message);

  // --- http.get shorthand ---
  await new Promise((resolve) => {
    http.get(`http://127.0.0.1:${port}/shorthand`, (res) => {
      res.on("data", () => {});
      res.on("end", () => {
        console.log("http.get status:", res.statusCode);
        resolve();
      });
    });
  });

  // --- fetch API (Node 18+) ---
  try {
    const fetchRes = await fetch(`http://127.0.0.1:${port}/fetch-test`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ via: "fetch" }),
    });
    const fetchData = await fetchRes.json();
    console.log("Fetch API result:", fetchData.message);
  } catch (e) {
    console.log("Fetch:", e.message);
  }

  // Server properties
  console.log("Max headers count:", server.maxHeadersCount);
  console.log("Keep-alive timeout:", server.keepAliveTimeout);
  console.log("Headers timeout:", server.headersTimeout);

  // Close server
  server.close();
  console.log("HTTP demo complete");
}

// ─────────────────────────────────────────────────────────────────
// 10. URL & QUERYSTRING
// ─────────────────────────────────────────────────────────────────

function demonstrateURL() {
  console.log("\n═══ 10. URL & QUERYSTRING ═══");

  // WHATWG URL API
  const myUrl = new URL(
    "https://user:pass@sub.example.com:8080/p/a/t/h?query=string#hash",
  );
  console.log("href:", myUrl.href);
  console.log("origin:", myUrl.origin);
  console.log("protocol:", myUrl.protocol);
  console.log("username:", myUrl.username);
  console.log("password:", myUrl.password);
  console.log("host:", myUrl.host);
  console.log("hostname:", myUrl.hostname);
  console.log("port:", myUrl.port);
  console.log("pathname:", myUrl.pathname);
  console.log("search:", myUrl.search);
  console.log("hash:", myUrl.hash);

  // URLSearchParams
  const params = new URLSearchParams("foo=bar&baz=qux&foo=quux");
  console.log("get:", params.get("foo"));
  console.log("getAll:", params.getAll("foo"));
  console.log("has:", params.has("baz"));
  params.append("new", "value");
  params.set("foo", "updated");
  params.delete("baz");
  params.sort();
  console.log("toString:", params.toString());
  for (const [key, value] of params) {
    console.log(`  ${key} = ${value}`);
  }

  // Legacy url module
  const parsed = url.parse("http://example.com:8080/path?q=1#frag", true);
  console.log("Legacy parsed:", parsed.hostname, parsed.query);
  console.log("Legacy format:", url.format(parsed));
  console.log("Resolve:", url.resolve("http://example.com/a/", "../b"));

  // querystring module
  const qs = querystring.parse(
    "name=Node&version=20&features=async&features=streams",
  );
  console.log("QS parse:", qs);
  console.log("QS stringify:", querystring.stringify(qs));
  console.log("QS escape:", querystring.escape("hello world!"));
  console.log("QS unescape:", querystring.unescape("hello%20world!"));
}

// ─────────────────────────────────────────────────────────────────
// 11. CRYPTO
// ─────────────────────────────────────────────────────────────────

function demonstrateCrypto() {
  console.log("\n═══ 11. CRYPTO ═══");

  // --- Hashing ---
  const hash = crypto.createHash("sha256").update("Hello World").digest("hex");
  console.log("SHA256:", hash);

  // Streaming hash
  const hashStream = crypto.createHash("md5");
  hashStream.update("part1");
  hashStream.update("part2");
  console.log("MD5 streaming:", hashStream.digest("hex"));

  // Available hashes
  console.log("Available hashes:", crypto.getHashes().slice(0, 5), "...");

  // --- HMAC ---
  const hmac = crypto
    .createHmac("sha256", "secret-key")
    .update("message")
    .digest("hex");
  console.log("HMAC:", hmac);

  // --- Random bytes ---
  const randomBytes = crypto.randomBytes(16);
  console.log("Random bytes:", randomBytes.toString("hex"));

  // Random int
  const randomInt = crypto.randomInt(1, 100);
  console.log("Random int [1,100):", randomInt);

  // UUID
  console.log("UUID:", crypto.randomUUID());

  // --- Symmetric Encryption (AES) ---
  const algorithm = "aes-256-gcm";
  const key = crypto.randomBytes(32);
  const iv = crypto.randomBytes(16);

  // Encrypt
  const cipher = crypto.createCipheriv(algorithm, key, iv);
  let encrypted = cipher.update("Secret message!", "utf-8", "hex");
  encrypted += cipher.final("hex");
  const authTag = cipher.getAuthTag();
  console.log("Encrypted:", encrypted);

  // Decrypt
  const decipher = crypto.createDecipheriv(algorithm, key, iv);
  decipher.setAuthTag(authTag);
  let decrypted = decipher.update(encrypted, "hex", "utf-8");
  decrypted += decipher.final("utf-8");
  console.log("Decrypted:", decrypted);

  // Available ciphers
  console.log("Ciphers:", crypto.getCiphers().slice(0, 5), "...");

  // --- Asymmetric Keys (RSA) ---
  const { publicKey, privateKey } = crypto.generateKeyPairSync("rsa", {
    modulusLength: 2048,
    publicKeyEncoding: { type: "spki", format: "pem" },
    privateKeyEncoding: { type: "pkcs8", format: "pem" },
  });
  console.log("RSA key generated:", publicKey.substring(0, 40) + "...");

  // Sign & Verify
  const sign = crypto.createSign("SHA256");
  sign.update("Data to sign");
  const signature = sign.sign(privateKey, "hex");
  console.log("Signature:", signature.substring(0, 40) + "...");

  const verify = crypto.createVerify("SHA256");
  verify.update("Data to sign");
  console.log("Verify:", verify.verify(publicKey, signature, "hex"));

  // --- Diffie-Hellman Key Exchange ---
  const alice = crypto.createDiffieHellman(2048);
  alice.generateKeys();
  const bob = crypto.createDiffieHellman(
    alice.getPrime(),
    alice.getGenerator(),
  );
  bob.generateKeys();
  const aliceSecret = alice.computeSecret(bob.getPublicKey());
  const bobSecret = bob.computeSecret(alice.getPublicKey());
  console.log("DH secrets match:", aliceSecret.equals(bobSecret));

  // --- ECDH ---
  const ecdh = crypto.createECDH("secp256k1");
  ecdh.generateKeys();
  console.log(
    "ECDH pubkey:",
    ecdh.getPublicKey("hex").substring(0, 30) + "...",
  );

  // --- PBKDF2 (Password hashing) ---
  const salt = crypto.randomBytes(16);
  const derivedKey = crypto.pbkdf2Sync("password", salt, 100000, 64, "sha512");
  console.log("PBKDF2:", derivedKey.toString("hex").substring(0, 40) + "...");

  // --- scrypt ---
  const scryptKey = crypto.scryptSync("password", salt, 64);
  console.log("Scrypt:", scryptKey.toString("hex").substring(0, 40) + "...");

  // --- X509Certificate (Node 15+) ---
  // const cert = new crypto.X509Certificate(certPem);

  // --- Web Crypto API ---
  console.log(
    "Web Crypto available:",
    typeof globalThis.crypto?.subtle !== "undefined",
  );

  // --- Certificate ---
  console.log("FIPS mode:", crypto.getFips());
}

// ─────────────────────────────────────────────────────────────────
// 12. ZLIB (Compression)
// ─────────────────────────────────────────────────────────────────

async function demonstrateZlib() {
  console.log("\n═══ 12. ZLIB (Compression) ═══");

  const input = "Hello World! ".repeat(100);
  console.log("Original size:", Buffer.byteLength(input));

  // --- Gzip / Gunzip ---
  const gzipped = zlib.gzipSync(input);
  console.log("Gzipped size:", gzipped.length);
  const gunzipped = zlib.gunzipSync(gzipped);
  console.log("Gunzipped matches:", gunzipped.toString() === input);

  // --- Deflate / Inflate ---
  const deflated = zlib.deflateSync(input);
  console.log("Deflated size:", deflated.length);
  const inflated = zlib.inflateSync(deflated);
  console.log("Inflated matches:", inflated.toString() === input);

  // --- DeflateRaw / InflateRaw ---
  const rawDeflated = zlib.deflateRawSync(input);
  const rawInflated = zlib.inflateRawSync(rawDeflated);
  console.log("Raw deflate/inflate:", rawInflated.toString() === input);

  // --- Brotli ---
  const brotli = zlib.brotliCompressSync(input);
  console.log("Brotli size:", brotli.length);
  const unbrotli = zlib.brotliDecompressSync(brotli);
  console.log("Brotli matches:", unbrotli.toString() === input);

  // --- Async (callback) ---
  await new Promise((resolve) => {
    zlib.gzip(input, (err, result) => {
      console.log("Async gzip size:", result.length);
      resolve();
    });
  });

  // --- Streaming compression ---
  const gzipStream = zlib.createGzip({ level: 9 });
  const gunzipStream = zlib.createGunzip();
  const compressedChunks = [];

  await new Promise((resolve) => {
    const source = stream.Readable.from([input]);
    source
      .pipe(gzipStream)
      .on("data", (chunk) => compressedChunks.push(chunk))
      .on("end", () => {
        console.log(
          "Stream compressed:",
          Buffer.concat(compressedChunks).length,
        );
        resolve();
      });
  });

  // Compression levels
  console.log(
    "Constants Z_BEST_COMPRESSION:",
    zlib.constants.Z_BEST_COMPRESSION,
  );
  console.log("Constants Z_BEST_SPEED:", zlib.constants.Z_BEST_SPEED);
}

// ─────────────────────────────────────────────────────────────────
// 13. CHILD PROCESS
// ─────────────────────────────────────────────────────────────────

async function demonstrateChildProcess() {
  console.log("\n═══ 13. CHILD PROCESS ═══");

  // --- exec (shell command, buffered output) ---
  const execPromise = util.promisify(child_process.exec);
  const { stdout: execOut } = await execPromise('echo "Hello from exec"');
  console.log("exec:", execOut.trim());

  // --- execSync ---
  const syncOut = child_process.execSync('echo "Sync exec"', {
    encoding: "utf-8",
  });
  console.log("execSync:", syncOut.trim());

  // --- execFile (no shell, safer) ---
  const execFilePromise = util.promisify(child_process.execFile);
  const { stdout: fileOut } = await execFilePromise("echo", [
    "Hello from execFile",
  ]);
  console.log("execFile:", fileOut.trim());

  // --- spawn (streaming I/O) ---
  await new Promise((resolve) => {
    const spawned = child_process.spawn("echo", ["Hello from spawn"]);
    let spawnData = "";
    spawned.stdout.on("data", (data) => {
      spawnData += data;
    });
    spawned.on("close", (code) => {
      console.log("spawn:", spawnData.trim(), "(exit code:", code + ")");
      resolve();
    });
  });

  // spawn with options
  const ls = child_process.spawn("ls", ["-la", "/tmp"], {
    cwd: "/",
    env: { ...process.env, CUSTOM: "value" },
    stdio: ["pipe", "pipe", "pipe"],
    // detached: false,
    // shell: false,
    // uid, gid, timeout, killSignal
  });
  ls.stdout.on("data", () => {}); // consume
  ls.stderr.on("data", () => {});
  await new Promise((r) => ls.on("close", r));
  console.log("spawn ls completed");

  // --- fork (spawn Node.js child with IPC) ---
  // child_process.fork('child.js', [], { execArgv: ['--max-old-space-size=512'] });
  // child.send({ type: 'message' });
  // child.on('message', (msg) => { ... });

  // --- spawnSync ---
  const syncSpawn = child_process.spawnSync("echo", ["sync spawn"], {
    encoding: "utf-8",
  });
  console.log(
    "spawnSync:",
    syncSpawn.stdout.trim(),
    "status:",
    syncSpawn.status,
  );

  // --- execFileSync ---
  const syncFile = child_process.execFileSync("echo", ["sync execFile"], {
    encoding: "utf-8",
  });
  console.log("execFileSync:", syncFile.trim());

  // --- Signal handling ---
  const longProcess = child_process.spawn("sleep", ["10"]);
  longProcess.kill("SIGTERM");
  console.log("Process killed:", longProcess.killed);
}

// ─────────────────────────────────────────────────────────────────
// 14. WORKER THREADS
// ─────────────────────────────────────────────────────────────────

async function demonstrateWorkerThreads() {
  console.log("\n═══ 14. WORKER THREADS ═══");

  const {
    Worker,
    isMainThread,
    parentPort,
    workerData,
    MessageChannel,
    BroadcastChannel,
  } = worker_threads;

  console.log("Is main thread:", isMainThread);
  console.log("Thread ID:", worker_threads.threadId);

  // Create worker with inline code using eval
  const workerCode = `
    const { parentPort, workerData, threadId } = require('worker_threads');
    parentPort.postMessage({
      result: workerData.a + workerData.b,
      threadId
    });
  `;

  const worker = new Worker(workerCode, {
    eval: true,
    workerData: { a: 10, b: 20 },
  });

  await new Promise((resolve) => {
    worker.on("message", (msg) => {
      console.log("Worker result:", msg.result, "from thread:", msg.threadId);
    });
    worker.on("error", (err) => console.error("Worker error:", err));
    worker.on("exit", (code) => {
      console.log("Worker exited with code:", code);
      resolve();
    });
  });

  // --- SharedArrayBuffer ---
  const shared = new SharedArrayBuffer(4);
  const sharedArray = new Int32Array(shared);
  Atomics.store(sharedArray, 0, 42);
  console.log("SharedArrayBuffer value:", Atomics.load(sharedArray, 0));

  // Atomic operations
  Atomics.add(sharedArray, 0, 8);
  console.log("After Atomics.add:", Atomics.load(sharedArray, 0));
  Atomics.sub(sharedArray, 0, 10);
  console.log("After Atomics.sub:", Atomics.load(sharedArray, 0));
  console.log("Atomics.exchange:", Atomics.exchange(sharedArray, 0, 100));
  console.log(
    "Atomics.compareExchange:",
    Atomics.compareExchange(sharedArray, 0, 100, 200),
  );

  // --- MessageChannel ---
  const { port1, port2 } = new MessageChannel();
  port1.on("message", (msg) => console.log("MessageChannel:", msg));
  port2.postMessage("Hello via MessageChannel!");
  port1.close();
  port2.close();

  // --- BroadcastChannel (Node 18+) ---
  const bc1 = new BroadcastChannel("test");
  const bc2 = new BroadcastChannel("test");
  bc2.onmessage = (event) => console.log("BroadcastChannel:", event.data);
  bc1.postMessage("Broadcast message");
  setTimeout(() => {
    bc1.close();
    bc2.close();
  }, 50);

  // --- Transferable objects ---
  // worker.postMessage(buffer, [buffer]); // Transfer ownership
}

// ─────────────────────────────────────────────────────────────────
// 15. CLUSTER
// ─────────────────────────────────────────────────────────────────

function demonstrateCluster() {
  console.log("\n═══ 15. CLUSTER ═══");

  // Cluster is used to fork multiple worker processes
  console.log("Is primary:", cluster.isPrimary);
  console.log("Is worker:", cluster.isWorker);

  // Typical usage pattern (not executed here):
  /*
  if (cluster.isPrimary) {
    console.log(`Primary ${process.pid} is running`);
    const numCPUs = os.cpus().length;

    for (let i = 0; i < numCPUs; i++) {
      const worker = cluster.fork();
      worker.on('message', (msg) => console.log('From worker:', msg));
      worker.send('Hello worker!');
    }

    cluster.on('exit', (worker, code, signal) => {
      console.log(`Worker ${worker.process.pid} died (${signal || code})`);
      cluster.fork(); // Restart worker
    });

    cluster.on('online', (worker) => {
      console.log(`Worker ${worker.id} online`);
    });

    // Cluster settings
    cluster.setupPrimary({
      exec: 'worker.js',
      args: ['--optimize'],
      silent: false,
    });

    // Access workers
    for (const id in cluster.workers) {
      console.log(`Worker ${id}: PID ${cluster.workers[id].process.pid}`);
    }
  } else {
    // Worker process
    http.createServer((req, res) => {
      res.end(`Worker ${process.pid}\n`);
    }).listen(8000);

    process.on('message', (msg) => console.log('From primary:', msg));
    process.send('Worker ready');
  }
  */

  console.log("Cluster scheduling policy:", cluster.schedulingPolicy);
  console.log("(Cluster demo shown as code pattern - not forked)");
}

// ─────────────────────────────────────────────────────────────────
// 16. DNS
// ─────────────────────────────────────────────────────────────────

async function demonstrateDNS() {
  console.log("\n═══ 16. DNS ═══");

  const dnsPromises = dns.promises;

  try {
    // lookup (uses OS resolver)
    const { address, family } = await dnsPromises.lookup("localhost");
    console.log("lookup:", address, "IPv" + family);

    // lookupService
    try {
      const { hostname, service } = await dnsPromises.lookupService(
        "127.0.0.1",
        22,
      );
      console.log("lookupService:", hostname, service);
    } catch {
      console.log("lookupService: not available");
    }

    // getServers
    console.log("DNS servers:", dns.getServers().slice(0, 2));

    // resolve (uses DNS protocol directly)
    try {
      const addresses = await dnsPromises.resolve4("localhost");
      console.log("resolve4:", addresses);
    } catch {
      console.log("resolve4: localhost not resolvable via DNS");
    }
  } catch (e) {
    console.log("DNS error:", e.message);
  }

  // DNS resolver class
  const resolver = new dns.Resolver();
  console.log("Resolver created");
}

// ─────────────────────────────────────────────────────────────────
// 17. NET (TCP) & TLS
// ─────────────────────────────────────────────────────────────────

async function demonstrateNet() {
  console.log("\n═══ 17. NET (TCP) ═══");

  // --- TCP Server ---
  const tcpServer = net.createServer((socket) => {
    socket.write("Hello TCP client!\n");
    socket.on("data", (data) => {
      console.log("TCP received:", data.toString().trim());
      socket.end("Goodbye!\n");
    });
    socket.on("end", () => console.log("TCP client disconnected"));
  });

  await new Promise((resolve) => tcpServer.listen(0, "127.0.0.1", resolve));
  const tcpPort = tcpServer.address().port;
  console.log("TCP server on port:", tcpPort);

  // --- TCP Client ---
  await new Promise((resolve) => {
    const client = net.createConnection(
      { port: tcpPort, host: "127.0.0.1" },
      () => {
        console.log("TCP connected");
        client.write("Hello from client!");
      },
    );
    client.on("data", (data) => {
      console.log("TCP client got:", data.toString().trim());
    });
    client.on("end", () => {
      console.log("TCP connection ended");
      resolve();
    });
  });

  tcpServer.close();

  // Socket properties
  console.log('net.isIP("127.0.0.1"):', net.isIP("127.0.0.1"));
  console.log('net.isIPv4("127.0.0.1"):', net.isIPv4("127.0.0.1"));
  console.log('net.isIPv6("::1"):', net.isIPv6("::1"));
}

// ─────────────────────────────────────────────────────────────────
// 18. DGRAM (UDP)
// ─────────────────────────────────────────────────────────────────

async function demonstrateDGram() {
  console.log("\n═══ 18. DGRAM (UDP) ═══");

  const udpServer = dgram.createSocket("udp4");
  const udpClient = dgram.createSocket("udp4");

  await new Promise((resolve) => {
    udpServer.on("message", (msg, rinfo) => {
      console.log(`UDP received: "${msg}" from ${rinfo.address}:${rinfo.port}`);
      udpServer.close();
      udpClient.close();
      resolve();
    });

    udpServer.bind(0, "127.0.0.1", () => {
      const port = udpServer.address().port;
      console.log("UDP server on port:", port);
      udpClient.send("Hello UDP!", port, "127.0.0.1");
    });
  });
}

// ─────────────────────────────────────────────────────────────────
// 19. READLINE
// ─────────────────────────────────────────────────────────────────

function demonstrateReadline() {
  console.log("\n═══ 19. READLINE ═══");

  // Create interface (from string stream for demo)
  const input = new stream.Readable({
    read() {
      this.push("Line 1\nLine 2\nLine 3\n");
      this.push(null);
    },
  });

  const rl = readline.createInterface({
    input,
    crlfDelay: Infinity,
    terminal: false,
  });

  const lines = [];
  rl.on("line", (line) => lines.push(line));
  rl.on("close", () => console.log("Lines read:", lines));

  // Interactive readline (pattern):
  /*
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    prompt: '> ',
    historySize: 100,
    completer: (line) => {
      const completions = ['hello', 'help', 'exit'];
      const hits = completions.filter(c => c.startsWith(line));
      return [hits.length ? hits : completions, line];
    }
  });

  rl.prompt();
  rl.on('line', (answer) => {
    console.log(`You said: ${answer}`);
    rl.prompt();
  });

  // readline/promises (Node 17+)
  const { createInterface } = require('readline/promises');
  const answer = await rl.question('What is your name? ');
  */

  console.log("(Interactive readline shown as pattern)");
}

// ─────────────────────────────────────────────────────────────────
// 20. UTIL MODULE
// ─────────────────────────────────────────────────────────────────

function demonstrateUtil() {
  console.log("\n═══ 20. UTIL MODULE ═══");

  // promisify
  const setTimeoutPromise = util.promisify(setTimeout);
  // await setTimeoutPromise(100);

  // callbackify
  async function asyncFn() {
    return "result";
  }
  const cbFn = util.callbackify(asyncFn);
  cbFn((err, result) => console.log("callbackify:", result));

  // inspect
  const obj = {
    a: 1,
    b: [2, 3],
    c: new Map([["key", "val"]]),
    d: new Set([1, 2, 3]),
  };
  console.log(
    "inspect:",
    util.inspect(obj, {
      depth: null,
      colors: true,
      maxArrayLength: 10,
      maxStringLength: 100,
      compact: false,
      sorted: true,
      showHidden: false,
      showProxy: true,
    }),
  );

  // Custom inspect
  class MyClass {
    [util.inspect.custom](depth, options) {
      return `MyClass { custom inspect }`;
    }
  }
  console.log("Custom inspect:", util.inspect(new MyClass()));

  // format
  console.log(
    "format:",
    util.format("Hello %s, you are %d years old. %j", "Node", 20, { v: 1 }),
  );
  console.log(
    "formatWithOptions:",
    util.formatWithOptions({ colors: true }, "%O", { a: 1 }),
  );

  // types
  console.log("isDate:", util.types.isDate(new Date()));
  console.log("isRegExp:", util.types.isRegExp(/test/));
  console.log("isMap:", util.types.isMap(new Map()));
  console.log("isSet:", util.types.isSet(new Set()));
  console.log("isPromise:", util.types.isPromise(Promise.resolve()));
  console.log(
    "isGeneratorFunction:",
    util.types.isGeneratorFunction(function* () {}),
  );
  console.log(
    "isAsyncFunction:",
    util.types.isAsyncFunction(async () => {}),
  );
  console.log("isArrayBuffer:", util.types.isArrayBuffer(new ArrayBuffer(8)));
  console.log("isTypedArray:", util.types.isTypedArray(new Uint8Array()));
  console.log("isProxy:", util.types.isProxy(new Proxy({}, {})));

  // deprecate
  const deprecated = util.deprecate(() => {}, "This function is deprecated");

  // debuglog
  const debug = util.debuglog("myapp");
  debug("This only shows if NODE_DEBUG=myapp");

  // getSystemErrorName
  console.log("Error name:", util.getSystemErrorName(-2)); // ENOENT

  // TextDecoder / TextEncoder
  console.log(
    "TextDecoder:",
    new util.TextDecoder("utf-8").decode(Buffer.from("hello")),
  );

  // isDeepStrictEqual
  console.log("isDeepStrictEqual:", util.isDeepStrictEqual({ a: 1 }, { a: 1 }));

  // MIMEType (Node 19+)
  try {
    const mime = new util.MIMEType("text/html; charset=utf-8");
    console.log("MIMEType:", mime.type, mime.subtype, mime.essence);
  } catch {
    console.log("MIMEType: not available");
  }

  // parseArgs (Node 18.3+)
  try {
    const { values, positionals } = util.parseArgs({
      args: ["--name", "node", "--verbose", "file.txt"],
      options: {
        name: { type: "string" },
        verbose: { type: "boolean" },
      },
      allowPositionals: true,
    });
    console.log("parseArgs:", values, positionals);
  } catch (e) {
    console.log("parseArgs:", e.message);
  }

  // styleText (Node 20+)
  try {
    console.log(
      "styleText:",
      util.styleText?.("red", "Red text") || "not available",
    );
  } catch {
    console.log("styleText: not available");
  }
}

// ─────────────────────────────────────────────────────────────────
// 21. ASSERT
// ─────────────────────────────────────────────────────────────────

function demonstrateAssert() {
  console.log("\n═══ 21. ASSERT ═══");

  // Strict mode
  const strict = assert.strict;

  // Basic assertions
  assert.ok(true, "truthy");
  assert.equal(1, 1);
  assert.notEqual(1, 2);
  assert.strictEqual(1, 1);
  assert.notStrictEqual(1, "1");
  assert.deepEqual({ a: 1 }, { a: 1 });
  assert.deepStrictEqual({ a: 1 }, { a: 1 });
  assert.notDeepEqual({ a: 1 }, { a: 2 });
  assert.notDeepStrictEqual({ a: 1 }, { a: "1" });

  // Throws
  assert.throws(() => {
    throw new Error("boom");
  }, Error);
  assert.throws(
    () => {
      throw new Error("boom");
    },
    { message: "boom" },
  );
  assert.doesNotThrow(() => {
    /* no throw */
  });

  // Rejects
  assert.rejects(async () => {
    throw new Error("async boom");
  }, Error);
  assert.doesNotReject(async () => "ok");

  // match / doesNotMatch
  assert.match("Hello World", /World/);
  assert.doesNotMatch("Hello World", /xyz/);

  // fail
  try {
    // assert.fail('Custom failure message');
  } catch (e) {}

  // ifError
  assert.ifError(null); // Passes
  assert.ifError(undefined); // Passes

  console.log("All assertions passed!");
}

// ─────────────────────────────────────────────────────────────────
// 22. V8 ENGINE
// ─────────────────────────────────────────────────────────────────

function demonstrateV8() {
  console.log("\n═══ 22. V8 ENGINE ═══");

  // Heap statistics
  const heapStats = v8.getHeapStatistics();
  console.log(
    "Heap total:",
    (heapStats.total_heap_size / 1024 / 1024).toFixed(2),
    "MB",
  );
  console.log(
    "Heap used:",
    (heapStats.used_heap_size / 1024 / 1024).toFixed(2),
    "MB",
  );
  console.log(
    "Heap limit:",
    (heapStats.heap_size_limit / 1024 / 1024).toFixed(2),
    "MB",
  );
  console.log("Malloced:", heapStats.malloced_memory);
  console.log("External:", heapStats.external_memory);

  // Heap space statistics
  const spaces = v8.getHeapSpaceStatistics();
  spaces.forEach((space) => {
    console.log(
      `  ${space.space_name}: ${(space.space_used_size / 1024).toFixed(0)} KB`,
    );
  });

  // Heap code statistics
  const codeStats = v8.getHeapCodeStatistics();
  console.log("Code size:", codeStats.code_and_metadata_size);

  // Serialization (structured clone)
  const serialized = v8.serialize({ key: "value", arr: [1, 2, 3] });
  console.log("Serialized:", serialized.length, "bytes");
  const deserialized = v8.deserialize(serialized);
  console.log("Deserialized:", deserialized);

  // V8 flags
  // v8.setFlagsFromString('--max-old-space-size=4096');
  console.log("V8 version:", process.versions.v8);

  // writeHeapSnapshot (creates file)
  // v8.writeHeapSnapshot();

  // GC exposure (requires --expose-gc flag)
  // global.gc && global.gc();
}

// ─────────────────────────────────────────────────────────────────
// 23. VM (Virtual Machine / Sandboxing)
// ─────────────────────────────────────────────────────────────────

function demonstrateVM() {
  console.log("\n═══ 23. VM MODULE ═══");

  // Run in new context
  const sandbox = { x: 10, y: 20, console };
  vm.createContext(sandbox);
  vm.runInContext('result = x + y; console.log("VM result:", result)', sandbox);
  console.log("Sandbox result:", sandbox.result);

  // Run in new context (isolated)
  const result = vm.runInNewContext("a * b + c", { a: 5, b: 10, c: 3 });
  console.log("runInNewContext:", result);

  // Run in this context
  vm.runInThisContext("globalThis.__vmTest = 42");
  console.log("runInThisContext:", globalThis.__vmTest);

  // Script (precompiled)
  const script = new vm.Script("x + y", { filename: "test.vm" });
  const ctx = vm.createContext({ x: 100, y: 200 });
  console.log("Script.runInContext:", script.runInContext(ctx));
  console.log(
    "Script.runInNewContext:",
    script.runInNewContext({ x: 1, y: 2 }),
  );

  // Timeout
  try {
    vm.runInNewContext("while(true) {}", {}, { timeout: 50 });
  } catch (e) {
    console.log("VM timeout:", e.message);
  }

  // isContext
  console.log("isContext:", vm.isContext(ctx));
  console.log("isContext plain:", vm.isContext({}));

  // Module (experimental)
  // const mod = new vm.SourceTextModule('export const x = 1');
  console.log("(VM Module API available experimentally)");
}

// ─────────────────────────────────────────────────────────────────
// 24. PERFORMANCE HOOKS
// ─────────────────────────────────────────────────────────────────

function demonstratePerfHooks() {
  console.log("\n═══ 24. PERFORMANCE HOOKS ═══");

  const { performance, PerformanceObserver, monitorEventLoopDelay } =
    perf_hooks;

  // performance.now()
  const start = performance.now();
  for (let i = 0; i < 1000000; i++) {} // Busy work
  const end = performance.now();
  console.log("Elapsed:", (end - start).toFixed(3), "ms");

  // performance.mark & measure
  performance.mark("A");
  for (let i = 0; i < 100000; i++) {}
  performance.mark("B");
  performance.measure("A to B", "A", "B");

  const measures = performance.getEntriesByType("measure");
  measures.forEach((m) => {
    console.log(`${m.name}: ${m.duration.toFixed(3)} ms`);
  });

  // Clear marks
  performance.clearMarks();
  performance.clearMeasures();

  // performance.timeOrigin
  console.log("Time origin:", performance.timeOrigin);

  // performance.toJSON
  console.log("Performance JSON keys:", Object.keys(performance.toJSON()));

  // timerify
  function someFunction() {
    let sum = 0;
    for (let i = 0; i < 10000; i++) sum += i;
    return sum;
  }
  const wrapped = performance.timerify(someFunction);
  wrapped();
  const fnEntries = performance.getEntriesByName("someFunction");
  if (fnEntries.length) {
    console.log("Timerified duration:", fnEntries[0].duration.toFixed(3), "ms");
  }

  // Event loop delay monitoring
  const eld = monitorEventLoopDelay({ resolution: 20 });
  eld.enable();
  setTimeout(() => {
    console.log("Event loop delay min:", eld.min / 1e6, "ms");
    console.log("Event loop delay mean:", (eld.mean / 1e6).toFixed(3), "ms");
    console.log("Event loop delay max:", (eld.max / 1e6).toFixed(3), "ms");
    console.log("Event loop p99:", (eld.percentile(99) / 1e6).toFixed(3), "ms");
    eld.disable();
    eld.reset();
  }, 100);

  // PerformanceObserver
  const obs = new PerformanceObserver((list) => {
    const entries = list.getEntries();
    entries.forEach((entry) => {
      // console.log('Observed:', entry.entryType, entry.name);
    });
  });
  obs.observe({ entryTypes: ["measure", "function"] });

  // Resource timing (Node 18+)
  // performance.getEntriesByType('resource');

  // performance.nodeTiming
  const nt = performance.nodeTiming;
  console.log(
    "Node timing - bootstrap:",
    nt.bootstrapComplete?.toFixed(0),
    "ms",
  );
}

// ─────────────────────────────────────────────────────────────────
// 25. ASYNC HOOKS
// ─────────────────────────────────────────────────────────────────

function demonstrateAsyncHooks() {
  console.log("\n═══ 25. ASYNC HOOKS ═══");

  // AsyncLocalStorage (most practical feature)
  const als = new async_hooks.AsyncLocalStorage();

  als.run({ requestId: "abc-123", userId: "user-1" }, () => {
    const store = als.getStore();
    console.log("AsyncLocalStorage:", store);

    // Available in any nested async call
    setTimeout(() => {
      const nestedStore = als.getStore();
      console.log("Nested ALS:", nestedStore);
    }, 10);
  });

  // enterWith (sets store for current execution)
  // als.enterWith({ key: 'value' });

  // AsyncResource
  class DBQuery extends async_hooks.AsyncResource {
    constructor() {
      super("DBQuery");
    }
    execute(callback) {
      this.runInAsyncScope(callback, null, "query result");
    }
  }
  const query = new DBQuery();
  query.execute((result) => console.log("AsyncResource:", result));

  // createHook (low-level)
  const hook = async_hooks.createHook({
    init(asyncId, type, triggerAsyncId) {
      // Called when async resource is created
    },
    before(asyncId) {
      // Called before async callback
    },
    after(asyncId) {
      // Called after async callback
    },
    destroy(asyncId) {
      // Called when async resource is destroyed
    },
    promiseResolve(asyncId) {
      // Called when promise resolves
    },
  });
  // hook.enable(); hook.disable();

  // executionAsyncId & triggerAsyncId
  console.log("Current async ID:", async_hooks.executionAsyncId());
  console.log("Trigger async ID:", async_hooks.triggerAsyncId());

  // AsyncResource.bind (Node 17+)
  const bound = async_hooks.AsyncResource.bind(() => {
    return "bound context";
  });
  console.log("AsyncResource.bind:", bound());
}

// ─────────────────────────────────────────────────────────────────
// 26. TIMERS
// ─────────────────────────────────────────────────────────────────

async function demonstrateTimers() {
  console.log("\n═══ 26. TIMERS ═══");

  // setTimeout
  const timeoutId = setTimeout(() => console.log("setTimeout fired"), 10);
  console.log("setTimeout ref:", typeof timeoutId);

  // clearTimeout
  const cancelMe = setTimeout(() => console.log("Should not fire"), 1000);
  clearTimeout(cancelMe);

  // setInterval / clearInterval
  let count = 0;
  const intervalId = setInterval(() => {
    count++;
    if (count >= 3) clearInterval(intervalId);
  }, 10);

  // setImmediate / clearImmediate
  const immediateId = setImmediate(() => console.log("setImmediate fired"));

  // ref / unref (prevent/allow process exit)
  const timer = setTimeout(() => {}, 10000);
  timer.unref(); // Won't keep process alive
  timer.ref(); // Will keep process alive
  clearTimeout(timer);

  // timers/promises (Node 16+)
  await timers.setTimeout(10);
  console.log("timers/promises setTimeout resolved");

  // setInterval as async iterator
  const ac = new AbortController();
  setTimeout(() => ac.abort(), 50);
  try {
    let iterCount = 0;
    for await (const _ of timers.setInterval(10, undefined, {
      signal: ac.signal,
    })) {
      iterCount++;
      if (iterCount >= 3) break;
    }
    console.log("timers/promises setInterval iterations:", iterCount);
  } catch (e) {
    if (e.code !== "ABORT_ERR") throw e;
    console.log("setInterval aborted");
  }

  // scheduler.yield() (Node 22+)
  // await timers.scheduler.yield();

  // scheduler.wait() (Node 22+)
  // await timers.scheduler.wait(100);

  await new Promise((r) => setTimeout(r, 100)); // Wait for demos
  console.log("Timer demos complete");
}

// ─────────────────────────────────────────────────────────────────
// 27. PROCESS OBJECT (Deep Dive)
// ─────────────────────────────────────────────────────────────────

function demonstrateProcess() {
  console.log("\n═══ 27. PROCESS OBJECT ═══");

  // Versions
  console.log("Versions:", process.versions);

  // Resource usage
  console.log("Resource usage:", process.resourceUsage());

  // hrtime (high-resolution time)
  const hrStart = process.hrtime.bigint();
  for (let i = 0; i < 100000; i++) {}
  const hrEnd = process.hrtime.bigint();
  console.log(
    "hrtime.bigint:",
    (Number(hrEnd - hrStart) / 1e6).toFixed(3),
    "ms",
  );

  // Legacy hrtime
  const [secs, nanos] = process.hrtime();
  console.log("hrtime:", secs, "s", nanos, "ns");

  // Channel (diagnostics_channel)
  console.log("Channel:", typeof process.channel);

  // config
  console.log("Config:", typeof process.config);

  // Connected (IPC)
  console.log("Connected:", process.connected ?? "N/A (no IPC)");

  // Debug port
  console.log("Debug port:", process.debugPort);

  // Allowed/deny (permission model - Node 20+)
  // process.permission.has('fs.read');

  // Title
  console.log("Title:", process.title);

  // Exit code
  // process.exitCode = 0;

  // Standard streams
  console.log("stdin isTTY:", process.stdin.isTTY);
  console.log("stdout columns:", process.stdout.columns);
  console.log("stderr isTTY:", process.stderr.isTTY);

  // Signal handling
  const handler = () => console.log("SIGUSR2 received");
  process.on("SIGUSR2", handler);
  process.removeListener("SIGUSR2", handler);

  // Event listeners
  process.on("warning", (warning) => {
    // console.log('Warning:', warning.message);
  });

  // process.on('uncaughtException', ...);
  // process.on('unhandledRejection', ...);
  // process.on('beforeExit', ...);
  // process.on('exit', (code) => ...);
  // process.on('SIGINT', ...);
  // process.on('SIGTERM', ...);

  // process.nextTick
  process.nextTick(() => {
    // Runs before I/O, after current operation
  });

  // Emit warning
  process.emitWarning("Demo warning", {
    code: "DEMO_WARNING",
    type: "DeprecationWarning",
  });

  // Report (Node 11+)
  // process.report.writeReport();
  console.log("Report directory:", process.report?.directory);

  // Features
  console.log("Features:", process.features);
}

// ─────────────────────────────────────────────────────────────────
// 28. DIAGNOSTICS CHANNEL (Node 16+)
// ─────────────────────────────────────────────────────────────────

function demonstrateDiagnosticsChannel() {
  console.log("\n═══ 28. DIAGNOSTICS CHANNEL ═══");

  const diagnostics_channel = require("diagnostics_channel");

  // Create a channel
  const channel = diagnostics_channel.channel("my-app:events");

  // Subscribe
  channel.subscribe((message, name) => {
    console.log(`Channel "${name}":`, message);
  });

  // Publish
  if (channel.hasSubscribers) {
    channel.publish({ event: "test", timestamp: Date.now() });
  }

  // TracingChannel (Node 19+)
  try {
    const tracing = diagnostics_channel.tracingChannel("my-app:trace");
    const handler = {
      start(message) {
        console.log("Trace start:", message);
      },
      end(message) {
        console.log("Trace end:", message);
      },
      error(message) {
        console.log("Trace error:", message);
      },
    };
    tracing.subscribe(handler);
    tracing.traceSync(() => "work", { data: "payload" });
    tracing.unsubscribe(handler);
  } catch {
    console.log("TracingChannel: not available");
  }
}

// ─────────────────────────────────────────────────────────────────
// 29. STRING DECODER
// ─────────────────────────────────────────────────────────────────

function demonstrateStringDecoder() {
  console.log("\n═══ 29. STRING DECODER ═══");

  const { StringDecoder } = string_decoder;
  const decoder = new StringDecoder("utf-8");

  // Multi-byte character split across chunks
  const euro = Buffer.from("€"); // 3 bytes: e2 82 ac
  console.log("Euro bytes:", euro);

  // Feed one byte at a time
  console.log("Byte 1:", decoder.write(euro.subarray(0, 1))); // ''
  console.log("Byte 2:", decoder.write(euro.subarray(1, 2))); // ''
  console.log("Byte 3:", decoder.write(euro.subarray(2, 3))); // '€'

  // end() flushes remaining
  const decoder2 = new StringDecoder("utf-8");
  decoder2.write(Buffer.from([0xe2]));
  console.log("end() flush:", JSON.stringify(decoder2.end()));
}

// ─────────────────────────────────────────────────────────────────
// 30. PROMISES & ASYNC PATTERNS
// ─────────────────────────────────────────────────────────────────

async function demonstrateAsyncPatterns() {
  console.log("\n═══ 30. ASYNC PATTERNS ═══");

  // --- Promise basics ---
  const p1 = new Promise((resolve, reject) => resolve("resolved"));
  const p2 = Promise.resolve(42);
  const p3 = Promise.reject(new Error("rejected")).catch((e) => e.message);

  // Promise.all (fails fast)
  const all = await Promise.all([p1, p2, p3]);
  console.log("Promise.all:", all);

  // Promise.allSettled (never rejects)
  const settled = await Promise.allSettled([
    Promise.resolve("ok"),
    Promise.reject("fail"),
    Promise.resolve("also ok"),
  ]);
  console.log(
    "Promise.allSettled:",
    settled.map((s) => s.status),
  );

  // Promise.race (first to settle)
  const race = await Promise.race([
    new Promise((r) => setTimeout(() => r("slow"), 100)),
    new Promise((r) => setTimeout(() => r("fast"), 10)),
  ]);
  console.log("Promise.race:", race);

  // Promise.any (first to fulfill)
  const any = await Promise.any([
    Promise.reject("fail1"),
    new Promise((r) => setTimeout(() => r("success"), 10)),
    Promise.reject("fail2"),
  ]);
  console.log("Promise.any:", any);

  // Promise.withResolvers (Node 22+)
  try {
    const { promise, resolve } = Promise.withResolvers();
    resolve("withResolvers!");
    console.log("Promise.withResolvers:", await promise);
  } catch {
    console.log("Promise.withResolvers: not available");
  }

  // --- Async generators ---
  async function* asyncGenerator() {
    yield "first";
    await new Promise((r) => setTimeout(r, 10));
    yield "second";
    yield "third";
  }

  const genResults = [];
  for await (const val of asyncGenerator()) {
    genResults.push(val);
  }
  console.log("Async generator:", genResults);

  // --- Event loop phases ---
  console.log("Event loop phases:");
  setImmediate(() => console.log("  1. setImmediate"));
  setTimeout(() => console.log("  2. setTimeout 0"), 0);
  process.nextTick(() => console.log("  3. nextTick"));
  queueMicrotask(() => console.log("  4. queueMicrotask"));
  Promise.resolve().then(() => console.log("  5. Promise.then"));

  await new Promise((r) => setTimeout(r, 50));

  // --- AbortController with async operations ---
  const ac = new AbortController();
  const abortableOp = async (signal) => {
    return new Promise((resolve, reject) => {
      if (signal.aborted) return reject(new Error("Already aborted"));
      const timer = setTimeout(() => resolve("completed"), 100);
      signal.addEventListener("abort", () => {
        clearTimeout(timer);
        reject(new Error("Aborted"));
      });
    });
  };
  ac.abort();
  try {
    await abortableOp(ac.signal);
  } catch (e) {
    console.log("AbortController:", e.message);
  }
}

// ─────────────────────────────────────────────────────────────────
// 31. ERROR HANDLING
// ─────────────────────────────────────────────────────────────────

function demonstrateErrors() {
  console.log("\n═══ 31. ERROR HANDLING ═══");

  // Standard Error types
  const errors = [
    new Error("Generic error"),
    new TypeError("Type error"),
    new RangeError("Range error"),
    new ReferenceError("Reference error"),
    new SyntaxError("Syntax error"),
    new URIError("URI error"),
    new EvalError("Eval error"),
  ];

  errors.forEach((e) => {
    console.log(`${e.constructor.name}: ${e.message}`);
  });

  // Error properties
  const err = new Error("Test error");
  console.log("message:", err.message);
  console.log("name:", err.name);
  console.log("stack:", err.stack?.split("\n")[0]);

  // Custom error
  class AppError extends Error {
    constructor(message, code, statusCode) {
      super(message);
      this.name = "AppError";
      this.code = code;
      this.statusCode = statusCode;
      Error.captureStackTrace(this, AppError);
    }
  }

  const appErr = new AppError("Not found", "NOT_FOUND", 404);
  console.log("Custom error:", appErr.name, appErr.code, appErr.statusCode);

  // cause (Error cause chaining - ES2022)
  const rootCause = new Error("Database connection failed");
  const wrappedError = new Error("Failed to fetch user", { cause: rootCause });
  console.log("Error cause:", wrappedError.cause?.message);

  // AggregateError
  const aggErr = new AggregateError(
    [new Error("Error 1"), new Error("Error 2")],
    "Multiple errors",
  );
  console.log("AggregateError:", aggErr.errors.length, "errors");

  // System errors (from Node.js)
  try {
    fs.readFileSync("/nonexistent/path");
  } catch (e) {
    console.log("System error code:", e.code); // ENOENT
    console.log("System error syscall:", e.syscall); // open
    console.log("System error errno:", e.errno);
    console.log("System error path:", e.path);
  }

  // Error.stackTraceLimit
  console.log("Stack trace limit:", Error.stackTraceLimit);

  // Error.prepareStackTrace (V8)
  // Error.prepareStackTrace = (err, stack) => { ... };
}

// ─────────────────────────────────────────────────────────────────
// 32. ES MODULES (ESM) PATTERNS
// ─────────────────────────────────────────────────────────────────

function demonstrateModulePatterns() {
  console.log("\n═══ 32. MODULE PATTERNS ═══");

  // CommonJS (this file uses CJS)
  console.log("Module type: CommonJS");
  console.log("module.filename:", module.filename);
  console.log("module.id:", module.id);
  console.log("module.path:", module.path);
  console.log("module.loaded:", module.loaded);
  console.log("module.children count:", module.children.length);
  console.log("require.main === module:", require.main === module);

  // require.resolve
  console.log("resolve path:", require.resolve("path"));

  // require.cache
  console.log("Cache entries:", Object.keys(require.cache).length);

  // Module wrapper
  console.log("Module wrapper:", require("module").wrapper);

  // Dynamic import (available in CJS too)
  // const { default: pkg } = await import('./package.json', { assert: { type: 'json' } });

  // ESM patterns (shown as reference):
  /*
  // Named exports
  export const name = 'Node';
  export function greet() { return 'Hello'; }

  // Default export
  export default class App {}

  // Named imports
  import { name, greet } from './module.mjs';

  // Default import
  import App from './module.mjs';

  // Namespace import
  import * as mod from './module.mjs';

  // Dynamic import
  const mod = await import('./module.mjs');

  // Import assertions (JSON)
  import data from './data.json' assert { type: 'json' };

  // import.meta
  console.log(import.meta.url);       // file:///path/to/file.mjs
  console.log(import.meta.resolve);   // Resolve specifiers
  console.log(import.meta.dirname);   // Node 21+
  console.log(import.meta.filename);  // Node 21+

  // Top-level await
  const data = await fetch('...');
  */

  console.log("(ESM patterns shown as reference code)");
}

// ─────────────────────────────────────────────────────────────────
// 33. JAVASCRIPT LANGUAGE FEATURES (Modern)
// ─────────────────────────────────────────────────────────────────

function demonstrateModernJS() {
  console.log("\n═══ 33. MODERN JAVASCRIPT FEATURES ═══");

  // --- Destructuring ---
  const { a: x, b: y = 10, ...rest } = { a: 1, c: 3, d: 4 };
  console.log("Object destructure:", x, y, rest);

  const [first, , third, ...remaining] = [1, 2, 3, 4, 5];
  console.log("Array destructure:", first, third, remaining);

  // --- Spread ---
  const merged = { ...{ a: 1 }, ...{ b: 2 }, c: 3 };
  const combined = [...[1, 2], ...[3, 4]];
  console.log("Spread:", merged, combined);

  // --- Optional chaining & nullish coalescing ---
  const obj = { a: { b: { c: 42 } } };
  console.log("Optional chain:", obj?.a?.b?.c);
  console.log("Optional chain miss:", obj?.x?.y?.z);
  console.log("Nullish coalescing:", null ?? "default", 0 ?? "not this");
  console.log("Logical assignment:");
  let la = null;
  la ??= "assigned";
  console.log("  ??=", la);
  let lb = 0;
  lb ||= "fallback";
  console.log("  ||=", lb);
  let lc = 1;
  lc &&= "replaced";
  console.log("  &&=", lc);

  // --- Template literals ---
  const tag = (strings, ...values) =>
    strings.reduce((r, s, i) => r + s + (values[i] || ""), "");
  console.log("Tagged template:", tag`Hello ${"World"}!`);

  // --- Symbol ---
  const sym = Symbol("description");
  const globalSym = Symbol.for("global");
  console.log("Symbol:", sym.toString(), Symbol.keyFor(globalSym));

  // Well-known symbols
  class Iterable {
    *[Symbol.iterator]() {
      yield 1;
      yield 2;
      yield 3;
    }
    [Symbol.toPrimitive](hint) {
      return hint === "number" ? 42 : "Iterable";
    }
    get [Symbol.toStringTag]() {
      return "MyIterable";
    }
  }
  const iter = new Iterable();
  console.log("Symbol.iterator:", [...iter]);
  console.log("Symbol.toPrimitive:", +iter, `${iter}`);
  console.log("Symbol.toStringTag:", Object.prototype.toString.call(iter));

  // --- WeakRef & FinalizationRegistry ---
  let target = { data: "important" };
  const weakRef = new WeakRef(target);
  console.log("WeakRef deref:", weakRef.deref()?.data);

  const registry = new FinalizationRegistry((value) => {
    console.log("Finalized:", value);
  });
  registry.register(target, "cleanup token");

  // --- Proxy & Reflect ---
  const proxy = new Proxy(
    {},
    {
      get(target, prop, receiver) {
        console.log(`  Proxy get: ${String(prop)}`);
        return Reflect.get(target, prop, receiver) ?? `default_${String(prop)}`;
      },
      set(target, prop, value, receiver) {
        console.log(`  Proxy set: ${String(prop)} = ${value}`);
        target[prop] = value;
        return true;
      },
      has(target, prop) {
        return true;
      },
      deleteProperty(target, prop) {
        return Reflect.deleteProperty(target, prop);
      },
      ownKeys(target) {
        return Reflect.ownKeys(target);
      },
      getOwnPropertyDescriptor(target, prop) {
        return (
          Reflect.getOwnPropertyDescriptor(target, prop) || {
            configurable: true,
            enumerable: true,
            value: undefined,
          }
        );
      },
    },
  );
  proxy.name = "test";
  console.log("Proxy result:", proxy.name);
  console.log("Proxy default:", proxy.unknown);

  // --- Collections ---
  // Map
  const map = new Map([
    ["key1", "val1"],
    ["key2", "val2"],
  ]);
  map.set("key3", "val3");
  console.log("Map size:", map.size, "has key1:", map.has("key1"));
  for (const [k, v] of map) {
    /* iterate */
  }

  // Set
  const set = new Set([1, 2, 3, 2, 1]);
  set.add(4);
  console.log("Set size:", set.size, "has 2:", set.has(2));

  // WeakMap & WeakSet
  const wm = new WeakMap();
  const ws = new WeakSet();
  const key = {};
  wm.set(key, "value");
  ws.add(key);
  console.log("WeakMap get:", wm.get(key));
  console.log("WeakSet has:", ws.has(key));

  // --- Classes ---
  class Animal {
    #name; // Private field
    static count = 0; // Static field

    constructor(name) {
      this.#name = name;
      Animal.count++;
    }

    get name() {
      return this.#name;
    }
    set name(val) {
      this.#name = val;
    }

    toString() {
      return `Animal(${this.#name})`;
    }

    static getCount() {
      return Animal.count;
    }

    // Private method
    #validate() {
      return this.#name.length > 0;
    }

    isValid() {
      return this.#validate();
    }
  }

  class Dog extends Animal {
    #breed;

    constructor(name, breed) {
      super(name);
      this.#breed = breed;
    }

    // Static block (ES2022)
    static {
      console.log("  Dog class initialized");
    }

    bark() {
      return `${this.name} says Woof!`;
    }
  }

  const dog = new Dog("Rex", "Shepherd");
  console.log("Class:", dog.bark(), "Count:", Animal.getCount());
  console.log("instanceof:", dog instanceof Dog, dog instanceof Animal);

  // --- Iterators & Generators ---
  function* fibonacci() {
    let [a, b] = [0, 1];
    while (true) {
      yield a;
      [a, b] = [b, a + b];
    }
  }
  const fib = fibonacci();
  const fibNums = Array.from({ length: 8 }, () => fib.next().value);
  console.log("Fibonacci:", fibNums);

  // Generator delegation
  function* inner() {
    yield "a";
    yield "b";
  }
  function* outer() {
    yield* inner();
    yield "c";
  }
  console.log("Delegation:", [...outer()]);

  // --- Array methods ---
  console.log(
    "Array.from:",
    Array.from({ length: 3 }, (_, i) => i * 2),
  );
  console.log("Array.of:", Array.of(1, 2, 3));
  console.log("flat:", [1, [2, [3]]].flat(Infinity));
  console.log(
    "flatMap:",
    [1, 2, 3].flatMap((x) => [x, x * 2]),
  );
  console.log("at:", [1, 2, 3].at(-1));
  console.log(
    "findLast:",
    [1, 2, 3, 4].findLast((x) => x < 3),
  );
  console.log(
    "findLastIndex:",
    [1, 2, 3, 4].findLastIndex((x) => x < 3),
  );

  // Immutable array methods (ES2023)
  try {
    console.log("toSorted:", [3, 1, 2].toSorted());
    console.log("toReversed:", [1, 2, 3].toReversed());
    console.log("toSpliced:", [1, 2, 3, 4].toSpliced(1, 2, "a"));
    console.log("with:", [1, 2, 3].with(1, "replaced"));
  } catch {
    console.log("Immutable array methods: not available");
  }

  // groupBy (ES2024)
  try {
    const grouped = Object.groupBy([1, 2, 3, 4, 5], (n) =>
      n % 2 === 0 ? "even" : "odd",
    );
    console.log("Object.groupBy:", grouped);
  } catch {
    console.log("Object.groupBy: not available");
  }

  // --- Object methods ---
  console.log("Object.entries:", Object.entries({ a: 1, b: 2 }));
  console.log(
    "Object.fromEntries:",
    Object.fromEntries([
      ["a", 1],
      ["b", 2],
    ]),
  );
  console.log("Object.hasOwn:", Object.hasOwn({ a: 1 }, "a"));
  console.log("structuredClone:", structuredClone({ a: 1, b: [2, 3] }));

  // --- String methods ---
  console.log("padStart:", "5".padStart(3, "0"));
  console.log("padEnd:", "hi".padEnd(5, "."));
  console.log("trimStart:", "  hello  ".trimStart());
  console.log("replaceAll:", "aaa".replaceAll("a", "b"));
  console.log(
    "matchAll:",
    [..."test1 test2".matchAll(/test(\d)/g)].map((m) => m[1]),
  );
  console.log("at:", "hello".at(-1));

  // --- RegExp ---
  console.log(
    "Named groups:",
    "YYYY-MM-DD".match(/(?<y>\w+)-(?<m>\w+)-(?<d>\w+)/)?.groups,
  );
  console.log("lookbehind:", "price: $100".match(/(?<=\$)\d+/)?.[0]);
  console.log("dotAll:", /foo.bar/s.test("foo\nbar"));

  // --- Numeric ---
  console.log("BigInt:", 9007199254740991n + 1n);
  console.log("Numeric separator:", 1_000_000);
  console.log("globalThis:", typeof globalThis);

  // --- Logical operators ---
  console.log("Exponent:", 2 ** 10);
}

// ─────────────────────────────────────────────────────────────────
// 34. TYPED ARRAYS
// ─────────────────────────────────────────────────────────────────

function demonstrateTypedArrays() {
  console.log("\n═══ 34. TYPED ARRAYS ═══");

  // All typed array types
  const i8 = new Int8Array(4);
  const u8 = new Uint8Array([10, 20, 30, 40]);
  const u8c = new Uint8ClampedArray([300, -5, 128]); // Clamps to 0-255
  const i16 = new Int16Array(4);
  const u16 = new Uint16Array(4);
  const i32 = new Int32Array(4);
  const u32 = new Uint32Array(4);
  const f32 = new Float32Array([1.1, 2.2, 3.3]);
  const f64 = new Float64Array([1.1, 2.2, 3.3]);
  const bi64 = new BigInt64Array([1n, 2n, 3n]);
  const bu64 = new BigUint64Array([1n, 2n, 3n]);

  console.log("Uint8Array:", u8);
  console.log("Uint8ClampedArray:", u8c); // [255, 0, 128]
  console.log("Float32Array:", f32);
  console.log("BigInt64Array:", bi64);

  // Shared ArrayBuffer
  const sharedBuf = new SharedArrayBuffer(16);
  const sharedView = new Int32Array(sharedBuf);

  // DataView
  const dv = new DataView(new ArrayBuffer(16));
  dv.setInt32(0, 42, true); // little-endian
  dv.setFloat64(4, 3.14, true);
  console.log("DataView int32:", dv.getInt32(0, true));
  console.log("DataView float64:", dv.getFloat64(4, true));

  // Properties
  console.log("BYTES_PER_ELEMENT:", Float64Array.BYTES_PER_ELEMENT);
  console.log("buffer:", u8.buffer);
  console.log("byteLength:", u8.byteLength);
  console.log("byteOffset:", u8.byteOffset);
}

// ─────────────────────────────────────────────────────────────────
// 35. JSON & SERIALIZATION
// ─────────────────────────────────────────────────────────────────

function demonstrateJSON() {
  console.log("\n═══ 35. JSON & SERIALIZATION ═══");

  // JSON.stringify
  const obj = {
    name: "Node",
    version: 20,
    date: new Date(),
    regex: /test/g,
    map: new Map([["a", 1]]),
    set: new Set([1, 2]),
    undefined: undefined,
    func: () => {},
    symbol: Symbol("test"),
    bigint: 42n,
    buffer: Buffer.from("hello"),
  };

  // Replacer function
  const json = JSON.stringify(
    obj,
    (key, value) => {
      if (typeof value === "bigint") return value.toString() + "n";
      if (value instanceof Map) return Object.fromEntries(value);
      if (value instanceof Set) return [...value];
      if (Buffer.isBuffer(value)) return value.toString("base64");
      return value;
    },
    2,
  );
  console.log("JSON.stringify with replacer:\n", json);

  // Replacer array (filter keys)
  console.log("Filtered:", JSON.stringify(obj, ["name", "version"]));

  // JSON.parse with reviver
  const parsed = JSON.parse(
    '{"date":"2024-01-01","num":"42n"}',
    (key, value) => {
      if (key === "date") return new Date(value);
      if (typeof value === "string" && value.endsWith("n"))
        return BigInt(value.slice(0, -1));
      return value;
    },
  );
  console.log("Revived:", parsed);

  // toJSON method
  class Serializable {
    constructor(data) {
      this.data = data;
      this.internal = "hidden";
    }
    toJSON() {
      return { data: this.data };
    }
  }
  console.log("toJSON:", JSON.stringify(new Serializable("visible")));

  // JSON.rawJSON (Stage 3 proposal - Node 21+)
  // const raw = JSON.rawJSON('42');

  // Structured clone (handles more types than JSON)
  const complex = {
    date: new Date(),
    map: new Map(),
    set: new Set(),
    regex: /test/,
  };
  const cloned = structuredClone(complex);
  console.log(
    "structuredClone preserves types:",
    cloned.date instanceof Date,
    cloned.map instanceof Map,
  );
}

// ─────────────────────────────────────────────────────────────────
// 36. WASI (WebAssembly System Interface)
// ─────────────────────────────────────────────────────────────────

function demonstrateWASI() {
  console.log("\n═══ 36. WEBASSEMBLY & WASI ═══");

  // WebAssembly basics
  console.log("WebAssembly available:", typeof WebAssembly !== "undefined");

  // Simple WASM module (add function)
  const wasmBytes = new Uint8Array([
    0x00,
    0x61,
    0x73,
    0x6d, // magic
    0x01,
    0x00,
    0x00,
    0x00, // version
    0x01,
    0x07,
    0x01,
    0x60,
    0x02,
    0x7f,
    0x7f,
    0x01,
    0x7f, // type section
    0x03,
    0x02,
    0x01,
    0x00, // function section
    0x07,
    0x07,
    0x01,
    0x03,
    0x61,
    0x64,
    0x64,
    0x00,
    0x00, // export section
    0x0a,
    0x09,
    0x01,
    0x07,
    0x00,
    0x20,
    0x00,
    0x20,
    0x01,
    0x6a,
    0x0b, // code section
  ]);

  const wasmModule = new WebAssembly.Module(wasmBytes);
  const wasmInstance = new WebAssembly.Instance(wasmModule);
  console.log("WASM add(3, 4):", wasmInstance.exports.add(3, 4));

  // WebAssembly utilities
  console.log("Module exports:", WebAssembly.Module.exports(wasmModule));
  console.log("Module imports:", WebAssembly.Module.imports(wasmModule));
  console.log("Validate:", WebAssembly.validate(wasmBytes));

  // WASI (experimental)
  // const { WASI } = require('wasi');
  // const wasi = new WASI({ version: 'preview1', args: [], env: {} });
  console.log("(WASI available as experimental feature)");
}

// ─────────────────────────────────────────────────────────────────
// 37. TEST RUNNER (Node 18+)
// ─────────────────────────────────────────────────────────────────

function demonstrateTestRunner() {
  console.log("\n═══ 37. TEST RUNNER (node:test) ═══");

  // The built-in test runner (shown as patterns):
  /*
  const { test, describe, it, before, after, beforeEach, afterEach, mock } = require('node:test');

  describe('Math operations', () => {
    let calculator;

    before(() => { calculator = new Calculator(); });
    after(() => { calculator = null; });
    beforeEach(() => { calculator.reset(); });
    afterEach(() => {});

    it('should add numbers', (t) => {
      assert.strictEqual(calculator.add(2, 3), 5);
    });

    it('should handle async', async (t) => {
      const result = await calculator.asyncOp();
      assert.ok(result);
    });

    test('with options', { timeout: 1000, concurrency: 2, skip: false, todo: false }, (t) => {
      // Subtests
      t.test('subtest', () => {});
      t.diagnostic('Extra info');
    });

    // Snapshot testing (Node 22+)
    test('snapshots', (t) => {
      t.assert.snapshot({ key: 'value' });
    });

    // Mocking
    test('mocking', (t) => {
      const fn = t.mock.fn(() => 42);
      fn();
      assert.strictEqual(fn.mock.calls.length, 1);

      // Timer mocking
      t.mock.timers.enable();
      setTimeout(() => {}, 1000);
      t.mock.timers.tick(1000);

      // Module mocking
      t.mock.module('fs', { namedExports: { readFileSync: () => 'mocked' } });
    });
  });

  // Run: node --test file.test.js
  // Run with coverage: node --test --experimental-test-coverage
  // Run with watch: node --test --watch
  */

  console.log("(Test runner patterns shown as reference)");
}

// ─────────────────────────────────────────────────────────────────
// 38. PERMISSION MODEL (Node 20+)
// ─────────────────────────────────────────────────────────────────

function demonstratePermissions() {
  console.log("\n═══ 38. PERMISSION MODEL ═══");

  // Run with: node --experimental-permission --allow-fs-read=* --allow-fs-write=./tmp
  /*
  process.permission.has('fs.read');
  process.permission.has('fs.write', '/tmp');
  process.permission.has('child');
  process.permission.has('worker');
  */

  console.log("(Permission model requires --experimental-permission flag)");
  console.log(
    "Flags: --allow-fs-read, --allow-fs-write, --allow-child-process, --allow-worker",
  );
}

// ─────────────────────────────────────────────────────────────────
// 39. SINGLE EXECUTABLE APPLICATIONS (Node 20+)
// ─────────────────────────────────────────────────────────────────

function demonstrateSEA() {
  console.log("\n═══ 39. SINGLE EXECUTABLE APPS ═══");

  /*
  // 1. Create sea-config.json:
  {
    "main": "app.js",
    "output": "sea-prep.blob",
    "disableExperimentalSEAWarning": true,
    "useSnapshot": false,
    "useCodeCache": true
  }

  // 2. Generate blob:
  // node --experimental-sea-config sea-config.json

  // 3. Copy node binary:
  // cp $(command -v node) myapp

  // 4. Inject blob:
  // npx postject myapp NODE_SEA_BLOB sea-prep.blob --sentinel-fuse NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2

  // 5. Run:
  // ./myapp
  */

  console.log("(SEA shown as build process reference)");
}

// ─────────────────────────────────────────────────────────────────
// 40. NODE.JS CLI FLAGS & ENVIRONMENT
// ─────────────────────────────────────────────────────────────────

function demonstrateCLI() {
  console.log("\n═══ 40. CLI FLAGS & ENV ═══");

  console.log("Key CLI flags (reference):");
  const flags = {
    "--inspect": "Enable debugger on port 9229",
    "--inspect-brk": "Enable debugger and break on start",
    "--prof": "Generate V8 profiler output",
    "--max-old-space-size=N": "Set max heap size in MB",
    "--expose-gc": "Expose global gc() function",
    "--harmony": "Enable staged harmony features",
    "--experimental-vm-modules": "Enable VM ESM modules",
    "--experimental-permission": "Enable permission model",
    "--enable-source-maps": "Enable source map support",
    "--test": "Run test files",
    "--watch": "Watch mode (restart on changes)",
    "--env-file=.env": "Load environment from file",
    "--import": "Preload ESM module",
    "--require": "Preload CJS module",
    "--trace-warnings": "Print stack traces for warnings",
    "--trace-uncaught": "Print stack for uncaught exceptions",
    "--no-warnings": "Suppress warnings",
    "--frozen-intrinsics": "Freeze built-in prototypes",
  };
  Object.entries(flags).forEach(([flag, desc]) => {
    console.log(`  ${flag.padEnd(35)} ${desc}`);
  });

  // Environment variables
  console.log("\nKey environment variables:");
  const envVars = {
    NODE_ENV: process.env.NODE_ENV || "(not set)",
    NODE_DEBUG: process.env.NODE_DEBUG || "(not set)",
    NODE_PATH: process.env.NODE_PATH || "(not set)",
    NODE_OPTIONS: process.env.NODE_OPTIONS || "(not set)",
    UV_THREADPOOL_SIZE: process.env.UV_THREADPOOL_SIZE || "(default: 4)",
  };
  Object.entries(envVars).forEach(([key, val]) => {
    console.log(`  ${key.padEnd(25)} ${val}`);
  });
}

// ═════════════════════════════════════════════════════════════════
// MAIN EXECUTION
// ═════════════════════════════════════════════════════════════════

async function main() {
  console.log(
    "╔══════════════════════════════════════════════════════════════╗",
  );
  console.log(
    "║       NODE.JS COMPREHENSIVE FEATURES DEMONSTRATION          ║",
  );
  console.log(
    `║       Node ${process.version} on ${process.platform} ${process.arch}               `,
  );
  console.log(
    "╚══════════════════════════════════════════════════════════════╝",
  );

  try {
    // Synchronous demos
    demonstrateGlobals();
    demonstrateBuffer();
    demonstratePath();
    demonstrateOS();
    demonstrateEvents();
    demonstrateURL();
    demonstrateCrypto();
    demonstrateReadline();
    demonstrateUtil();
    demonstrateAssert();
    demonstrateV8();
    demonstrateVM();
    demonstratePerfHooks();
    demonstrateAsyncHooks();
    demonstrateProcess();
    demonstrateDiagnosticsChannel();
    demonstrateStringDecoder();
    demonstrateTypedArrays();
    demonstrateJSON();
    demonstrateWASI();
    demonstrateModulePatterns();
    demonstrateModernJS();
    demonstrateErrors();
    demonstrateCluster();
    demonstrateTestRunner();
    demonstratePermissions();
    demonstrateSEA();
    demonstrateCLI();

    // Async demos
    await demonstrateFS();
    await demonstrateStreams();
    await demonstrateHTTP();
    await demonstrateZlib();
    await demonstrateChildProcess();
    await demonstrateWorkerThreads();
    await demonstrateNet();
    await demonstrateDGram();
    await demonstrateDNS();
    await demonstrateTimers();
    await demonstrateAsyncPatterns();

    // Wait for any remaining async operations
    await new Promise((r) => setTimeout(r, 200));

    console.log(
      "\n╔══════════════════════════════════════════════════════════════╗",
    );
    console.log(
      "║                  ALL DEMOS COMPLETED! ✓                     ║",
    );
    console.log(
      "╚══════════════════════════════════════════════════════════════╝",
    );
  } catch (err) {
    console.error("Demo error:", err);
    process.exitCode = 1;
  }
}

main();
