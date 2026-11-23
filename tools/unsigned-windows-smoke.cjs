const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");
const { createHash, randomUUID } = require("node:crypto");
const { once } = require("node:events");
const net = require("node:net");
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

// Arm both Tauri transports before the first application script can execute.
function installIpcGuard(w, expectedGuide) {
  const pass = Symbol();
  const guide = [];
  const blocked = [];
  const nativeReads = [];
  let updateChecks = 0;
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
      updateChecks++;
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
    const url = new URL(typeof input === "string" ? input : input.url ?? input.href, w.location.href);
    if (url.origin === w.location.origin) return fetchNative(input, init);
    const ipcOrigin = new URL("/_", w.location.href);
    ipcOrigin.hostname = "ipc.localhost";
    if (url.origin !== ipcOrigin.origin) {
      blocked.push("external WebView fetch");
      return Promise.reject(Error("External requests are disabled in the unsigned smoke"));
    }
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

  const refuseWindow = () => {
    blocked.push("external browser window");
    throw Error("External browser windows are disabled in the unsigned smoke");
  };
  w.fetch = guardedFetch;
  webview.postMessage = guardedPost;
  w.open = refuseWindow;
  if (w.fetch !== guardedFetch || webview.postMessage !== guardedPost || w.open !== refuseWindow) {
    throw Error("Both Windows IPC transports and window.open must be guarded before startup");
  }
  w.unsignedSmokeIpc = () => ({ updateChecks, guide: [...guide], blocked: [...blocked], nativeReads: [...nativeReads] });
}

async function selfTest() {
  const native = [];
  const callbacks = [];
  const w = {
    Response,
    location: { href: "http://tauri.localhost/", origin: "http://tauri.localhost" },
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
  const update = await w.fetch("http://ipc.localhost/check_component_updates", {});
  assert.equal((await update.json()).state, "unavailable");
  await assert.rejects(w.fetch("https://example.invalid/"), /External requests are disabled/);
  assert.throws(() => w.open("https://example.invalid/"), /External browser windows are disabled/);
  await w.fetch("http://ipc.localhost/get_project_component_provenance", {});
  assert.equal(w.unsignedSmokeIpc().updateChecks, 1);
  assert.deepEqual(w.unsignedSmokeIpc().guide, [guideUrl, guideUrl, "https://example.invalid/"]);
  assert.deepEqual(w.unsignedSmokeIpc().blocked, ["request_graph_session", "external WebView fetch", "external browser window"]);
  assert.deepEqual(w.unsignedSmokeIpc().nativeReads, ["get_project_component_provenance"]);
  assert.deepEqual(callbacks, [[1, null], [2, "IPC outside the unsigned smoke scope"]]);
  assert.equal(native.length, 1, "Only local provenance may leave the guard during the self-test");
  Object.defineProperty(w.chrome.webview, "postMessage", { writable: false });
  assert.throws(() => installIpcGuard(w, guideUrl), /Both Windows IPC transports/);
  console.log("Guide IPC capture and fail-closed fallback guard passed");
}

async function until(read, detail, timeoutMs = 45_000) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    const value = await read();
    if (value) return value;
    await delay(250);
  }
  throw Error("Timed out waiting for " + detail);
}

async function withOfflineWebView(port, run) {
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
    const call = async (method, params = {}) => {
      const id = ++next;
      const reply = await new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(Error("WebView2 debugger timed out: " + method));
        }, 20_000);
        pending.set(id, (message) => {
          pending.delete(id);
          clearTimeout(timer);
          resolve(message);
        });
        ws.send(JSON.stringify({ id, method, params }));
      });
      if (reply.error || reply.result.exceptionDetails) throw Error("WebView2 debugger failed: " + method);
      return reply.result;
    };
    const evaluate = async (expression) => (
      await call("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true })
    ).result.value;
    const pausedBeforeApp = "document.readyState === 'loading' && !document.querySelector('#app')?.childElementCount";
    assert.equal(await evaluate(pausedBeforeApp), true, "The app document must be paused before rendering");
    const source = "if (location.hostname === 'tauri.localhost' && !window.unsignedSmokeIpc) { (" +
      installIpcGuard.toString() + ")(window," + JSON.stringify(guideUrl) + "); }";
    const documentGuard = await call("Page.addScriptToEvaluateOnNewDocument", { source, runImmediately: true });
    assert.ok(documentGuard.identifier);
    assert.equal(await evaluate(pausedBeforeApp), true, "Installing the guard must not resume startup");
    assert.deepEqual(await evaluate("window.unsignedSmokeIpc()"), {
      updateChecks: 0, guide: [], blocked: [], nativeReads: []
    });
    await call("Runtime.runIfWaitingForDebugger");

    await run(evaluate);
  } finally {
    ws.close();
  }
}

async function probe(project, port) {
  const files = [fileName, lockName];
  const hashes = () => files.map((name) => hash(fs.readFileSync(path.join(project, name))));
  const before = hashes();
  await withOfflineWebView(port, async (evaluate) => {
    const invoke = (name, args = {}) =>
      evaluate("window.__TAURI_INTERNALS__.invoke(" + JSON.stringify(name) + "," + JSON.stringify(args) + ")");
    const click = (label) => until(() => evaluate(
      "(() => { const button = [...document.querySelectorAll('button')].find(" +
      "b => !b.disabled && b.textContent.trim() === " + JSON.stringify(label) +
      "); if (!button) return false; button.click(); return true; })()"
    ), label);

    await until(() => evaluate("document.body?.innerText.includes(" + JSON.stringify(projectName) + ")"), "native startup fixture");
    const startup = await evaluate("window.unsignedSmokeIpc()");
    assert.equal(startup.updateChecks, 1, "The startup update check must be intercepted before native dispatch");
    assert.ok(startup.nativeReads.includes("bootstrap"), "The UI must bootstrap through native IPC");
    assert.deepEqual(startup.blocked, []);
    const boot = await invoke("bootstrap");
    for (const id of ["azd", "az-cli", "pwsh"]) {
      const tool = boot.tools.find((item) => item.id === id);
      assert.equal(tool?.state, "missing");
      assert.equal(tool?.executablePath, null);
    }
    assert.ok(boot.connections.length > 0 && boot.connections.every((connection) =>
      ["unknown", "disconnected"].includes(connection.state) && !connection.account));
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
    console.log("Startup update stayed local and native bootstrap resolved no tools or connected accounts");
    console.log("Installed app verified registered local provenance and emitted the pinned guide request; the OS handler was stubbed");
  });
}

async function launchOffline(exe, scratch, port, run) {
  const id = randomUUID();
  const pipeName = "\\\\.\\pipe\\WebView2\\Debugger\\" + path.basename(exe) + "\\" + id;
  const sockets = new Set();
  let startupNotice = false;
  const debuggerPipe = net.createServer((socket) => {
    sockets.add(socket);
    socket.on("error", () => {});
    socket.once("data", () => { startupNotice = true; socket.destroy(); });
    socket.once("close", () => sockets.delete(socket));
  });
  // Route WebView requests other than the two Tauri origins to a loopback
  // listener that closes every connection. It cannot proxy or resolve a host.
  const rejectProxy = net.createServer((socket) => socket.destroy());
  let app;
  try {
    debuggerPipe.listen(pipeName);
    await once(debuggerPipe, "listening");
    rejectProxy.listen(0, "127.0.0.1");
    await once(rejectProxy, "listening");
    const noTools = path.join(scratch, "no-tools");
    const temp = path.join(scratch, "runtime-temp");
    fs.mkdirSync(noTools);
    fs.mkdirSync(temp);
    const env = {
      SystemRoot: process.env.SystemRoot,
      SystemDrive: process.env.SystemDrive,
      USERPROFILE: process.env.USERPROFILE,
      APPDATA: process.env.APPDATA,
      LOCALAPPDATA: process.env.LOCALAPPDATA,
      TEMP: temp,
      TMP: temp,
      PATH: noTools,
      PATHEXT: ".EXE;.CMD",
      WEBVIEW2_USER_DATA_FOLDER: path.join(scratch, "webview"),
      WEBVIEW2_WAIT_FOR_SCRIPT_DEBUGGER: "1",
      WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER: id,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: [
        "--remote-debugging-port=" + port,
        "--remote-debugging-address=127.0.0.1",
        "--disable-background-networking",
        "--disable-quic",
        '--host-resolver-rules="MAP * ~NOTFOUND, EXCLUDE tauri.localhost, EXCLUDE ipc.localhost"',
        "--proxy-server=http://127.0.0.1:" + rejectProxy.address().port,
        "--proxy-bypass-list=<-loopback>;tauri.localhost;ipc.localhost"
      ].join(" ")
    };
    app = spawn(exe, { cwd: path.dirname(exe), env, windowsHide: true, stdio: "ignore" });
    await once(app, "spawn");
    await until(() => {
      assert.equal(app.exitCode, null, "Installed app exited before its debugger handshake");
      return startupNotice;
    }, "the pre-script debugger pipe notice");
    await run();
  } finally {
    try {
      if (app?.pid) {
        if (app.exitCode === null && app.signalCode === null) app.kill();
        await until(() => app.exitCode !== null || app.signalCode !== null, "the installed app to exit", 8000);
        await until(() => new Promise((resolve) => {
          const probe = net.connect({ host: "127.0.0.1", port: Number(port) });
          probe.once("connect", () => { probe.destroy(); resolve(false); });
          probe.once("error", (error) => resolve(error.code === "ECONNREFUSED"));
          probe.setTimeout(1000, () => { probe.destroy(); resolve(false); });
        }), "the owned debugger listener to stop", 8000);
      }
    } finally {
      for (const socket of sockets) socket.destroy();
      debuggerPipe.close();
      rejectProxy.close();
    }
  }
}

async function main() {
  const [mode, first, second, exe, scratch] = process.argv.slice(2);
  if (mode === "--self-test") return selfTest();
  if (mode === "--seed") return seed(first, second);
  if (mode === "--probe") return launchOffline(exe, scratch, second, () => probe(first, second));
  throw Error("Expected --self-test, --seed, or --probe");
}

main().catch((error) => { console.error(error.message); process.exitCode = 1; });
