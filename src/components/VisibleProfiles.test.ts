import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import VisibleProfiles from "./VisibleProfiles.svelte";

const list = vi.hoisted(() => vi.fn());
vi.mock("../lib/bridge", () => ({ listVisibleProfiles: list }));
afterEach(() => { cleanup(); list.mockReset(); });
const profile = { account: "nora@example.test", tenantId: "11111111-1111-4111-8111-111111111111", subscriptionId: "22222222-2222-4222-8222-222222222222", subscriptionName: "Lab" };
const inventory = { providerId: "azureCli", state: "available", profiles: [profile], hasMore: false };
async function expand() {
  const details = screen.getByText("Locally visible profiles").closest("details")!;
  details.open = true;
  await fireEvent(details, new Event("toggle"));
}

describe("locally visible profile hints", () => {
  it("loads only on expansion and presents hints without account switching", async () => {
    list.mockResolvedValue(inventory);
    render(VisibleProfiles, { providerId: "azureCli" });
    expect(list).not.toHaveBeenCalled();
    await expand();
    await screen.findByText(profile.account);
    expect(list).toHaveBeenCalledWith("azureCli");
    expect(screen.getByText("Unverified profile hint")).toBeInTheDocument();
    expect(screen.getAllByRole("button")).toHaveLength(1);
    expect(screen.queryByRole("button", { name: profile.account })).not.toBeInTheDocument();
    expect(screen.getByText(/This list may be incomplete/)).toBeInTheDocument();
  });

  it("keeps provider errors private and permits an explicit retry", async () => {
    list.mockRejectedValueOnce(new Error("Bearer secret-value"));
    render(VisibleProfiles, { providerId: "azPowerShell" });
    await expand();
    await screen.findByText(/Visible profiles are unavailable/);
    expect(screen.queryByText(/secret-value/)).not.toBeInTheDocument();
    list.mockResolvedValueOnce({ ...inventory, providerId: "azPowerShell", profiles: [], hasMore: true });
    await fireEvent.click(screen.getByRole("button", { name: "Refresh visible profiles" }));
    await screen.findByText(/No local profiles were reported/);
    expect(screen.getByText(/Showing at most 100 profiles/)).toBeInTheDocument();
  });

  it("discards an in-flight inventory and old hints when the effective context changes", async () => {
    let resolve!: (value: unknown) => void;
    list.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
    const view = render(VisibleProfiles, { providerId: "azureCli", contextKey: "old-account" });
    await expand();
    await view.rerender({ providerId: "azureCli", contextKey: "new-account" });
    resolve(inventory);
    await waitFor(() => expect(screen.getByRole("button", { name: "Refresh visible profiles" })).toBeEnabled());
    expect(screen.queryByText(profile.account)).not.toBeInTheDocument();
    list.mockResolvedValueOnce(inventory);
    await fireEvent.click(screen.getByRole("button", { name: "Refresh visible profiles" }));
    await screen.findByText(profile.account);
    await view.rerender({ providerId: "azPowerShell", contextKey: "new-account" });
    expect(screen.queryByText(profile.account)).not.toBeInTheDocument();
  });

  it("rejects a result attributed to a different provider", async () => {
    list.mockResolvedValue({ ...inventory, providerId: "azPowerShell" });
    render(VisibleProfiles, { providerId: "azureCli" });
    await expand();
    await screen.findByText(/Visible profiles are unavailable/);
    expect(screen.queryByText(profile.account)).not.toBeInTheDocument();
  });
});
