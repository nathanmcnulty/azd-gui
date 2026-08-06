import { describe, expect, it } from "vitest";
import { azurePortalUrl, projectServices } from "./project-state";

const resourceId =
  "/subscriptions/11111111-1111-1111-1111-111111111111/resourceGroups/rg-app/providers/Microsoft.App/containerApps/web";

describe("azd project state", () => {
  it("projects the current azd show contract into safe service results", () => {
    expect(projectServices({
      name: "sample",
      services: {
        web: {
          project: { path: "./src/web", language: "node" },
          target: { resourceIds: [resourceId] },
          ingressUrl: "https://web.example.com"
        }
      }
    })).toEqual([
      {
        name: "web",
        endpoint: "https://web.example.com/",
        language: "node",
        projectPath: "./src/web",
        resources: [{
          id: resourceId,
          portalUrl: `https://portal.azure.com/#resource${resourceId}`,
          name: "web",
          type: "Microsoft.App/containerApps",
          resourceGroup: "rg-app"
        }]
      }
    ]);
  });

  it("supports legacy endpoint fields while rejecting unsafe links", () => {
    const services = projectServices({
      services: {
        api: { ingresUrl: "https://api.example.com" },
        worker: { endpoint: "javascript:alert(1)", target: { resourceIds: ["not-an-arm-id"] } }
      }
    });

    expect(services[0].endpoint).toBe("https://api.example.com/");
    expect(services[1]).toMatchObject({ name: "worker", resources: [] });
    expect(services[1].endpoint).toBeUndefined();
    expect(azurePortalUrl("/subscriptions/not-a-guid/resourceGroups/rg")).toBeNull();
  });
});
