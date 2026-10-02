import type { Page } from "@playwright/test";

type StudioSection = "Lessons" | "Notebook" | "Recall" | "Sources" |
  "Written practice" | "Code & simulations" | "Quick checks" | "Assessments" |
  "Canvas" | "Plan" | "Import & export";

/** Navigate through the same visible groups a learner uses. */
export async function openStudioSection(page: Page, section: StudioSection) {
  const nav = page.getByRole("navigation", { name: "Module workspace" });
  if (["Written practice", "Code & simulations", "Quick checks", "Assessments"].includes(section)) {
    await nav.getByRole("tab", { name: "Practice", exact: true }).click();
    await page.getByRole("tablist", { name: "Practice activities" })
      .getByRole("tab", { name: section, exact: true }).click();
  } else if (["Canvas", "Plan", "Import & export"].includes(section)) {
    const more = nav.getByRole("button", { name: "More", exact: true });
    if (await more.getAttribute("aria-expanded") !== "true") await more.click();
    await page.getByRole("menuitem", { name: section, exact: true }).click();
  } else {
    await nav.getByRole("tab", { name: section, exact: true }).click();
  }
}
