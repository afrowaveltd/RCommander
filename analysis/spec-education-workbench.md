# Education Workbench — proposed reference plugin

Status: owner-requested design, 2026-11-27; not implemented.
This is a reusable teaching workflow, not an independent LMS or a replacement
for the teacher. The teacher is the agent's collaborating client throughout
the lesson when a human teacher is present. The same workflow also supports
self-study with an AI tutor: the learner directs the course and can revise
or pause it at any time. The course is not tied to a subject or model.

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

## Course planner and adaptive entry assessment (owner extension)

A course is a reusable curriculum, composed of lessons following a versioned
lesson template. Distinguish the course planner, diagnostic assessor, lesson
author, and teaching agent. They are logical roles, not mandatory separate
models. The planner can create or revise the editable course/lesson template
for the human teacher. It consumes a goal such as "I want to learn advanced
C programming for the Linux kernel", not an already enumerated syllabus.

### Entry assessment

1. Record the learner's goal, language, desired depth, available tools and
   optional pacing/time preferences; ask only for missing relevant details.
2. Build an adaptive diagnostic: start with broad prerequisite questions,
   then use progressively targeted questions or practical tasks to identify
   demonstrated competence, gaps and uncertainty. The learner/teacher can
   pause, resume, shorten, extend or skip it. No fixed time or question count.
3. Produce a structured, editable knowledge profile with topic/skill IDs,
   evidence from answers or exercises, confidence, gaps and suggested
   starting level. Distinguish untested skills from failed skills and
   self-reported from demonstrated competence. Do not infer unrelated traits
   or claim validated certification.
4. Feed this profile into the course planner prompt and subsequent teaching
   prompts as private context. The learner/teacher can correct the profile.
   Update it from later lesson assessments, keeping the assessment history
   separate from the reusable course. Keep personal answers and profile local
   by default; remote model sharing follows existing egress settings.

### Curriculum generation

The planner receives the requested goal, knowledge profile, teacher/learner
preferences, approved resources, available tool capabilities and the
universal lesson template. Example editable prompt:

> Create a course on {topic} using {lesson_template}. Adapt the depth,
> prerequisites, sequence and exercises to {knowledge_profile} and
> {learner_preferences}. Use only available tools and approved materials.
> Identify knowledge gaps, learning objectives, dependencies, assessment
> criteria and alternatives when a demonstration is unavailable. Return a
> structured curriculum for review before generating all chapters.

Produce ordered modules and lessons with stable IDs, prerequisite links,
objectives, suggested duration (not mandatory), theory, illustration plan,
practical tasks when relevant, assessments, rubrics and resource references.
Allow branching, optional refresher lessons, topic skipping and later
revision; preserve completed work when a curriculum is revised. A teacher
or learner can approve/edit the plan and request any chapter on demand.
The lesson teaching agent follows the existing lesson workflow and adapts
explanation to the learner's current knowledge without rewriting the
canonical shared course merely to personalize a session.

The system is subject-agnostic: programming, philosophy, history,
mathematics, languages, arts, and cultural or belief systems such as
astrology. For empirical subjects distinguish supported evidence, uncertainty,
interpretation and unsupported claims. Teach astrology as a historical,
cultural or belief practice rather than presenting predictions as scientifically
established. Subject-specific practical exercises are optional: philosophical
argument analysis is a valid practice activity. For Linux kernel examples,
prefer an authorized disposable VM or isolated build environment; real host
kernel changes are not a default teaching operation.

## Learning workspace and location-independent storage (owner extension)

On first use, offer to create a Learning directory in the current user's
home directory (resolve it via the platform's home-directory API, not a
hardcoded Linux path). The default is local and usable offline. The user can
change the root or choose an existing folder in a Commander panel, including
a supported remote/cloud location. Create missing directories through the
same file-operation service and scoped policy as ordinary Commander actions;
never overwrite an existing course or silently relocate user data.

Illustrative workspace layout (names and format are provisional):

```text
Learning/
  Courses/
    advanced-c-linux-kernel/
      manifest.json
      curriculum.json
      lessons/
      assets/
      examples/
      assessments/
  Learners/
    <local-learner-id>/
      profile.json
      courses/
        <course-id>/
          progress.json
          submissions/
          feedback/
          assessment-results/
  Work/
    <session-id>/
      drafts/
      agent-results/
  Teacher/
    <course-id>/
      answer-keys/
      review-notes/
  Cache/
    materials/
```

The teacher/learner chooses where course files, personal results, agent
outputs and private teacher materials are stored. These are distinct logical
roots, not mandatory physical subdirectories on the same disk. The agent
creates chapters, saves explanations and demonstrations, and saves student
work, assessment results and feedback as regular inspectable files through
the shared workspace/file APIs. Reuse the existing durable session store
for resumable work and checkpoint receipts; avoid a competing task ledger.
A course package is self-contained or contains portable resource references;
learner records and credentials are never included in its default export.

Use provider-neutral resource references (provider identity, canonical
location and relative resource ID/path) and stable course/learner IDs, not
absolute OS-specific paths in course manifests. Learning must not depend on
whether a directory resides on a local disk, a mounted/synced folder, SFTP,
a cloud plugin, or another provider with the necessary capabilities.
Resolve references through the existing Location/source abstraction and
capability catalogue. Detect read-only or offline providers, unavailable
resources and unsupported operations explicitly; allow a local working copy
and reviewed transfer where needed. Never report a remote write as saved
until its outcome is verified. Keep version/conflict evidence, use atomic
replacement only where supported, and preserve both edits rather than
silently overwriting a newer version.

### Afrowave Cloud provider (future, shared Commander plugin)

Plan an optional generic storage plugin with an Afrowave Cloud profile
pointing to an administrator/user-configured Nextcloud instance (WebDAV as
a candidate transport) or another compatible storage service. It is a
normal Commander panel/source provider, not an education-only API.
Education, document preview/editor, backup and other workloads can all use
it. Reuse source registration, shared HTTP/TLS, credential references,
policy, transfer, cache and synchronization services. Do not embed a server
URL or require an Afrowave account for local learning. Validate actual
provider capabilities, authentication and licenses before choosing the
first adapter. Cloud use, personal-record sync, sharing and publication
are independent user choices; default to private storage and no automatic
upload of personal results. Keep teaching materials/catalogue retrieval
separate from this general-purpose storage provider.

Acceptance: first-run Learning creation on Linux/Windows with existing
directories preserved; reopen and resume from local files; equivalent
course/lesson operations on a mock remote provider and a real supported
provider before claiming compatibility; copy/move course to a different
root without broken relative links; read-only/offline failure and recovery;
concurrent edits/conflicts, verified transfer, permission denial, and
sharing without private learner records. A first local implementation does
not depend on cloud support.

## Course authoring, packaging and sharing

Support three authoring modes using the same course schema and editor:

- Manual: human authors the plan, lessons, assets and assessments.
- Assisted: human directs an agent to create or revise chapters, illustrations
  and tests, reviewing individual changes and keeping manual edits.
- Automatic: planner and author roles produce a complete course or selected
  chapters, with explicit review/publication as separate actions.

Proposed portable, provider-neutral package: an open directory of a versioned
manifest, course metadata, lesson template/version, curriculum, Markdown
lesson files, assessments, teacher-only keys (optional separate package),
assets/examples, and license/provenance records. An optional archive is
transport only; reuse the existing archive engine. Proposed layout:

```text
course/
  manifest.json
  curriculum.json
  templates/lesson.md
  lessons/001-introduction/lesson.md
  lessons/001-introduction/exercises.md
  lessons/001-introduction/assets/
  assessments/
  LICENSE
```

The manifest declares stable course/lesson IDs, schema version, language,
author/attribution, license, objectives, prerequisites, dependencies and file
hashes. Courses must remain readable without an AI provider. Preserve the
source and generated output where appropriate, not just chat transcripts.
Import/export and validate packages with the same format regardless of
authoring mode. Do not silently execute imported course code, prompts or
embedded tools. Use existing review, archive and broker mechanisms.

A learner who finds a course useful can export/share the complete reusable
course (including any assets whose licenses permit redistribution), subject
to explicit user publication and attribution. Provide optional teacher-key
export, excluding private progress, diagnostic answers, grades, conversations,
credentials and model settings by default. Personalized progress is a
separate private workspace/session record (local by default); make any optional personal export explicit.
Version or fork shared courses without overwriting originals; support manual
or agent-assisted translation in a later slice. External copyrighted material
may need a reference instead of bundling. No public service is required for
local export/import; a community catalogue can be a later provider.

## Illustration and demonstration capability discovery

Choose tools from the existing capability catalogue rather than requiring a
single illustration provider. Prefer an available, permitted suitable tool:
web research and licensed/source-attributed images; material server assets;
local Markdown tables, Unicode icons, ASCII diagrams, SVG or generated HTML;
ImageMagick for composition/conversion where installed; PhotoCraft or another
compatible MCP image/diagram application. Distinguish retrieving existing
images from generating or transforming them. Without network or an image
provider, teach with text, diagrams and local assets; do not claim unavailable
illustrations were created. Keep illustrations editable/source-linked when
possible, provide text alternatives, record provenance and use the same
permission checks for tool execution and file access. Use F3, WebView,
compiler, terminal, database and other approved panel capabilities for
demonstrations; never require them when the topic or device does not need them.

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

Use small versioned, editable prompt templates: diagnostic assessor, course
planner, course/lesson template designer, chapter author, lesson planner, theory
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

1. Declarative prompt pack, editable universal lesson template, adaptive entry
   assessment, knowledge profile and course planner; teacher-reviewed Markdown
   chapters and separate assessment keys. Start with a simple portable course
   directory and manual/assisted/automatic authoring using the same schema.
2. Versioned material catalogue adapter and bounded download/cache tool,
   using existing transport and policy; fake server and offline tests.
3. Teaching workflow state, teacher checkpoints, artifact links and
   resume/export; reuse panel context and existing F3/F4.
4. Optional interactive demonstrations with browser/DOM, local compilers,
   data or other approved tools as they become available.
5. Validated portable package import/export, versioning, offline reading and
   explicit course sharing, with separate personal progress and attribution.
   Test capability-based illustration selection and offline fallback.

Tests: adaptive diagnostic branching, pause/resume/skip, unknown vs failed
skills, profile propagation and privacy, curriculum prerequisite validation,
manual/assisted/automatic authoring round-trips, import/export excluding
private state, missing-tool illustration fallbacks, and subject-agnostic lessons.
Also test template variables and stage transitions, teacher interruption and
revision, no answer-key disclosure to the student view, no fabricated
practical stage, catalogue failure/offline fallback, malformed manifests,
path escape and checksum mismatch, permission denial, and one complete
teacher-led reference lesson. Stage 1 must remain useful before all optional
renderers exist. Do not claim automatic grading accuracy or a functioning
server until tested.
