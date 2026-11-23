import { invoke, isTauri } from "@tauri-apps/api/core";
import { afterEach, expect, it, vi } from "vitest";
import {
  confirmProjectApplicationReview, getProjectApplicationReviewContext, verifyProjectApplication
} from "./application-ownership";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), isTauri: vi.fn() }));
afterEach(() => { vi.resetAllMocks(); });

it("calls the native context and passes its opaque digest through both review commands", async () => {
  const projectPath = "C:\\Projects\\fixture";
  const environment = "dev";
  const contextDigest = "c".repeat(64);
  const verificationDigest = "a".repeat(64);
  const request = { projectPath, environment, contextDigest };
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(invoke).mockResolvedValueOnce({ contextDigest });
  const saved = await getProjectApplicationReviewContext(projectPath, environment);
  expect(saved.contextDigest).toBe(contextDigest);
  await verifyProjectApplication(request);
  await confirmProjectApplicationReview(request, verificationDigest);
  expect(vi.mocked(invoke).mock.calls).toEqual([
    ["get_project_application_review_context", { projectPath, environment }],
    ["verify_project_application_ownership", { request }],
    ["confirm_project_application_review", { request, verificationDigest }]
  ]);
});
