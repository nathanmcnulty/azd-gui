const assert = require("node:assert/strict");
const { createHash } = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { DatabaseSync } = require("node:sqlite");

const [project, database] = process.argv.slice(2);
assert.match(project, /^[A-Za-z]:\\/);
fs.mkdirSync(project);
assert.equal(fs.realpathSync.native(project), project);
const file = "component.txt";
const bytes = "synthetic local component\n";
fs.writeFileSync(path.join(project, file), bytes);
fs.writeFileSync(path.join(project, "azd-components.lock.json"), JSON.stringify({
  manifestVersion: "1.0",
  components: [{
    id: "synthetic-component",
    version: "1.0.0",
    sourceRepository: "https://example.invalid/components",
    sourceRevision: "a".repeat(40),
    files: [{ source: file, target: file, sha256: createHash("sha256").update(bytes).digest("hex") }]
  }]
}));
fs.writeFileSync(path.join(project, "solution-owned.txt"), "the app must preserve this file\n");
const db = new DatabaseSync(database);
try {
  db.exec("CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL UNIQUE, environment TEXT, template_id TEXT, revision TEXT, last_operation TEXT, state TEXT NOT NULL DEFAULT 'ready')");
  db.prepare("INSERT INTO projects (id, name, path) VALUES (?, ?, ?)")
    .run("unsigned154", "Unsigned provenance fixture", "\\\\?\\" + project);
} finally {
  db.close();
}
