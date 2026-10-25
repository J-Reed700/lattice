import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { LearningPracticalActivityDto } from "@/lib/bindings";

import { ActivityBrief } from "./ActivityBrief";

vi.mock("@/components/TiptapEditor", () => ({ TiptapViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const activity: LearningPracticalActivityDto = {
  id: "interview", kind: "interview", title: "Acme interview", brief: "Reason through a scenario.",
  programId: "program", lessonId: "lesson", predecessorId: null, status: "ready",
  revision: 0, generatorModel: "model", practiceMode: "practice", allowedAids: [], rubric: [], runtimeKind: "none",
  outcomeIds: [], sourceVersionIds: [], runtimeProfileId: null, runtimeEngine: null, runtimeImageId: null,
  runtimeCommand: null, runtimeLimits: null, runtimeAvailable: false, runtimeUnavailableReason: null, files: [], createdAt: 0, updatedAt: 0,
  interviewPlan: {
    companyName: "Acme", companyUrl: "https://acme.example", targetRole: "Engineer", roundKind: "full_loop",
    companySummary: "Preparation dossier", researchedSourceVersionIds: ["version-1"],
    learnerRole: "Candidate", counterpartRole: "Interviewer", questions: [{
      id: "q1", round: "system_design", question: "How would you design a queue?", companyRationale: "Company constraints",
      followUpProbes: ["What about overload?"], evaluationSignals: ["Bounded backpressure"],
    }],
  },
};

describe("Interview preparation material", () => {
  it("withholds the dossier and question bank during demonstration", () => {
    render(<ActivityBrief activeActivity={{ ...activity, practiceMode: "demonstrate" }} />);
    expect(screen.getByRole("heading", { name: "Acme" })).toBeVisible();
    expect(screen.queryByText("Preparation dossier")).not.toBeInTheDocument();
    expect(screen.queryByText("How would you design a queue?")).not.toBeInTheDocument();
    expect(screen.queryByText("Company constraints")).not.toBeInTheDocument();
    expect(screen.getByText(/hidden during unassisted demonstration/)).toBeVisible();
  });

  it("does not render unsafe website links from imported plans", () => {
    render(<ActivityBrief activeActivity={{ ...activity, interviewPlan: { ...activity.interviewPlan!, companyUrl: "javascript:alert(1)" } }} />);
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(screen.getByText("Preparation dossier")).toBeVisible();
  });
});
