# Example Document with @-mentions and [[wikilinks]]

This document demonstrates the usage of @-mentions and [[wikilinks]] in the Recall/Vault application.

## Project Meeting Notes

### Date: 2024-01-15

**Attendees:**
- @[Sarah Chen] - Project Manager
- @[Michael Rodriguez] - Lead Developer
- @[Emily Watson] - UX Designer
- @[David Kim] - QA Engineer

### Agenda

1. **Review Progress** on [[Q1 Deliverables]]
2. **Discuss** [[API Architecture Refactoring]]
3. **Plan** [[User Testing Sessions]]
4. **Address** technical debt in [[Authentication System]]

### Discussion Summary

@[Sarah Chen] opened the meeting by reviewing our progress on the [[Q1 Deliverables]]. We're currently at 65% completion, which is slightly behind schedule. The main blocker is the [[Database Migration]] that @[Michael Rodriguez] is working on.

@[Michael Rodriguez] provided an update on [[API Architecture Refactoring]]. He mentioned that the new [[REST API Endpoints]] are ready for testing. He's collaborating with @[David Kim] to ensure comprehensive [[API Test Coverage]].

@[Emily Watson] presented the updated [[UI Mockups]] for the [[Dashboard Redesign]]. The team agreed that the new design better reflects our [[Design System]]. She's also working on [[User Flow Diagrams]] to clarify the [[Onboarding Experience]].

@[David Kim] raised concerns about [[Browser Compatibility]] issues found during testing of the [[Search Feature]]. He's documented these in [[Bug Report - Search Issues]]. @[Michael Rodriguez] agreed to investigate and fix these by end of week.

### Action Items

- [ ] @[Michael Rodriguez]: Complete [[Database Migration]] by Friday
- [ ] @[David Kim]: Run [[API Test Suite]] on staging environment
- [ ] @[Emily Watson]: Finalize [[Design Specifications]] for [[Mobile View]]
- [ ] @[Sarah Chen]: Schedule [[Client Demo]] for next week
- [ ] Everyone: Review [[Code Review Guidelines]] before next sprint

### Related Documents

- [[Previous Meeting - 2024-01-08]]
- [[Project Timeline]]
- [[Resource Allocation]]
- [[Risk Assessment]]

---

## Technical Research: Machine Learning

### Authors

Research conducted by @[Dr. Jennifer Liu] and @[Dr. Ahmed Hassan]

### Key Concepts

This research explores several fundamental concepts in @[Machine Learning]:

1. **[[Neural Networks]]** - Computational models inspired by biological neurons
2. **[[Deep Learning]]** - Subset of machine learning using multi-layer neural networks
3. **[[Convolutional Neural Networks]]** - Specialized for processing grid-like data
4. **[[Recurrent Neural Networks]]** - Designed for sequential data processing
5. **[[Transfer Learning]]** - Using pre-trained models for new tasks

### Applications

We're applying @[Natural Language Processing] techniques to analyze:
- [[Customer Feedback Analysis]]
- [[Sentiment Detection]]
- [[Topic Modeling]]
- [[Text Classification]]

@[Dr. Ahmed Hassan] has made significant progress on the [[NLP Pipeline]] and has documented the approach in [[Technical Specification - NLP]].

### Collaboration

We're working with:
- @[Stanford AI Lab] on [[Vision Research]]
- @[MIT Media Lab] on [[Robotics Applications]]
- @[OpenAI] on [[Language Models]]

### References

See also:
- [[Papers We Love - ML Edition]]
- [[ML Course Resources]]
- [[Recommended Reading List]]
- [[Code Examples Repository]]

---

## Business Strategy

### Stakeholders

- @[CEO] - @[Robert Anderson]
- @[CFO] - @[Linda Martinez]
- @[CTO] - @[James Thompson]
- @[VP of Sales] - @[Patricia Brown]
- @[VP of Marketing] - @[Christopher Lee]

### Strategic Initiatives

Our [[2024 Strategic Plan]] focuses on three key areas:

1. **Market Expansion**
   - [[International Markets]]
   - [[Product Localization]]
   - Partner with @[Enterprise Clients]
   - Target @[SMB Segment]

2. **Product Development**
   - Launch [[Premium Tier]]
   - Enhance [[Mobile App]]
   - Build [[API Platform]]
   - Integrate with [[Third-Party Tools]]

3. **Operational Excellence**
   - Improve [[Customer Support]]
   - Streamline [[Sales Process]]
   - Optimize [[Marketing Funnel]]
   - Scale [[Infrastructure]]

### Financial Projections

As discussed with @[CFO], our [[Q1 Budget]] allocates resources to:
- [[R&D Investment]]
- [[Marketing Campaigns]]
- [[Sales Team Expansion]]
- [[Infrastructure Upgrades]]

See [[Financial Model 2024]] for detailed projections.

### Competitive Analysis

Key competitors tracked in [[Competitive Landscape]]:
- @[Competitor A] - Strong in @[Enterprise Space]
- @[Competitor B] - Focus on @[Developer Tools]
- @[Competitor C] - Leading @[Consumer Market]

Our differentiation strategy is detailed in [[Positioning Document]].

---

## Knowledge Base Structure

### How to Use This System

This knowledge base uses **wikilinks** and **@-mentions** to create an interconnected web of information:

**Wikilinks** `[[Page Title]]`:
- Link to other notes and documents
- Create bidirectional connections
- Build a knowledge graph
- Navigate between related topics

**@-mentions** `@[Name]`:
- Tag people: `@[Alice Johnson]`
- Tag concepts: `@[Artificial Intelligence]`
- Tag organizations: `@[Stanford University]`
- Track relationships and references

### Example Patterns

**Meeting Templates:**
```markdown
# Meeting - [Date]
**Attendees:** @[Person1], @[Person2]
**Topics:** [[Topic1]], [[Topic2]]
**Action Items:** @[Owner] to complete [[Task]]
**Next Meeting:** [[Follow-up Meeting]]
```

**Project Templates:**
```markdown
# Project [Name]
**Lead:** @[Project Manager]
**Team:** @[Dev1], @[Dev2], @[Designer]
**Documents:** [[Spec]], [[Design]], [[Timeline]]
**Status:** Link to [[Status Report]]
**Related:** [[Related Project]]
```

**Research Templates:**
```markdown
# Research: [Topic]
**Authors:** @[Researcher1], @[Researcher2]
**Concepts:** @[Concept1], @[Concept2]
**Papers:** [[Paper1]], [[Paper2]]
**Applications:** [[Use Case1]], [[Use Case2]]
**References:** [[Resource1]], [[Resource2]]
```

---

## Tips for Effective Usage

### 1. Consistent Naming

Use full names consistently:
- ✅ Always use `@[Sarah Chen]`
- ❌ Don't mix `@[Sarah]`, `@[S. Chen]`, `@[Sarah C.]`

### 2. Descriptive Wikilinks

Be specific with note titles:
- ✅ `[[Q1 2024 Marketing Strategy]]`
- ❌ `[[Strategy]]` (too vague)

### 3. Natural Integration

Integrate mentions naturally in sentences:
- ✅ "According to @[Dr. Smith], the [[Research Paper]] shows..."
- ❌ "According to @[Dr. Smith] [[Research Paper]] @[Machine Learning]"

### 4. Hierarchical Structure

Organize notes hierarchically:
```
[[Projects]]
  ├── [[Project Alpha]]
  │   ├── [[Alpha - Requirements]]
  │   ├── [[Alpha - Design]]
  │   └── [[Alpha - Timeline]]
  └── [[Project Beta]]
      ├── [[Beta - Proposal]]
      └── [[Beta - Budget]]
```

### 5. Cross-Referencing

Link related concepts:
- "See also: [[Related Topic 1]], [[Related Topic 2]]"
- "Prerequisites: [[Foundational Concept]]"
- "Next steps: [[Follow-up Topic]]"

---

## Graph Visualization Examples

When you open the **Mention Graph**, you'll see:

- 🔵 **Blue nodes** = People (`@[Person]`)
- 🟡 **Yellow nodes** = Concepts (`@[Concept]`)
- 🟢 **Green nodes** = Wikilinks (`[[Note]]`)

**Node size** indicates the number of connections:
- Larger nodes = More references
- Smaller nodes = Fewer references

**Connections** (edges) show relationships:
- Thicker lines = Stronger connections
- Thinner lines = Weaker connections

### Example Graph Structure

```
                    [[Q1 Deliverables]]
                           |
        +------------------+------------------+
        |                  |                  |
    @[Sarah Chen]    [[API Architecture]]  @[Michael Rodriguez]
        |                  |                  |
   [[Timeline]]       [[REST API]]      [[Database Migration]]
                           |
                      @[David Kim]
```

---

## Advanced Patterns

### Bi-directional Linking

Create two-way connections:

In `[[Project Alpha]]`:
```markdown
Related to [[Project Beta]]
```

In `[[Project Beta]]`:
```markdown
Builds upon [[Project Alpha]]
```

### Hub Notes

Create hub notes that aggregate related information:

`[[Machine Learning Hub]]`:
```markdown
# Machine Learning Hub

## Core Concepts
- [[Neural Networks]]
- [[Deep Learning]]
- [[Transfer Learning]]

## Researchers
- @[Dr. Liu]
- @[Dr. Hassan]

## Projects
- [[NLP Pipeline]]
- [[Vision Research]]
```

### Daily Notes

Link from daily notes to entities:

`[[2024-01-15]]`:
```markdown
- Met @[Alice] about [[API Design]]
- Reviewed [[Code Review Guidelines]]
- Planned [[Sprint Retrospective]]
```

---

This example demonstrates how @-mentions and [[wikilinks]] create a rich, interconnected knowledge base that's easy to navigate and explore!
