import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, Link, RouterProvider } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useStudioNavigationGuard } from "@/features/learning/workspace/useStudioNavigationGuard";
import { registerPendingSave } from "@/lib/pendingSaves";


function Studio() {
  const { error } = useStudioNavigationGuard(true);
  return <><h1>Studio editor</h1><Link to="/home">Leave Studio</Link>{error && <p role="alert">{error}</p>}</>;
}
const cleanups: Array<() => void> = [];
afterEach(() => cleanups.splice(0).forEach((cleanup) => cleanup()));

function show() {
  const router = createMemoryRouter([
    { path: "/studio", element: <Studio /> },
    { path: "/home", element: <h1>Home</h1> },
  ], { initialEntries: ["/studio"] });
  render(<RouterProvider router={router} />);
  return router;
}

describe("Studio navigation saves", () => {
  it("keeps the editor mounted until its pending save is acknowledged", async () => {
    let acknowledge!: (saved: boolean) => void;
    const save = vi.fn(() => new Promise<boolean>((resolve) => { acknowledge = resolve; }));
    cleanups.push(registerPendingSave(save));
    show();
    await userEvent.click(screen.getByRole("link", { name: "Leave Studio" }));
    await waitFor(() => expect(save).toHaveBeenCalledOnce());
    expect(screen.getByRole("heading", { name: "Studio editor" })).toBeVisible();
    await act(async () => acknowledge(true));
    expect(await screen.findByRole("heading", { name: "Home" })).toBeVisible();
  });

  it("cancels failed navigation, preserves the editor, and allows another attempt", async () => {
    const save = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    cleanups.push(registerPendingSave(save));
    const router = show();
    await act(async () => { await router.navigate("/home"); });
    expect(await screen.findByRole("alert")).toHaveTextContent("Your work could not be saved");
    expect(screen.getByRole("heading", { name: "Studio editor" })).toBeVisible();
    await userEvent.click(screen.getByRole("link", { name: "Leave Studio" }));
    expect(await screen.findByRole("heading", { name: "Home" })).toBeVisible();
  });
});
