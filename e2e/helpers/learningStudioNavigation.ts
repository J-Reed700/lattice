import type { Page } from "@playwright/test";

type StudioSection = "Lessons" | "Quizzes" | "Notebook" | "Recall" | "Sources" |
  "Written practice" | "Code & simulations" |
  "Canvas" | "Plan" | "Import & export";

/** Navigate through the same visible groups a learner uses. */
export async function openStudioSection(page: Page, section: StudioSection) {
  const nav = page.getByRole("navigation", { name: "Module workspace" });
  if (["Written practice", "Code & simulations"].includes(section)) {
    await nav.getByRole("tab", { name: "Practice", exact: true }).click();
    const home = page.getByTestId("practice-home");
    const back = page.getByRole("button", { name: "Back to Practice" });
    await home.or(back).first().waitFor();
    if (await back.isVisible()) {
      await back.click();
      await home.waitFor();
    }
    if (section === "Written practice") {
      await page.getByTestId("card-explain").getByRole("button", { name: /^Start/ }).click();
    } else {
      await page.getByRole("button", { name: "Code labs and simulations" }).click();
    }
  } else if (["Canvas", "Plan", "Import & export"].includes(section)) {
    const more = nav.getByRole("button", { name: "More", exact: true });
    if (await more.getAttribute("aria-expanded") !== "true") await more.click();
    await page.getByRole("menuitem", { name: section, exact: true }).click();
  } else {
    await nav.getByRole("tab", { name: section, exact: true }).click();
  }
}
