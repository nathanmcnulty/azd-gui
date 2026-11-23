const assert = require("node:assert/strict");
const { createHash } = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { setTimeout: delay } = require("node:timers/promises");

const component = "synthetic-component";
const projectName = "Unsigned provenance fixture";
const fileName = "component.txt";
const lockName = "azd-components.lock.json";
const guideUrl = "https://github.com/nathanmcnulty/azd-pim/blob/9fd616afe7b278df7a91faa015b6cb6bb32d57dc/docs/component-updates.md";
const hash = (data) => createHash("sha256").update(data).digest("hex");

function seed(project, database) {
  const { DatabaseSync } = require("node:sqlite");
  fs.mkdirSync(project);
  const body = "synthetic local component\n";
  fs.writeFileSync(path.join(project, fileName), body);
  fs.writeFileSync(path.join(project, lockName), JSON.stringify({
    manifestVersion: "1.0",
    components: [{
      id: component,
      version: "1.0.0",
      sourceRepository: "https://example.invalid/components",
      sourceRevision: "a".repeat(40),
      files: [{ source: fileName, target: fileName, sha256: hash(body) }]
    }]
  }));
  const db = new DatabaseSync(database);
  try {
    // Leave user_version unset so native startup owns the rest of schema creation.
    db.exec("CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL UNIQUE, environment TEXT, template_id TEXT, revision TEXT, last_operation TEXT, state TEXT NOT NULL DEFAULT 'ready')");
    db.prepare("INSERT INTO projects (id, name, path) VALUES (?, ?, ?)").run("unsigned154", projectName, project);
  } finally {
    db.close();
  }
}

// Guard both transports before clicking. The custom-protocol reply resolves like
// the opener plugin. The Windows WebView2 fallback is intercepted before native dispatch too.
function installIpcGuard(w, expectedGuide) {
  const pass = Symbol();
  const guide = [];
  const blocked = [];
  const nativeReads = [];
  const allowed = new Set([
    "bootstrap", "get_project_component_provenance", "get_project_template", "list_projects",
    "plugin:event|listen", "plugin:event|unlisten"
  ]);
  const fetchNative = w.fetch.bind(w);
  const webview = w.chrome?.webview;
  if (typeof webview?.postMessage !== "function") throw Error("No Windows IPC fallback to guard");
  const postNative = webview.postMessage.bind(webview);

  function intercept(cmd, getPayload) {
    if (cmd === "plugin:opener|open_url") {
      const url = getPayload()?.url;
      guide.push(url);
      return url === expectedGuide ? { ok: true, data: null } : { ok: false, data: "Unexpected guide" };
    }
    if (cmd === "check_component_updates") {
      return { ok: true, data: { state: "unavailable", checkedAt: null, updates: [] } };
    }
    if (cmd === "dispose_graph_session") return { ok: true, data: null };
    if (allowed.has(cmd)) {
      nativeReads.push(cmd);
      return pass;
    }
    blocked.push(cmd);
    return { ok: false, data: "IPC outside the unsigned smoke scope" };
  }

  function guardedFetch(input, init) {
    const url = new URL(typeof input === "string" ? input : input.url, w.location.href);
    if (url.hostname !== "ipc.localhost") return fetchNative(input, init);
    const cmd = decodeURIComponent(url.pathname.slice(1));
    const result = intercept(cmd, () => JSON.parse(init.body));
    if (result === pass) return fetchNative(input, init);
    return Promise.resolve(new w.Response(JSON.stringify(result.data), {
      headers: { "Content-Type": "application/json", "Tauri-Response": result.ok ? "ok" : "error" }
    }));
  }

  function guardedPost(data) {
    const message = JSON.parse(data);
    const result = intercept(message.cmd, () => message.payload);
    if (result === pass) return postNative(data);
    queueMicrotask(() => w.__TAURI_INTERNALS__.runCallback(result.ok ? message.callback : message.error, result.data));
  }

  w.fetch = guardedFetch;
  webview.postMessage = guardedPost;
  if (w.fetch !== guardedFetch || webview.postMessage !== guardedPost) {
    throw Error("Both Windows IPC transports must be guarded before the guide click");
  }
  w.unsignedSmokeIpc = () => ({ guide: [...guide], blocked: [...blocked], nativeReads: [...nativeReads] });
}

async function selfTest() {
  const native = [];
  const callbacks = [];
  const w = {
    Response,
    location: { href: "http://tauri.localhost/" },
    fetch: (...args) => native.push(args),
    chrome: { webview: { postMessage: (data) => native.push(data) } },
    __TAURI_INTERNALS__: { runCallback: (...args) => callbacks.push(args) }
  };
  installIpcGuard(w, guideUrl);
  const open = "http://ipc.localhost/plugin%3Aopener%7Copen_url";
  const reply = await w.fetch(open, { body: JSON.stringify({ url: guideUrl }) });
  assert.equal(reply.headers.get("Tauri-Response"), "ok");
  assert.equal(await reply.json(), null);
  w.chrome.webview.postMessage(JSON.stringify({ cmd: "plugin:opener|open_url", callback: 1, payload: { url: guideUrl } }));
  w.chrome.webview.postMessage(JSON.stringify({ cmd: "request_graph_session", error: 2 }));
  await w.fetch(open, { body: JSON.stringify({ url: "https://example.invalid/" }) });
  await w.fetch("http://ipc.localhost/get_project_component_provenance", {});
  assert.deepEqual(w.unsignedSmokeIpc().guide, [guideUrl, guideUrl, "https://example.invalid/"]);
  assert.deepEqual(w.unsignedSmokeIpc().blocked, ["request_graph_session"]);
  assert.deepEqual(w.unsignedSmokeIpc().nativeReads, ["get_project_component_provenance"]);
  assert.deepEqual(callbacks, [[1, null], [2, "IPC outside the unsigned smoke scope"]]);
  assert.equal(native.length, 1, "Only local provenance may leave the guard during the self-test");
  Object.defineProperty(w.chrome.webview, "postMessage", { writable: false });
  assert.throws(() => installIpcGuard(w, guideUrl), /Both Windows IPC transports/);
  console.log("Guide IPC capture and fail-closed fallback guard passed");
}

async function until(read, detail) {
  const end = Date.now() + 45_000;
  while (Date.now() < end) {
    const value = await read();
    if (value) return value;
    await delay(250);
  }
  throw Error("Timed out waiting for " + detail);
}

async function probe(project, port) {
  const files = [fileName, lockName];
  const hashes = () => files.map((name) => hash(fs.readFileSync(path.join(project, name))));
  const before = hashes();
  const target = await until(async () => {
    try {
      const response = await fetch("http://127.0.0.1:" + port + "/json/list", { signal: AbortSignal.timeout(2000) });
      return (await response.json()).find((target) => /^https?:\/\/tauri\.localhost\/$/.test(target.url));
    } catch {
      return null;
    }
  }, "the installed WebView2 page");
  const endpoint = new URL(target.webSocketDebuggerUrl);
  assert.equal(endpoint.protocol, "ws:");
  assert.equal(endpoint.hostname, "127.0.0.1");
  assert.equal(endpoint.port, port);
  const ws = new WebSocket(endpoint);
  try {
    await until(() => ws.readyState === WebSocket.OPEN, "loopback debugger handshake");
    let next = 0;
    const pending = new Map();
    ws.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      pending.get(message.id)?.(message);
    });
    const evaluate = async (expression, awaitPromise = true) => {
      const id = ++next;
      const reply = await new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(Error("WebView2 evaluation timed out"));
        }, 20_000);
        pending.set(id, (message) => {
          pending.delete(id);
          clearTimeout(timer);
          resolve(message);
        });
        ws.send(JSON.stringify({ id, method: "Runtime.evaluate", params: { expression, awaitPromise, returnByValue: true } }));
      });
      if (reply.error || reply.result.exceptionDetails) throw Error("WebView2 evaluation failed");
      return reply.result.result.value;
    };
    const invoke = (name, args = {}) =>
      evaluate("window.__TAURI_INTERNALS__.invoke(" + JSON.stringify(name) + "," + JSON.stringify(args) + ")");
    const click = (label) => until(() => evaluate(
      "(() => { const button = [...document.querySelectorAll('button')].find(" +
      "b => !b.disabled && b.textContent.trim() === " + JSON.stringify(label) +
      "); if (!button) return false; button.click(); return true; })()"
    ), label);

    await until(() => evaluate("document.body?.innerText.includes(" + JSON.stringify(projectName) + ")"), "native startup fixture");
    await evaluate("(" + installIpcGuard.toString() + ")(window," + JSON.stringify(guideUrl) + ")");
    const boot = await invoke("bootstrap");
    assert.equal(boot.isNative, true);
    assert.equal(boot.appVersion, "0.1.1");
    assert.equal(boot.projects.find((item) => item.name === projectName)?.path, project);
    const provenance = await invoke("get_project_component_provenance", { projectPath: project });
    assert.equal(provenance.state, "available");
    assert.equal(provenance.components.length, 1);
    const observed = provenance.components[0];
    assert.equal(observed.id, component);
    assert.equal(observed.version, "1.0.0");
    assert.equal(observed.status, "verified");
    assert.equal(observed.files[0].sha256, before[0]);
    assert.equal(observed.files[0].status, "verified");
    const unexpected = path.join(project, "unregistered");
    assert.equal(await evaluate(
      "window.__TAURI_INTERNALS__.invoke('get_project_component_provenance', " +
      JSON.stringify({ projectPath: unexpected }) + ").then(() => false, error => String(error).includes('not registered'))"
    ), true);

    await click("Browse catalog");
    await click("Configure PIM pilot");
    assert.deepEqual((await evaluate("window.unsignedSmokeIpc()")).guide, []);
    await click("Open update guide");
    const receipt = await until(async () => {
      const state = await evaluate("window.unsignedSmokeIpc()");
      return state.guide.length ? state : null;
    }, "guide opener IPC");
    assert.deepEqual(receipt.guide, [guideUrl]);
    assert.deepEqual(receipt.blocked, [], "Only the expected IPC may be attempted");
    assert.ok(receipt.nativeReads.includes("get_project_component_provenance"));
    assert.deepEqual(hashes(), before, "The local fixture files must stay unchanged");
    console.log("Installed native bootstrap, exact local component provenance and pinned guide IPC passed");
  } finally {
    ws.close();
  }
}

async function main() {
  const [mode, first, second] = process.argv.slice(2);
  if (mode === "--self-test") return selfTest();
  if (mode === "--seed") return seed(first, second);
  if (mode === "--probe") return probe(first, second);
  throw Error("Expected --self-test, --seed, or --probe");
}

main().catch((error) => { console.error(error.message); process.exitCode = 1; });
