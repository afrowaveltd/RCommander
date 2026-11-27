# Education Workbench — proposed reference plugin

Status: owner-requested design, 2026-11-27; not implemented.
This is a reusable teaching workflow, not an independent LMS or a replacement
for the teacher. The teacher is the agent's collaborating client throughout
the lesson; the student participates under the teacher's guidance.

## Goals

- One universal lesson workflow, applicable to programming, mathematics,
  languages and other subjects. Add a topic and material references rather
  than creating a new executable plugin for each subject.
- Prompt-first MVP: use the existing prompt catalogue, workload packs,
  agent runtime, shared panels, Markdown, F3/F4, and existing tool broker.
  Add a narrow material retrieval capability and structured lesson state.
- Keep materials and generated outputs accessible on modest hardware.
  Local models or user-selected remote providers are equally valid.
- Teacher chooses language, audience/level, scope, objectives, time budget
  and preferred illustration/practical/test formats. The agent proposes
  suitable defaults and revises them during the conversation.

## Universal lesson workflow

| Stage | Agent contribution | Teacher interaction / checkpoint |
| --- | --- | --- |
| Preparation | Propose objectives, prerequisites, sequence, materials and expected outcomes from the topic | Confirm or edit plan; choose difficulty and language |
| Theory and illustration | Explain concepts, make diagrams or use reviewed illustrations, give examples | Ask for revisions, alternative examples or simpler explanations at any point |
| Practical demonstration | Prepare a runnable or inspectable example when appropriate; explain every step | Observe the same panel; pause, change variables, request another demonstration |
| Student practice | Generate a task and hints; provide permitted files and assets | Teacher can choose independent work or guided practice |
| Student test | Create questions/tasks and a separate answer key and rubric; evaluate responses against objectives | Teacher reviews test and feedback, can override evaluation; no automatic final grade |
| Review | Identify misconceptions, suggest revision or next topic, save/export lesson record | Teacher decides whether to repeat, advance or revise |

Stages are editable, repeatable and skippable when inappropriate to a subject.
Do not fabricate practical experiments for a topic that has none. Checkpoint
state belongs to the existing session/workflow store; preserve teacher
decisions, topic, stage, artifacts and completion status. Answers and the
teacher's key are separate from student-facing files and previews.

## Teaching agent prompt pack

Use small versioned, editable prompt templates: lesson planner, theory
explainer, illustration planner, demonstration builder, exercise author,
assessment author/reviewer, and review/adaptation. A coordinator selects the
stage; a single model can run all roles for the MVP. Prompts consume topic,
goals, learner level, language, approved materials, previous outputs and
teacher feedback. Treat source material as data, not privileged instructions.
Provide clear schema for generated lesson sections, examples, questions and
rubrics. Reuse shared renderers and tools; don't embed a second browser,
editor or model runtime.

## Retrieve source files — special teaching capability

Proposed tool identifier: `education.get_source_files`. Settings hold a
teacher-configurable material-server base URL or IP address (including
optional port and path), optional credential reference and destination
workspace. An Afrowave server can publish the first catalogue, without
hardcoding that provider as the only possible source.

Proposed catalogue contract (versioned, read-only for learners):

- Browse categories, topics, languages, levels, and material collections;
  return descriptions, versions, license/attribution and asset manifest.
- Select a collection and retrieve its listed files: images, starter code,
  diagrams, audio, data, templates and other teaching resources.
- Use the shared HTTP/artifact transport and policy broker; verify
  manifest sizes and hashes, sanitize destination paths, enforce download
  quotas and destination scope, and prevent traversal/unsafe archive
  extraction. Prefer HTTPS; any user-configured HTTP development endpoint
  follows the explicit network policy. Never expose credentials to models.
- Cache versioned materials locally for reuse/offline teaching. A cached
  collection must remain usable without a live server. Respect licensing
  and record provenance. Never execute downloaded code automatically.
- Keep source assets separate from generated lesson files, examples,
  student submissions and teacher-only assessment keys.

Illustrative layout (not an existing API):
```text
education/
  web-development/
    html-basics/
      manifest.json
      assets/
        logo.svg
        photos/
      starter/
        index.html
        style.css
```

## Reference lesson: build a first web page

Teacher opens the lesson workspace in the right panel and the teaching agent
in the left. Agent proposes a lesson covering HTML structure and CSS, fetches
approved starter assets from the catalogue, explains a diagram in Markdown,
and creates an example page in the workspace. F3 renders it through the
WebView when available; F4 edits its HTML/CSS. In development mode, the agent
can inspect the DOM and apply permitted temporary CSS changes to demonstrate
the visual result. The teacher requests changes and can ask the student to
predict what will happen before revealing the result. Student builds a small
variation, then takes a quiz/practical test. Teacher receives the rubric and
feedback, decides what to revisit. Without WebView, use source and existing
renderers and report the limitation; do not require WebView for the MVP.

## Delivery and acceptance

1. Declarative prompt pack and editable universal lesson template, with a
   teacher-reviewed Markdown lesson and separate assessment key.
2. Versioned material catalogue adapter and bounded download/cache tool,
   using existing transport and policy; fake server and offline tests.
3. Teaching workflow state, teacher checkpoints, artifact links and
   resume/export; reuse panel context and existing F3/F4.
4. Optional interactive demonstrations with browser/DOM, local compilers,
   data or other approved tools as they become available.

Tests: template variables and stage transitions, teacher interruption and
revision, no answer-key disclosure to the student view, no fabricated
practical stage, catalogue failure/offline fallback, malformed manifests,
path escape and checksum mismatch, permission denial, and one complete
teacher-led reference lesson. Stage 1 must remain useful before all optional
renderers exist. Do not claim automatic grading accuracy or a functioning
server until tested.
