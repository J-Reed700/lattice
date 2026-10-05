import type { ReactNode } from "react";

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { RuntimeSetupPanel } from "@/features/learning/practice/practical/RuntimeSetupPanel";
import type {
  LearningPracticalWorkspaceDto,
  LearningRuntimePresetDto,
} from "@/lib/bindings";


const mocks = vi.hoisted(() => ({
  catalog: vi.fn(),
  prepare: vi.fn(),
  workspace: vi.fn(),
}));
vi.mock("@/lib/api", () => ({
  default: {
    getLearningRuntimeCatalog: mocks.catalog,
    prepareLearningRuntimePreset: mocks.prepare,
    getLearningPracticalWorkspace: mocks.workspace,
  },
}));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const fail = (error: string) => ({ ok: false as const, error });
const limits = {
  timeoutSeconds: 10,
  memoryMegabytes: 256,
  cpuMillis: 500,
  processLimit: 32,
  outputBytes: 8192,
};
const presets = [
  {
    id: "python",
    name: "Python",
    description: "A friendly first scripting environment.",
    imageRef: "python:stable",
    command: ["python"],
    limits,
    entrypointContract: "run",
  },
  {
    id: "node",
    name: "React & TypeScript",
    description: "Build component projects and run their tests.",
    imageRef: "node:stable",
    command: ["node"],
    limits,
    entrypointContract: "run",
  },
  {
    id: "rust",
    name: "Rust",
    description: "Compile and run Rust programs.",
    imageRef: "rust:stable",
    command: ["rustc"],
    limits,
    entrypointContract: "compile",
  },
  {
    id: "csharp",
    name: "C# / .NET",
    description: "Build and run a .NET project.",
    imageRef: "dotnet:stable",
    command: ["dotnet"],
    limits,
    entrypointContract: "project",
  },
] as unknown as LearningRuntimePresetDto[];
const profile = {
  id: "profile-csharp",
  name: "C# / .NET",
  engine: "docker",
  imageId: "sha256:private",
  command: ["dotnet", "run"],
  limits,
  enabled: true,
  revision: 1,
  createdAt: 1_790_000_000_000,
  updatedAt: 1_790_000_000_000,
};
const workspace = (overrides: Record<string, unknown> = {}) =>
  ({
    programId: "program-1",
    builtinRuntimes: [],
    runtimeCapabilities: [
      { engine: "docker", available: true, version: "Docker 25", reason: null },
      {
        engine: "podman",
        available: false,
        version: null,
        reason: "Podman is not installed.",
      },
    ],
    runtimeProfiles: [],
    activities: [],
    runs: [],
    simulations: [],
    ...overrides,
  }) as LearningPracticalWorkspaceDto;

function renderSetup(
  currentWorkspace = workspace(),
  onWorkspace?: (nextWorkspace: LearningPracticalWorkspaceDto) => void,
) {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: Infinity },
      mutations: { retry: false },
    },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return render(
    <RuntimeSetupPanel
      programId="program-1"
      workspace={currentWorkspace}
      onWorkspace={onWorkspace}
    />,
    { wrapper },
  );
}

describe("Learning Studio runtime setup", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.catalog.mockResolvedValue(ok(presets));
    mocks.prepare.mockResolvedValue(
      ok(workspace({ runtimeProfiles: [profile] })),
    );
    mocks.workspace.mockResolvedValue(ok(workspace()));
  });

  it("shows C# first, keeps execution details out of the cards, and refreshes the workspace after setup", async () => {
    const user = userEvent.setup();
    const onWorkspace = vi.fn();
    renderSetup(workspace(), onWorkspace);
    await user.click(
      screen.getByRole("button", { name: /Execution environments/ }),
    );

    const firstSetupButton = await screen.findByRole("button", {
      name: "Set up C#",
    });
    const card = firstSetupButton.closest("article");
    expect(card).toHaveTextContent("C# / .NET");
    expect(card).not.toHaveTextContent("dotnet:stable");
    expect(card).not.toHaveTextContent("dotnet run");

    await user.click(firstSetupButton);
    await waitFor(() =>
      expect(mocks.prepare).toHaveBeenCalledWith(
        expect.objectContaining({
          programId: "program-1",
          preset: "csharp",
          engine: "docker",
          profileId: expect.any(String),
          operationId: expect.any(String),
        }),
      ),
    );
    await waitFor(() =>
      expect(onWorkspace).toHaveBeenCalledWith(
        expect.objectContaining({ runtimeProfiles: [profile] }),
      ),
    );
  });

  it("retries setup with the same operation and profile identifiers", async () => {
    const user = userEvent.setup();
    mocks.prepare
      .mockResolvedValueOnce(fail("Image could not be prepared."))
      .mockResolvedValueOnce(ok(workspace({ runtimeProfiles: [profile] })));
    renderSetup();
    await user.click(
      screen.getByRole("button", { name: /Execution environments/ }),
    );

    await user.click(await screen.findByRole("button", { name: "Set up C#" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Image could not be prepared.",
    );
    const originalRequest = mocks.prepare.mock.calls[0][0];
    await user.click(screen.getByRole("button", { name: "Retry setup" }));
    await waitFor(() => expect(mocks.prepare).toHaveBeenCalledTimes(2));
    expect(mocks.prepare.mock.calls[1][0]).toEqual(originalRequest);
  });

  it("explains that Docker must be started and links to setup instructions when no engine is available", async () => {
    const user = userEvent.setup();
    renderSetup(
      workspace({
        runtimeCapabilities: [
          {
            engine: "docker",
            available: false,
            version: null,
            reason: "Docker is not installed.",
          },
        ],
      }),
    );
    await user.click(
      screen.getByRole("button", { name: /Execution environments/ }),
    );

    expect(await screen.findByText("Docker is not installed.")).toBeVisible();
    expect(
      screen.getByRole("link", { name: /Docker Desktop setup/ }),
    ).toHaveAttribute("href", "https://docs.docker.com/desktop/");
    expect(
      await screen.findByRole("button", { name: "Set up C#" }),
    ).toBeDisabled();
  });

  it("refreshes daemon availability without leaving the panel", async () => {
    const user = userEvent.setup();
    const onWorkspace = vi.fn();
    const refreshed = workspace({
      runtimeCapabilities: [
        {
          engine: "docker",
          available: true,
          version: "Docker 26",
          reason: null,
        },
      ],
    });
    mocks.workspace.mockResolvedValue(ok(refreshed));
    renderSetup(
      workspace({
        runtimeCapabilities: [
          {
            engine: "docker",
            available: false,
            version: null,
            reason: "Docker is not installed.",
          },
        ],
      }),
      onWorkspace,
    );

    await user.click(
      screen.getByRole("button", { name: /Execution environments/ }),
    );
    await user.click(
      screen.getByRole("button", { name: "Refresh availability" }),
    );
    await waitFor(() => expect(onWorkspace).toHaveBeenCalledWith(refreshed));
    expect(mocks.workspace).toHaveBeenCalledWith("program-1");
  });

  it("identifies a prepared language by its engine and command, not a similar profile name", async () => {
    const user = userEvent.setup();
    const first = renderSetup(workspace({ runtimeProfiles: [profile] }));
    await user.click(screen.getByRole("button", { name: /Execution environments/ }));
    // A custom C# profile with another command does not satisfy the catalog.
    expect(await screen.findByRole("button", { name: "Set up C#" })).toBeEnabled();
    first.unmount();

    renderSetup(workspace({ runtimeProfiles: [{ ...profile, name: "My renamed environment", command: ["dotnet"] }] }));
    await user.click(screen.getByRole("button", { name: /Execution environments/ }));
    expect(await screen.findByText("Environment ready", { exact: true })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Set up C#" })).not.toBeInTheDocument();
  });
});
