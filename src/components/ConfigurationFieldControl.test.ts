import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";
import ConfigurationFieldControl from "./ConfigurationFieldControl.svelte";

describe("configuration field control", () => {
  it("renders multiselect choices and their descriptions", async () => {
    render(ConfigurationFieldControl, {
      field: {
        id: "services",
        environmentVariable: "SERVICES",
        label: "Services",
        type: "multiselect",
        options: [
          { label: "Web", value: "web", description: "Include the web service." },
          { label: "Worker", value: "worker", description: "Include background processing." }
        ]
      },
      value: ["web"]
    });

    expect(screen.getByRole("checkbox", { name: /web/i })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: /worker/i })).not.toBeChecked();
    expect(screen.getByText("Include the web service.")).toBeInTheDocument();
    expect(screen.getByText("Include background processing.")).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("checkbox", { name: /worker/i }));
    expect(screen.getByRole("checkbox", { name: /worker/i })).toBeChecked();
  });
});

it("uses an accessible required checkbox and specific validation message for acknowledgements", async () => {
  const { rerender } = render(ConfigurationFieldControl, { field: {
    id: "ack", environmentVariable: "ACK_ALL", type: "acknowledgement", required: true,
    label: "I accept all-role scope", condition: { field: "scope", equals: "all" }, validationMessage: "Accept every role before deployment."
  }, value: "true" });
  const checkbox = screen.getByRole("checkbox", { name: /I accept all-role scope/ });
  expect(checkbox).not.toBeChecked();
  expect(checkbox).toBeRequired();
  expect(checkbox).toHaveAttribute("aria-invalid", "true");
  expect(checkbox).toHaveAccessibleDescription("Accept every role before deployment.");
  await rerender({ value: true });
  expect(checkbox).toBeChecked();
  expect(checkbox).toHaveAttribute("aria-invalid", "false");
});

it("cannot visually accept an acknowledgement without a tenant and controlling context", async () => {
  render(ConfigurationFieldControl, { field: { id: "unavailable-ack", environmentVariable: "ACK", type: "acknowledgement", required: true, label: "Accept scope", condition: { field: "scope", equals: "all" } }, value: false, acknowledgementAvailable: false });
  const checkbox = screen.getByRole("checkbox", { name: /Accept scope/ });
  expect(checkbox).toBeDisabled();
  expect(checkbox).not.toBeChecked();
  expect(checkbox).toHaveAccessibleDescription("Select an available tenant and controlling scope before acknowledging.");
});
