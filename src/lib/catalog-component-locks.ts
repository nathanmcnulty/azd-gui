import deploymentCheckLock from "../../catalog/component-locks/deployment-check.json";
import maesterAutomationLock from "../../catalog/component-locks/maester-automation.json";
import pimPilotLock from "../../catalog/component-locks/pim-pilot.json";
import type { CatalogEntry } from "./types";

// Each bundled lock is a byte-for-byte snapshot from its exact reviewed template.
const reviewedLocks = [
  {
    id: "deployment-check",
    repository: "https://github.com/nathanmcnulty/azd-reference",
    commit: "abc6e896620990e68360a63dd14efda66a3f75cd",
    subdirectory: "examples/deployment-check",
    lock: deploymentCheckLock
  },
  {
    id: "maester-automation",
    repository: "https://github.com/nathanmcnulty/azd-maester",
    commit: "1cbaf9064a6ac4c61dd175bf2f61ca08367de0ff",
    subdirectory: "automation-account",
    lock: maesterAutomationLock
  },
  {
    id: "pim-pilot",
    repository: "https://github.com/nathanmcnulty/azd-pim",
    commit: "48bdbadb6a02c06fb4144e616ed9562b905b36ec",
    subdirectory: undefined,
    lock: pimPilotLock
  }
];

export function catalogComponentPins(entry: CatalogEntry) {
  if (entry.source !== "bundled" || entry.trusted !== true) return undefined;
  return reviewedLocks.find((snapshot) =>
    snapshot.id === entry.id && snapshot.repository === entry.repository &&
    snapshot.commit === entry.commit && snapshot.subdirectory === (entry.subdirectory ?? undefined)
  )?.lock.components;
}
