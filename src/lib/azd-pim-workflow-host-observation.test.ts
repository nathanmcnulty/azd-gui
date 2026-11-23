import { readFileSync } from "node:fs";
import Ajv2020 from "ajv/dist/2020.js";
import { expect, it } from "vitest";
import observation from "../../src-tauri/tests/fixtures/azd-pim-workflow-host-observation.json";

const validate = new Ajv2020().compile(JSON.parse(
  readFileSync("schemas/internal/azd-pim-workflow-host-observation.schema.json", "utf8")
));

it("accepts the hostname and workflow ID projected by the azd-pim producer fixture", () => {
  expect(validate(observation), JSON.stringify(validate.errors)).toBe(true);
});

it.each([
  "https://example.logic.azure.com",
  "example.logic.azure.com:443",
  "example.logic.azure.com/triggers/manual",
  "example.logic.azure.com?sig=fixture",
  "operator@example.logic.azure.com",
  "EXAMPLE.logic.azure.com",
  "example.logic.azure.com.",
  "example.logic.azure.com\n",
  "127.0.0.1",
  "0x7f.0.0.1",
  "éxample.logic.azure.com",
  "-example.logic.azure.com",
  "example-.logic.azure.com",
  "example..logic.azure.com",
  `${"a".repeat(64)}.logic.azure.com`,
  `${("a".repeat(63) + ".").repeat(4)}com`
])("rejects a noncanonical hostname %j", (hostname) => {
  expect(validate({ ...observation, hostname })).toBe(false);
});

it.each([
  observation.workflowResourceId.replace("microsoft.logic/workflows", "microsoft.web/sites"),
  observation.workflowResourceId.replace("22222222-2222-4222-8222-222222222222", "00000000-0000-0000-0000-000000000000"),
  observation.workflowResourceId.replace("rg-fixture", "rg-fixture."),
  observation.workflowResourceId.replace("rg-fixture", ".."),
  observation.workflowResourceId.replace("microsoft.logic", "Microsoft.Logic"),
  observation.workflowResourceId + "/triggers/manual",
  observation.workflowResourceId + "\n"
])("rejects a noncanonical workflow resource ID %j", (workflowResourceId) => {
  expect(validate({ ...observation, workflowResourceId })).toBe(false);
});

it("rejects producer-supplied provenance and raw endpoint fields", () => {
  expect(validate({ hostname: observation.hostname })).toBe(false);
  for (const field of ["operationId", "sourceContractDigest", "callbackUrl", "identifierUri"]) {
    expect(validate({ ...observation, [field]: "synthetic-value" })).toBe(false);
  }
});
