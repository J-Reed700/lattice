import { useEffect, useRef, useState } from "react";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Check,
  ChevronDown,
  Code2,
  ExternalLink,
  LoaderCircle,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";

import VaultAPI from "@/lib/api";
import type {
  LearningContainerEngine,
  LearningBuiltinRuntimeCapability,
  LearningPracticalWorkspaceDto,
  LearningRuntimePresetDto,
} from "@/lib/bindings";

const workspaceKey = (programId: string) =>
  ["learning-practical-workspace", programId] as const;
const catalogKey = ["learning-runtime-catalog"] as const;
const presetOrder = ["csharp", "rust", "node", "python"] as const;
const presetActionNames: Record<(typeof presetOrder)[number], string> = {
  csharp: "C#",
  rust: "Rust",
  node: "React & TypeScript",
  python: "Python",
};
const dockerDocsUrl = "https://docs.docker.com/desktop/";

function uuid() {
  return (
    globalThis.crypto?.randomUUID?.() ?? "00000000-0000-4000-8000-000000000004"
  );
}

function unwrap<T>(
  result: { ok: true; data: T } | { ok: false; error: string },
): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

function errorText(error: unknown) {
  return error instanceof Error
    ? error.message
    : "The request could not be completed.";
}

function presetLabel(preset: LearningRuntimePresetDto) {
  return presetActionNames[preset.id] ?? preset.name;
}

type SetupRequest = {
  operationId: string;
  programId: string;
  profileId: string;
  preset: LearningRuntimePresetDto["id"];
  engine: LearningContainerEngine;
};

export function RuntimeSetupPanel({
  programId,
  workspace,
  onWorkspace,
  openRequest = 0,
}: {
  programId: string;
  workspace: LearningPracticalWorkspaceDto;
  onWorkspace?: (workspace: LearningPracticalWorkspaceDto) => void;
  openRequest?: number;
}) {
  const client = useQueryClient();
  const [expanded, setExpanded] = useState(false);
  const [engine, setEngine] = useState<LearningContainerEngine>("docker");
  const [setupError, setSetupError] = useState<string | null>(null);
  const requestIds = useRef(
    new Map<string, Pick<SetupRequest, "operationId" | "profileId">>(),
  );
  const toggleRef = useRef<HTMLButtonElement | null>(null);
  const previousExpanded = useRef(expanded);
  const capabilities = workspace.runtimeCapabilities;
  const builtinRuntimes = workspace.builtinRuntimes ?? [];
  const readyBuiltinCount = builtinRuntimes.filter(
    (runtime) => runtime.available,
  ).length;
  const enabledProfileCount = workspace.runtimeProfiles.filter(
    (profile) => profile.enabled,
  ).length;
  const availableEngines = capabilities
    .filter((capability) => capability.available)
    .map((capability) => capability.engine);
  const selectedEngine = availableEngines.includes(engine)
    ? engine
    : (availableEngines[0] ?? engine);

  useEffect(() => {
    if (openRequest > 0) setExpanded(true);
  }, [openRequest]);

  useEffect(() => {
    if (!previousExpanded.current && expanded) {
      toggleRef.current?.focus();
    }
    previousExpanded.current = expanded;
  }, [expanded]);

  const catalog = useQuery({
    queryKey: catalogKey,
    queryFn: async () => unwrap(await VaultAPI.getLearningRuntimeCatalog()),
    staleTime: 5 * 60_000,
    retry: false,
  });

  const prepare = useMutation({
    retry: false,
    mutationFn: async (request: SetupRequest) =>
      unwrap(await VaultAPI.prepareLearningRuntimePreset(request)),
    onSuccess: async (nextWorkspace) => {
      setSetupError(null);
      client.setQueryData(workspaceKey(programId), nextWorkspace);
      onWorkspace?.(nextWorkspace);
      await client.invalidateQueries({ queryKey: workspaceKey(programId) });
    },
    onError: (error) => setSetupError(errorText(error)),
  });

  const refreshAvailability = useMutation({
    retry: false,
    mutationFn: async () =>
      unwrap(await VaultAPI.getLearningPracticalWorkspace(programId)),
    onSuccess: (nextWorkspace) => {
      client.setQueryData(workspaceKey(programId), nextWorkspace);
      onWorkspace?.(nextWorkspace);
    },
  });

  const orderedPresets = [...(catalog.data ?? [])].sort((left, right) => {
    const leftIndex = presetOrder.indexOf(
      left.id as (typeof presetOrder)[number],
    );
    const rightIndex = presetOrder.indexOf(
      right.id as (typeof presetOrder)[number],
    );
    return (
      (leftIndex < 0 ? presetOrder.length : leftIndex) -
      (rightIndex < 0 ? presetOrder.length : rightIndex)
    );
  });
  const docker = capabilities.find(
    (capability) => capability.engine === "docker",
  );
  const podman = capabilities.find(
    (capability) => capability.engine === "podman",
  );

  const setupPreset = async (preset: LearningRuntimePresetDto) => {
    const key = `${programId}:${preset.id}:${selectedEngine}`;
    let ids = requestIds.current.get(key);
    if (!ids) {
      ids = { operationId: uuid(), profileId: uuid() };
      requestIds.current.set(key, ids);
    }
    setSetupError(null);
    try {
      await prepare.mutateAsync({
        ...ids,
        programId,
        preset: preset.id,
        engine: selectedEngine,
      });
      requestIds.current.delete(key);
    } catch {
      // Keep both identifiers so retrying the same setup is idempotent.
    }
  };

  return (
    <section
      id="learning-runtime-setup"
      className="rounded-2xl border border-border bg-surface p-4 outline-hidden focus-visible:ring-2 focus-visible:ring-accent"
      aria-labelledby="runtime-setup-title"
    >
      <button
        ref={toggleRef}
        type="button"
        aria-expanded={expanded}
        aria-controls="runtime-setup-content"
        onClick={() => setExpanded((value) => !value)}
        className="flex w-full flex-wrap items-center gap-3 text-left"
      >
        <span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-accent/10 text-accent">
          <Code2 size={17} />
        </span>
        <span className="min-w-40 flex-1">
          <span
            id="runtime-setup-title"
            className="block text-xs font-semibold uppercase tracking-[.14em] text-accent"
          >
            Execution environments
          </span>
          <span className="mt-1 block text-xs leading-5 text-text-secondary">
            Python and JavaScript are included. Add C#, Rust, or React when you need them.
          </span>
        </span>
        <span className="rounded-full bg-background px-2.5 py-1 text-xs text-text-muted">
          {readyBuiltinCount} included · {enabledProfileCount} prepared
        </span>
        <ChevronDown
          size={15}
          className={`shrink-0 text-text-muted transition-transform ${expanded ? "rotate-180" : ""}`}
        />
      </button>

      {expanded && (
        <div
          id="runtime-setup-content"
          className="mt-4 space-y-4 border-t border-border pt-4"
        >
          <div>
            <div className="text-xs font-semibold uppercase tracking-[.12em] text-text-muted">
              Included with Lattice
            </div>
            <div className="mt-2 grid gap-2 sm:grid-cols-2">
              {(["javascript", "python"] as const).map((runtimeId) => {
                const runtime: LearningBuiltinRuntimeCapability | undefined =
                  builtinRuntimes.find(
                    (candidate) => candidate.id === runtimeId,
                  );
                const label =
                  runtimeId === "javascript" ? "JavaScript" : "Python";
                return (
                  <div
                    key={runtimeId}
                    className="rounded-xl border border-border bg-background/45 p-3"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <h3 className="text-sm font-medium text-text-primary">
                        {runtime?.name ?? label}
                      </h3>
                      <span
                        className={`text-xs ${runtime?.available ? "text-emerald-700 dark:text-emerald-300" : "text-text-muted"}`}
                      >
                        {runtime?.available ? "Ready" : "Unavailable"}
                      </span>
                    </div>
                    <p className="mt-1 text-xs leading-4 text-text-muted">
                      {runtime?.description ??
                        "Available inside Lattice for supported practice activities."}
                    </p>
                    {!runtime?.available && runtime?.reason && (
                      <p className="mt-1 text-xs leading-4 text-text-muted">
                        {runtime.reason}
                      </p>
                    )}
                  </div>
                );
              })}
            </div>
          </div>

          <div className="rounded-xl border border-border bg-background/50 p-3">
            <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[.12em] text-text-muted">
              <ShieldCheck size={13} /> Container runtime
              <button
                type="button"
                disabled={refreshAvailability.isPending}
                onClick={() => refreshAvailability.mutate()}
                className="ml-auto inline-flex items-center gap-1 rounded-full border border-border px-2 py-1 text-xs font-medium normal-case tracking-normal text-text-secondary disabled:opacity-50"
              >
                <RefreshCw
                  size={10}
                  className={
                    refreshAvailability.isPending ? "animate-spin" : ""
                  }
                />
                {refreshAvailability.isPending
                  ? "Checking…"
                  : "Refresh availability"}
              </button>
            </div>
            {capabilities.length === 0 ? (
              <p className="mt-2 text-xs leading-5 text-text-muted">
                Runtime availability has not been reported by this device.
              </p>
            ) : (
              <div className="mt-2 grid gap-2 sm:grid-cols-2">
                {capabilities.map((capability) => (
                  <div
                    key={capability.engine}
                    className="rounded-lg bg-surface px-3 py-2"
                  >
                    <div className="flex items-center justify-between gap-2 text-xs">
                      <span className="capitalize text-text-secondary">
                        {capability.engine}
                      </span>
                      <span
                        className={
                          capability.available
                            ? "text-emerald-700 dark:text-emerald-300"
                            : "text-text-muted"
                        }
                      >
                        {capability.available ? "Available" : "Not found"}
                      </span>
                    </div>
                    {capability.version && (
                      <p className="mt-1 text-xs text-text-muted">
                        {capability.version}
                      </p>
                    )}
                    {capability.reason && (
                      <p className="mt-1 text-xs leading-4 text-text-muted">
                        {capability.reason}
                      </p>
                    )}
                  </div>
                ))}
              </div>
            )}
            {docker && !docker.available && (
              <p className="mt-3 text-xs leading-4 text-text-muted">
                C#, Rust, and React projects use Docker or Podman.{" "}
                <a
                  href={dockerDocsUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 font-medium text-accent hover:underline"
                >
                  Docker Desktop setup <ExternalLink size={10} />
                </a>
              </p>
            )}
            {!docker && !podman && (
              <p className="mt-3 text-xs leading-4 text-text-muted">
                Install Docker Desktop to prepare local code environments.{" "}
                <a
                  href={dockerDocsUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 font-medium text-accent hover:underline"
                >
                  Read Docker setup instructions <ExternalLink size={10} />
                </a>
              </p>
            )}
          </div>

          {workspace.runtimeProfiles.length > 0 && (
            <div>
              <h3 className="text-xs font-semibold uppercase tracking-[.12em] text-text-muted">
                Prepared profiles
              </h3>
              <ul className="mt-2 space-y-2">
                {workspace.runtimeProfiles.map((profile) => (
                  <li
                    key={profile.id}
                    className="flex items-center gap-2 rounded-lg border border-border bg-background px-3 py-2"
                  >
                    <span
                      className={`grid h-6 w-6 place-items-center rounded-full ${profile.enabled ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300" : "bg-amber-500/10 text-amber-700"}`}
                    >
                      {profile.enabled ? (
                        <Check size={12} />
                      ) : (
                        <ShieldCheck size={12} />
                      )}
                    </span>
                    <span className="min-w-0 flex-1 truncate text-xs text-text-primary">
                      {profile.name}
                    </span>
                    <span className="text-xs capitalize text-text-muted">
                      {profile.engine}
                    </span>
                    <span
                      className={`text-xs ${profile.enabled ? "text-emerald-700 dark:text-emerald-300" : "text-text-muted"}`}
                    >
                      {profile.enabled ? "Ready" : "Disabled"}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {catalog.isLoading && (
            <p role="status" className="text-xs text-text-muted">
              <LoaderCircle size={13} className="mr-1 inline animate-spin" />
              Loading available environments…
            </p>
          )}
          {catalog.isError && (
            <div className="rounded-lg border border-rose-500/20 bg-rose-500/5 p-3">
              <p role="alert" className="text-xs leading-5 text-rose-700 dark:text-rose-300">
                {errorText(catalog.error)}
              </p>
              <button
                type="button"
                onClick={() => void catalog.refetch()}
                className="mt-2 rounded-full border border-border px-3 py-1.5 text-xs font-medium text-text-secondary"
              >
                Retry loading environments
              </button>
            </div>
          )}
          {catalog.isSuccess && orderedPresets.length === 0 && (
            <p className="rounded-lg bg-background p-3 text-xs leading-5 text-text-muted">
              No language environments are available right now.
            </p>
          )}
          {catalog.isSuccess && orderedPresets.length > 0 && (
            <>
              {availableEngines.length > 1 && (
                <label className="block text-xs font-medium text-text-secondary">
                  Use local engine
                  <select
                    value={selectedEngine}
                    onChange={(event) =>
                      setEngine(event.target.value as LearningContainerEngine)
                    }
                    className="mt-1 block w-full rounded-lg border border-border bg-background px-3 py-2 text-xs"
                  >
                    {availableEngines.map((availableEngine) => (
                      <option key={availableEngine} value={availableEngine}>
                        {availableEngine === "docker" ? "Docker" : "Podman"}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                {orderedPresets.map((preset) => {
                  const label = presetLabel(preset);
                  const matchingProfile = workspace.runtimeProfiles.find(
                    (profile) =>
                      profile.enabled &&
                      profile.engine === selectedEngine &&
                      profile.command.length === preset.command.length &&
                      profile.command.every((part, index) => part === preset.command[index]),
                  );
                  const busy =
                    prepare.isPending &&
                    prepare.variables?.preset === preset.id;
                  return (
                    <article
                      key={preset.id}
                      className="flex min-w-0 flex-col rounded-xl border border-border bg-background/45 p-3"
                    >
                      <h3 className="text-sm font-medium text-text-primary">
                        {preset.name || label}
                      </h3>
                      <p className="mt-1 min-h-10 text-xs leading-4 text-text-muted">
                        {preset.description}
                      </p>
                      {matchingProfile ? (
                        <div className="mt-3 inline-flex items-center gap-1.5 text-xs font-medium text-emerald-700 dark:text-emerald-300">
                          <Check size={12} /> Environment ready
                        </div>
                      ) : (
                        <button
                          type="button"
                          disabled={
                            !availableEngines.length || prepare.isPending
                          }
                          onClick={() => void setupPreset(preset)}
                          className="mt-3 inline-flex w-fit items-center gap-1.5 rounded-full bg-accent px-3 py-2 text-xs font-semibold text-accent-fg disabled:opacity-45"
                        >
                          {busy && (
                            <LoaderCircle size={12} className="animate-spin" />
                          )}
                          {busy ? "Setting up…" : `Set up ${label}`}
                        </button>
                      )}
                    </article>
                  );
                })}
              </div>
            </>
          )}
          {setupError && (
            <p
              role="alert"
              className="rounded-lg bg-rose-500/10 p-3 text-xs leading-5 text-rose-700 dark:text-rose-300"
            >
              {setupError}{" "}
              <button
                type="button"
                onClick={() => {
                  const request = prepare.variables;
                  if (request)
                    void prepare.mutateAsync(request).catch(() => undefined);
                }}
                className="ml-1 font-semibold underline"
              >
                Retry setup
              </button>
            </p>
          )}
          {refreshAvailability.error && (
            <p role="alert" className="text-xs text-rose-700 dark:text-rose-300">
              {errorText(refreshAvailability.error)}
            </p>
          )}
          {!availableEngines.length && catalog.isSuccess && (
            <p className="rounded-lg bg-amber-500/10 p-3 text-xs leading-4 text-text-secondary">
              Start Docker Desktop or another supported local container engine
              to prepare these environments.
            </p>
          )}
        </div>
      )}
    </section>
  );
}
