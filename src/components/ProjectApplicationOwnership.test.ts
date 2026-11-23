import { act, cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, expect, it, vi } from "vitest";
import ProjectApplicationOwnership from "./ProjectApplicationOwnership.svelte";
import * as service from "../lib/application-ownership";
import type { ApplicationVerification } from "../lib/application-ownership";
import fixture from "../lib/fixtures/application-registration-ownership.json";

vi.mock("../lib/application-ownership", () => ({
  verifyProjectApplication: vi.fn(),
  confirmProjectApplicationReview: vi.fn()
}));

const { checkedAt } = fixture;
const binding = { ...fixture.binding, provider: "entra-application-registration", cloud: "AzureCloud" } as const;
const properties = { ...fixture.properties, singleTenant: true, identifierUriMatches: true, accessTokenVersion: 2 } as const;
const props = {
  projectPath: "C:\\Projects\\fixture", environment: binding.environment, tenantId: binding.tenantId,
  objectId: binding.objectId, expectedApplicationId: properties.applicationId, context: "source-a"
};
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
afterEach(() => { cleanup(); vi.resetAllMocks(); });

it("shows exact saved IDs and calls only a one-use review decision after explicit acknowledgement", async () => {
  vi.mocked(service.verifyProjectApplication).mockResolvedValue(verified);
  vi.mocked(service.confirmProjectApplicationReview).mockResolvedValue(receipt);
  render(ProjectApplicationOwnership, { props });
  expect(screen.getByText(binding.objectId)).toBeVisible();
  expect(screen.getByText(binding.tenantId)).toBeVisible();
  expect(service.verifyProjectApplication).not.toHaveBeenCalled();
  expect(decisionButton()).toBeNull();
  await fireEvent.click(verifyButton());
  expect(service.verifyProjectApplication).toHaveBeenCalledExactlyOnceWith(props.projectPath, props.environment);
  expect(await screen.findByText("Matches the expected URI")).toBeVisible();
  expect(decisionButton()).toBeDisabled();
  expect(service.confirmProjectApplicationReview).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole("checkbox", { name: /reviewed this exact verified application/i }));
  await fireEvent.click(decisionButton()!);
  expect(service.confirmProjectApplicationReview).toHaveBeenCalledExactlyOnceWith(props.projectPath, props.environment, verified.verificationDigest);
  expect(await screen.findByText(/Ownership is unchanged/)).toBeVisible();
  expect(decisionButton()).toBeNull();
  expect(screen.getByText("Review receipt")).toBeInTheDocument();
});

it.each([
  [{ state: "unknown", reason: "unavailable" }, /read is unavailable/],
  [{ state: "unknown", reason: "accessDenied" }, /read was denied/],
  [{ state: "foreign", reason: "differentTenant" }, /different tenant/],
  [{ state: "unknown", reason: "cancelled" }, /read was cancelled/],
  [{ ...verified, binding: { ...binding, tenantId: "99999999-9999-4999-8999-999999999999" } }, /did not match/]
] as const)("keeps non-matching or unavailable evidence out of the decision path", async (response, status) => {
  vi.mocked(service.verifyProjectApplication).mockResolvedValue(response);
  render(ProjectApplicationOwnership, { props });
  await fireEvent.click(verifyButton());
  expect(await screen.findByText(status)).toBeVisible();
  expect(decisionButton()).toBeNull();
  expect(service.confirmProjectApplicationReview).not.toHaveBeenCalled();
});

it("discards late and previously acknowledged results when the saved context changes", async () => {
  let finish!: (result: ApplicationVerification) => void;
  vi.mocked(service.verifyProjectApplication).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  const view = render(ProjectApplicationOwnership, { props });
  await fireEvent.click(verifyButton());
  await view.rerender({ ...props, tenantId: "99999999-9999-4999-8999-999999999999" });
  await act(() => finish(verified));
  expect(screen.getByRole("status")).toHaveTextContent(/saved context changed/i);
  expect(decisionButton()).toBeNull();

  await view.rerender(props);
  vi.mocked(service.verifyProjectApplication).mockResolvedValueOnce(verified);
  await fireEvent.click(verifyButton());
  await fireEvent.click(await screen.findByRole("checkbox", { name: /reviewed this exact verified application/i }));
  await view.rerender({ ...props, context: "source-b", inputsSaved: false });
  expect(verifyButton()).toBeDisabled();
  expect(decisionButton()).toBeNull();
  expect(service.confirmProjectApplicationReview).not.toHaveBeenCalled();
});
