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
