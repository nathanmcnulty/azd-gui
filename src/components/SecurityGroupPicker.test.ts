import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import SecurityGroupPicker from "./SecurityGroupPicker.svelte";

const search = vi.hoisted(() => vi.fn());
vi.mock("../lib/bridge", () => ({ searchSecurityGroups: search }));
afterEach(() => { cleanup(); search.mockReset(); });
const target = { tenantId: "11111111-1111-4111-8111-111111111111", subscriptionId: "22222222-2222-4222-8222-222222222222" };
const group = { id: "33333333-3333-4333-8333-333333333333", displayName: "Admin group" };
async function submit() {
  await fireEvent.input(screen.getByLabelText("Find a security group by name"), { target: { value: "Ad" } });
  await fireEvent.click(screen.getByRole("button", { name: "Search groups" }));
}

describe("security group picker", () => {
  it("searches explicitly and requires selection instead of choosing the first match", async () => {
    search.mockResolvedValue({ ...target, groups: [group], hasMore: true });
    const changed = vi.fn();
    render(SecurityGroupPicker, { props: { target }, events: { change: changed } });
    expect(search).not.toHaveBeenCalled();
    await submit();
    const select = await screen.findByRole("button", { name: /Select Admin group/ });
    expect(search).toHaveBeenCalledWith(target, "Ad");
    expect(changed).not.toHaveBeenCalled();
    expect(screen.getByText(/More matches are available/)).toBeInTheDocument();
    await fireEvent.click(select);
    expect(changed.mock.calls[0][0].detail).toBe(group.id);
  });

  it("discards results when the query changes during a search", async () => {
    let resolve!: (value: unknown) => void;
    search.mockReturnValue(new Promise((done) => { resolve = done; }));
    render(SecurityGroupPicker, { props: { target } });
    await submit();
    await fireEvent.input(screen.getByLabelText("Find a security group by name"), { target: { value: "Other" } });
    resolve({ ...target, groups: [group], hasMore: false });
    await waitFor(() => expect(screen.getByRole("button", { name: "Search groups" })).toBeEnabled());
    expect(screen.queryByText(group.displayName)).not.toBeInTheDocument();
  });

  it("rejects a mismatched target and clears results when the selected target changes", async () => {
    search.mockResolvedValueOnce({ ...target, tenantId: "44444444-4444-4444-8444-444444444444", groups: [group], hasMore: false });
    const view = render(SecurityGroupPicker, { props: { target } });
    await submit();
    expect(await screen.findByText(/search target changed/i)).toBeInTheDocument();
    expect(screen.queryByText(group.displayName)).not.toBeInTheDocument();
    search.mockResolvedValueOnce({ ...target, groups: [group], hasMore: false });
    await submit();
    await screen.findByText(group.displayName);
    await view.rerender({ target: { ...target, subscriptionId: "55555555-5555-4555-8555-555555555555" } });
    expect(screen.queryByText(group.displayName)).not.toBeInTheDocument();
  });

  it("keeps provider failure details out of the UI and offers manual entry", async () => {
    search.mockRejectedValue(new Error("Bearer secret-value"));
    render(SecurityGroupPicker, { props: { target } });
    await submit();
    expect(await screen.findByText(/Group search is unavailable/)).toBeInTheDocument();
    expect(screen.queryByText(/secret-value/)).not.toBeInTheDocument();
  });
});

