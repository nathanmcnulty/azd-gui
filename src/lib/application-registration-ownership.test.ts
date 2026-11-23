import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import { describe, expect, it } from "vitest";
import fixture from "./fixtures/application-registration-ownership.json";

const ajv = new Ajv2020();
addFormats(ajv);
const validate = ajv.compile(JSON.parse(readFileSync("schemas/internal/application-registration-ownership.schema.json", "utf8")));
const { binding, properties, checkedAt, recordedAt } = fixture;
const sha256 = (...parts: unknown[]) => createHash("sha256").update(JSON.stringify(parts)).digest("hex");

function bindingValues(value: Omit<typeof binding, "expectedApplicationId"> & { expectedApplicationId?: string }) {
  return [
    value.provider, value.cloud, value.tenantId, value.subscriptionId, value.environment,
    value.operatorDigest, value.targetResourceDigest, value.sourceContractDigest, value.inputsDigest,
    value.objectId, value.expectedApplicationId ?? null
  ];
}

function verificationDigest(target: typeof binding, checked: string, observed: typeof properties): string {
  return sha256("application-registration-verification", bindingValues(target), checked, [
    observed.objectId, observed.applicationId, observed.singleTenant,
    observed.identifierUriMatches, observed.accessTokenVersion
  ]);
}

const verification = {
  kind: "verification", binding, checkedAt, state: "verified", properties,
  verificationDigest: verificationDigest(binding, checkedAt, properties)
};

function receiptDigest(receipt: {
  binding: typeof binding;
  applicationId: string;
  ownershipSource: string;
  verifiedAt: string;
  recordedAt: string;
  verificationDigest: string;
}): string {
  return sha256("application-registration-ownership-receipt", bindingValues(receipt.binding),
    receipt.applicationId, receipt.ownershipSource, receipt.verifiedAt, receipt.recordedAt,
    receipt.verificationDigest);
}

const receipt = {
  kind: "ownershipReceipt", binding, applicationId: properties.applicationId,
  ownershipSource: "verifiedLegacyMigration", verifiedAt: checkedAt, recordedAt,
  verificationDigest: verification.verificationDigest
};
const receiptFixture = { ...receipt, receiptDigest: receiptDigest(receipt) };

describe("provisional application registration ownership contract", () => {
  it("admits one registered stable-ID verifier and sanitized result and receipt shapes", () => {
    for (const example of [fixture.declaration, verification, receiptFixture]) {
      expect(validate(example), JSON.stringify(validate.errors)).toBe(true);
    }
  });

  it("keeps a reviewed decision distinct from a durable ownership receipt", () => {
    const reviewed = {
      kind: "ownershipReviewDecision", binding, applicationId: properties.applicationId,
      verifiedAt: checkedAt, reviewedAt: recordedAt, verificationDigest: verification.verificationDigest,
      receiptDigest: sha256("application-registration-review-receipt", bindingValues(binding),
        properties.applicationId, checkedAt, recordedAt, verification.verificationDigest)
    };
    expect(validate(reviewed), JSON.stringify(validate.errors)).toBe(true);
    expect(validate({ ...reviewed, ownershipSource: "explicitlyAdopted" })).toBe(false);
    expect(validate({ ...reviewed, kind: "ownershipReceipt" })).toBe(false);
  });

  it("requires a stable object identifier and refuses caller-selected endpoints and names", () => {
    for (const invalid of [
      { kind: "verifier", provider: "entra-application-registration", displayName: "Same name" },
      { ...fixture.declaration, query: "displayName eq 'Same name'" },
      { ...fixture.declaration, uri: "https://example.invalid" },
      { ...fixture.declaration, script: "deploy.ps1" },
      { ...fixture.declaration, provider: "template-provider" },
      { ...verification, binding: { ...binding, objectId: "Same name" } }
    ]) expect(validate(invalid)).toBe(false);
  });

  it.each([
    ["unknown", "unavailable"], ["unknown", "cancelled"], ["unknown", "incomplete"],
    ["unknown", "accessDenied"], ["foreign", "differentTenant"], ["foreign", "differentOperator"],
    ["mismatch", "differentObject"], ["mismatch", "differentApplication"], ["mismatch", "unexpectedProperties"]
  ])("keeps %s/%s distinct from a receipt or verified snapshot", (state, reason) => {
    const rejected = { kind: "verification", state, reason, binding, checkedAt };
    expect(validate(rejected)).toBe(true);
    expect(validate({ ...rejected, properties })).toBe(false);
    expect(validate({ ...rejected, verificationDigest: verification.verificationDigest })).toBe(false);
    expect(validate({ ...rejected, kind: "ownershipReceipt" })).toBe(false);
  });

  it("detects copied context and changed application properties in the proof fingerprints", () => {
    for (const changed of [
      { tenantId: "55555555-5555-4555-8555-555555555555" },
      { subscriptionId: "55555555-5555-4555-8555-555555555555" },
      { operatorDigest: "a".repeat(64) }, { targetResourceDigest: "b".repeat(64) },
      { sourceContractDigest: "c".repeat(64) }, { inputsDigest: "d".repeat(64) },
      { environment: "other" }, { objectId: "55555555-5555-4555-8555-555555555555" }
    ]) {
      const target = { ...binding, ...changed };
      expect(verificationDigest(target, checkedAt, properties)).not.toBe(verification.verificationDigest);
      expect(receiptDigest({ ...receipt, binding: target })).not.toBe(receiptFixture.receiptDigest);
    }
    const drift = { ...properties, accessTokenVersion: 1 };
    expect(verificationDigest(binding, checkedAt, drift)).not.toBe(verification.verificationDigest);
    expect(validate({ ...verification, properties: drift })).toBe(false);
    expect(receiptDigest({ ...receipt, ownershipSource: "explicitlyAdopted" })).not.toBe(receiptFixture.receiptDigest);
  });

  it("rejects raw Graph properties, callback URIs and credentials at every output boundary", () => {
    for (const key of ["displayName", "identifierUris", "callbackUrl", "accessToken", "clientSecret", "rawGraphResponse"]) {
      expect(validate({ ...verification, properties: { ...properties, [key]: "private-fixture" } })).toBe(false);
      expect(validate({ ...receiptFixture, [key]: "private-fixture" })).toBe(false);
      expect(validate({ ...receiptFixture, binding: { ...binding, [key]: "private-fixture" } })).toBe(false);
    }
  });
});
