import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import ProjectApplicationOwnership from "./ProjectApplicationOwnership.svelte";
import * as service from "../lib/application-ownership";
import type { ApplicationVerification } from "../lib/application-ownership";
import fixture from "../lib/fixtures/application-registration-ownership.json";

vi.mock("../lib/application-ownership", () => ({
  getProjectApplicationReviewContext: vi.fn(),
  verifyProjectApplication: vi.fn(),
  confirmProjectApplicationReview: vi.fn()
}));

const { checkedAt } = fixture;
const binding = { ...fixture.binding, provider: "entra-application-registration", cloud: "AzureCloud" } as const;
const target = { ...binding, resourceGroup: "rg-fixture", contextDigest: "c".repeat(64) };
const properties = { ...fixture.properties, singleTenant: true, identifierUriMatches: true, accessTokenVersion: 2 } as const;
const props = {
  projectPath: "C:\\Projects\\fixture", environment: binding.environment,
  sourceContractDigest: binding.sourceContractDigest, context: "connection-a"
};
const request = { projectPath: props.projectPath, environment: props.environment, contextDigest: target.contextDigest };
const verified = {
  kind: "verification", state: "verified", binding, properties, checkedAt, verificationDigest: "a".repeat(64)
} as const;
const receipt = {
  kind: "ownershipReviewDecision", binding, applicationId: properties.applicationId,
  verifiedAt: checkedAt, reviewedAt: fixture.recordedAt, verificationDigest: verified.verificationDigest,
  receiptDigest: "b".repeat(64)
} as const;
const verifyButton = () => screen.getByRole("button", { name: "Verify existing application" });
const decisionButton = () => screen.queryByRole("button", { name: "Record review decision" });
beforeEach(() => { vi.mocked(service.getProjectApplicationReviewContext).mockResolvedValue(target); });
afterEach(() => { cleanup(); vi.resetAllMocks(); });

async function ready() {
  await waitFor(() => expect(verifyButton()).toBeEnabled());
}

async function acknowledge() {
  await fireEvent.click(await screen.findByRole("checkbox", { name: /reviewed this exact verified application/i }));
}

it("shows the scoped saved target and calls a single review decision after explicit acknowledgement", async () => {
  vi.mocked(service.verifyProjectApplication).mockResolvedValue(verified);
  vi.mocked(service.confirmProjectApplicationReview).mockResolvedValue(receipt);
  render(ProjectApplicationOwnership, { props });
  await ready();
  expect(service.getProjectApplicationReviewContext).toHaveBeenCalledExactlyOnceWith(props.projectPath, props.environment);
  expect(screen.getByText(binding.objectId)).toBeVisible();
  expect(screen.getByText(binding.tenantId)).toBeVisible();
  expect(screen.getByText(binding.subscriptionId)).toBeVisible();
  expect(screen.getByText(target.resourceGroup)).toBeVisible();
  expect(service.verifyProjectApplication).not.toHaveBeenCalled();
  expect(decisionButton()).toBeNull();
  await fireEvent.click(verifyButton());
  expect(service.verifyProjectApplication).toHaveBeenCalledExactlyOnceWith(request);
  expect(await screen.findByText("Matches the expected URI")).toBeVisible();
  expect(decisionButton()).toBeDisabled();
  expect(service.confirmProjectApplicationReview).not.toHaveBeenCalled();
  await acknowledge();
  await fireEvent.click(decisionButton()!);
  expect(service.confirmProjectApplicationReview).toHaveBeenCalledExactlyOnceWith(request, verified.verificationDigest);
  expect(await screen.findByText(/Ownership is unchanged/)).toBeVisible();
  expect(decisionButton()).toBeNull();
  expect(screen.getByText("Review receipt")).toBeInTheDocument();
});

it.each([
  [{ state: "unknown", reason: "unavailable" }, /read is unavailable/],
  [{ state: "unknown", reason: "providerUnavailable" }, /read without refreshing credentials and an independently verified workflow hostname/],
  [{ state: "unknown", reason: "accessDenied" }, /read was denied/],
  [{ state: "foreign", reason: "differentTenant" }, /different tenant/],
  [{ state: "unknown", reason: "cancelled" }, /read was cancelled/]
] as const)("keeps unverified evidence out of the decision path", async (response, status) => {
  vi.mocked(service.verifyProjectApplication).mockResolvedValue(response);
  render(ProjectApplicationOwnership, { props });
  await ready();
  await fireEvent.click(verifyButton());
  expect(await screen.findByText(status)).toBeVisible();
  expect(decisionButton()).toBeNull();
  expect(service.confirmProjectApplicationReview).not.toHaveBeenCalled();
});

const drift = [
  { tenantId: "99999999-9999-4999-8999-999999999999" },
  { subscriptionId: "99999999-9999-4999-8999-999999999999" },
  { targetResourceDigest: "d".repeat(64) },
  { sourceContractDigest: "e".repeat(64) }
];
it.each(drift)("rejects a changed binding before the decision %j", async (changed) => {
  vi.mocked(service.verifyProjectApplication).mockResolvedValue({ ...verified, binding: { ...binding, ...changed } });
  render(ProjectApplicationOwnership, { props });
  await ready();
  await fireEvent.click(verifyButton());
  expect(await screen.findByText(/did not match this review/)).toBeVisible();
  expect(decisionButton()).toBeNull();
  expect(service.confirmProjectApplicationReview).not.toHaveBeenCalled();
});

it.each(drift)("discards a receipt whose binding changed %j", async (changed) => {
  vi.mocked(service.verifyProjectApplication).mockResolvedValue(verified);
  vi.mocked(service.confirmProjectApplicationReview).mockResolvedValue({ ...receipt, binding: { ...binding, ...changed } });
  render(ProjectApplicationOwnership, { props });
  await ready();
  await fireEvent.click(verifyButton());
  await acknowledge();
  await fireEvent.click(decisionButton()!);
  expect(await screen.findByText(/review could not be recorded/)).toBeVisible();
  expect(decisionButton()).toBeNull();
  expect(screen.queryByText("Review receipt")).not.toBeInTheDocument();
});

it.each(["verify", "confirm"] as const)("reloads changed saved context after native %s rejection and waits for a new explicit action", async (action) => {
  const changedTarget = { ...target, resourceGroup: "rg-changed", targetResourceDigest: "d".repeat(64), contextDigest: "e".repeat(64) };
  vi.mocked(service.getProjectApplicationReviewContext).mockResolvedValueOnce(target).mockResolvedValue(changedTarget);
  if (action === "verify") {
    vi.mocked(service.verifyProjectApplication).mockRejectedValueOnce("The saved application target or source changed.");
  } else {
    vi.mocked(service.verifyProjectApplication).mockResolvedValue(verified);
    vi.mocked(service.confirmProjectApplicationReview).mockRejectedValueOnce("The saved application target or source changed.");
  }
  render(ProjectApplicationOwnership, { props });
  await ready();
  await fireEvent.click(verifyButton());
  if (action === "confirm") {
    await acknowledge();
    await fireEvent.click(decisionButton()!);
    expect(service.confirmProjectApplicationReview).toHaveBeenCalledExactlyOnceWith(request, verified.verificationDigest);
  }
  expect(await screen.findByText("rg-changed")).toBeVisible();
  expect(screen.getByRole("status")).toHaveTextContent(/verify again/i);
  expect(decisionButton()).toBeNull();
  expect(screen.queryByText("Review receipt")).not.toBeInTheDocument();
  expect(service.verifyProjectApplication).toHaveBeenCalledTimes(1);
});

it("discards late and previously acknowledged results when the saved context changes", async () => {
  let finish!: (result: ApplicationVerification) => void;
  vi.mocked(service.verifyProjectApplication).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  const view = render(ProjectApplicationOwnership, { props });
  await ready();
  await fireEvent.click(verifyButton());
  await view.rerender({ ...props, context: "connection-b" });
  await act(() => finish(verified));
  expect(screen.getByRole("status")).toHaveTextContent(/saved context changed/i);
  expect(decisionButton()).toBeNull();

  await view.rerender(props);
  await ready();
  vi.mocked(service.verifyProjectApplication).mockResolvedValueOnce(verified);
  await fireEvent.click(verifyButton());
  await acknowledge();
  await view.rerender({ ...props, inputsSaved: false });
  expect(verifyButton()).toBeDisabled();
  expect(decisionButton()).toBeNull();
  expect(service.confirmProjectApplicationReview).not.toHaveBeenCalled();
});
