# SubForge — Product and Implementation Specification

**Specification date:** 11 October 2026  
**Status:** Implementation updated to SubForge 0.1.6. See `BUILD_REPORT.md` for the installer, verified behavior and remaining release-validation limits.  
**Target:** Windows 10 and Windows 11, 64-bit (x64)  
**Project directory:** Repository root  
**Credit:** Sidos - info@sidos.gr

## 1. Purpose and handoff

SubForge is a simple Windows desktop application that creates subtitles for selected video files. Its primary entry point is the Windows File Explorer right-click menu. It first reuses or translates suitable existing subtitles; otherwise, it extracts audio, transcribes the speech, and translates the resulting timed text into the requested language.

This document consolidates the user's requirements and subsequent decisions. It can be used by GPT Sol or another implementation agent without reconstructing the conversation.

**Requirement terminology:**

- **Required:** Explicitly requested by the user or necessary to deliver an agreed feature.
- **Proposed:** An engineering choice or default recommended during planning. It may be adjusted for a concrete technical reason while preserving required behavior.
- **Deferred:** Outside the initial release unless the user expands the scope.

The original authorized task was to create this specification. The user subsequently authorized implementation explicitly. On 19 October 2026, the user additionally authorized publishing the project in a public GitHub repository named SubForge. Purchasing services and sending messages remain outside that scope.

### Project state when the specification was written

- The project folder was inspected before writing this specification.
- It contained `test.mkv`; no application scaffold or existing project instructions were found in the checked project/ancestor `AGENTS.md` locations.
- Preserve `test.mkv`. It is an existing user file, not a generated fixture.
- No live provider credentials, speech quality benchmarks, or Windows installer tests have been completed.
- Earlier provider research was documentation-based, not a successful integration test.
- Local sandbox execution failed during planning; a permitted read-only retry outside that sandbox succeeded. Recheck tool availability during implementation rather than assuming the application or repository is at fault.

## 2. Confirmed decisions

| Area | Required decision |
| --- | --- |
| Product name | SubForge |
| Platform | Windows 10 and Windows 11, x64 |
| Implementation | Use Rust where possible; Rust is the intended processing core |
| Default subtitle language | Greek |
| Default interface language | English |
| Interface scope | Simple progress interface, Settings, and Help/About |
| Processing order | Look for existing subtitles before extracting/transcribing audio |
| Batch processing | Multiple selected files and folders |
| Default parallel file limit | 1; user can configure a higher limit |
| AI usage | Free by default; paid usage only when explicitly enabled |
| Local transcription | Optional multilingual Whisper Base or Whisper Small, chosen by the user |
| Local translation models | Excluded |
| Google Translate | Excluded |
| Azure Speech | Excluded |
| Appearance | Dark mode and a theme setting |
| Localization | All ten languages listed below |
| Documentation | Detailed offline HTML instructions in all ten languages, accessible within the app |
| Branding | Original icon used for executables, shortcuts, and the right-click entry |
| Final distribution | One installable setup EXE |
| Credits | Exactly “Sidos - info@sidos.gr” |

### Resolution of the local-AI requirement

The original request prohibited local AI models. The user later explicitly approved **Whisper Base and Small as local transcription options**.

That is the agreed exception. It does not authorize adding local translation/chat models or substituting Medium, Large, or Turbo as local model choices in the initial release. Cloud-hosted Whisper models remain permitted online transcription services.

## 3. Languages

Subtitle target language, interface language, and help language are separate settings.

| Language | Proposed locale identifier | Example subtitle suffix |
| --- | --- | --- |
| English | en | .en.srt |
| Greek | el | .el.srt |
| Spanish | es | .es.srt |
| French | fr | .fr.srt |
| German | de | .de.srt |
| Portuguese | pt | .pt.srt |
| Italian | it | .it.srt |
| Chinese, Mandarin / Simplified written output | zh-Hans | .zh-Hans.srt |
| Korean | ko | .ko.srt |
| Japanese | ja | .ja.srt |

- English is the initial interface language; Greek is the initial subtitle target.
- The initial help language should follow the interface, with a language selector.
- Auto-detect source speech/subtitle language where reliable, and offer an override.
- Use multilingual Whisper models, not the English-only `.en` models.
- Portuguese regional variant was not specified. Use a documented, consistent convention; regional variants are not a release requirement.
- English fallback is allowed for genuinely missing resource keys during development, but release resources must cover every user-facing screen and error category.
- Verify Greek and CJK text rendering, line wrapping, filenames, and help navigation.

## 4. Main user workflows

### 4.1 First launch

1. Choose or retain the interface language, theme, and subtitle target.
2. Configure at least one usable translation route when translation is needed.
3. Choose local or online transcription for videos without suitable subtitles.
4. Test configured providers or install a local model.
5. Show a concise readiness summary with actionable setup errors.

Existing-subtitle translation must remain usable without configuring a speech provider. Same-language local transcription must not require a translation provider.

### 4.2 File Explorer

- Register “Create subtitles with SubForge” for supported video files.
- Support one file, multiple selected files, and folders.
- Display the SubForge icon.
- Pass complete Unicode paths safely without shell interpolation.
- Start one application instance and queue later selections in it.
- Use the saved subtitle target for the quick action.
- Do not change the user's default video player.
- Provide a setting to enable or disable Explorer integration.
- Remove owned registrations on uninstall.

**Proposed:** Support the Windows 11 modern menu using an Explorer extension and package identity, alongside classic menu support. Prototype packaging and signing requirements early. The user's essential requirement is a working right-click entry; any limitation to “Show more options” must be explicitly documented rather than represented as modern-menu support.

### 4.3 Inside the app

- Allow adding files/folders and drag-and-drop as convenient secondary entry points.
- Display the target language before processing and allow changes for queued work.
- Show a compact queue and the active processing stage.
- Provide pause, resume, cancel, retry failed jobs, and open output folder controls.
- Continue processing other files when one file fails.
- Folder recursion is configurable; proposed default is off.

## 5. Processing pipeline

### 5.1 Inspect the input

Use FFprobe or an equivalent bundled media probe to identify duration, audio streams, subtitle streams, and readable codecs.

“Any video” means any readable media container/codec supported by the bundled tools. Do not promise support for corrupt files, DRM-protected content, missing audio, or unsupported codecs. Report the specific limitation when possible.

Audio track selection should use a saved language preference/default track and expose an override for multiple tracks. Do not silently transcribe a commentary track when a suitable main track is available.

### 5.2 Discover subtitles first

Search in this order, subject to match quality:

1. Matching sidecar subtitle files beside the video.
2. Matching subtitles in associated subtitle folders.
3. Embedded text subtitle tracks.

Proposed initial text formats: SRT, WebVTT, ASS, and SSA.

- Match by filename/stem, language tags, track metadata, and timing plausibility.
- Do not pick unrelated subtitles merely because they share a directory.
- Prefer full dialogue over forced-only tracks.
- Do not assume every track in the target language is complete.
- If a suitable target-language subtitle already exists, skip generation by default.
- If suitable source-language subtitles exist, translate their text while preserving timing.
- If multiple candidates are ambiguous, provide an override or a clear selection explanation.
- Treat malformed or obviously incomplete subtitles as unsuitable rather than reporting success.

Image subtitles such as PGS/VobSub, and burned-in text, are not text-extraction sources in the initial release. Use the audio route when necessary. OCR is deferred.

### 5.3 Extract and transcribe audio

- Bundle FFmpeg/FFprobe or equivalent redistributable tools with required notices.
- Extract only the selected audio stream.
- Choose audio encoding, sample rate, and chunk sizes for the selected speech backend.
- Keep uploads below provider limits.
- Use overlapping chunks when needed, restore absolute timestamps, and remove boundary duplicates.
- Detect empty/silent input and report insufficient speech instead of fabricating cues.
- Preserve the source-language transcript as an intermediate result.
- If source and target languages match, skip translation.

### 5.4 Translate timed segments

- Translate dialogue through the configured online AI routes.
- Give every source segment a stable identifier.
- Supply bounded surrounding context to maintain names, meaning, and consistent terminology.
- Ask for structured output containing identifiers and translated text.
- Keep timestamps under application control; the translation model must not invent or freely rewrite them.
- Validate missing, duplicate, unexpected, and empty segment results.
- Retry or fall back for invalid output within bounded limits.
- Treat subtitle text as content, not instructions to execute tools or change configuration.
- Where ASS/SSA input is normalized to SRT, preserve meaningful text and basic supported formatting; advanced styling preservation is not guaranteed.

### 5.5 Validate and save

Default output: UTF-8 SRT beside the video.

Examples:

- `Movie.el.srt`
- `Movie.en.srt`
- `Movie.zh-Hans.srt`

Settings should include output directory, filename pattern, optional WebVTT output, source transcript retention, and existing-file behavior.

- Validate start/end times, ordering, duration bounds, cue identity, and nonempty text.
- Apply language-appropriate line wrapping and readable subtitle segmentation.
- Avoid accidental overlap; retain intentional overlapping dialogue only when supported.
- Write output atomically after successful validation.
- Do not overwrite videos or existing subtitles by default.
- Use a noncolliding output name or require an explicitly configured replacement policy.
- Handle unwritable folders with a clear recoverable error or user-selected output location.

## 6. Local transcription

### Required options

- Whisper Base, multilingual.
- Whisper Small, multilingual.
- User chooses the model in Settings.
- Download, inspect installed size/status, and remove downloaded models.
- Work offline after the selected model has been installed.

**Proposed engine:** whisper.cpp, called from Rust through a suitable binding or a controlled child process. The engine itself is C/C++; the application and orchestration remain Rust. Package a ready-to-run build rather than requiring users to install Python or compilers.

### Model handling

- Download only after the user selects installation.
- Show download progress and failures.
- Pin trusted model artifacts and validate checksums.
- Support recovery from interrupted downloads.
- Store models outside temporary directories.
- Prevent removal while a model is in use.
- Keep large model downloads optional to avoid inflating the base installer.

### Execution

- CPU execution is required.
- Supported GPU acceleration may be added as an automatic or explicit option, with CPU fallback and truthful capability detection.
- Start with one local transcription worker to avoid memory exhaustion.
- The global parallel-file limit must not bypass backend resource limits.
- Keep local transcription available without provider keys.
- Do not upload audio automatically after a local failure unless the user enabled cloud fallback.
- Keep local translation AI out of scope.

Hardware recommendations are guidance to validate during implementation, not benchmarked guarantees: a modern CPU with 8 GB RAM is a reasonable Base/Small starting point; 16 GB RAM provides more headroom. Do not promise processing speed without measurement.

## 7. Online speech providers

Required configurable adapters:

| Provider | Intended role | Access classification |
| --- | --- | --- |
| Groq | Cloud Whisper transcription with timestamps | Free tier where available; paid only by opt-in |
| Cloudflare Workers AI | Cloud transcription with timed output | Free allocation where available; paid only by opt-in |
| Google Gemini API | Audio transcription with structured timed output | Eligible free models/quotas; paid only by opt-in |
| Deepgram | Speech-to-text API | Introductory credits and optional paid access |
| Speechmatics | Speech-to-text API | Introductory credits and optional paid access |
| Custom speech endpoint | User-specified compatible transcription service | Declared and tested by the user |

Microsoft Azure Speech is deliberately excluded. AssemblyAI was researched but was not included in the final agreed provider list.

- Authenticate with the provider's supported account/key mechanism.
- Expose the selected model and supported language/capability information.
- Verify timestamp quality and language coverage through representative samples.
- Never assume a text-only model accepts audio.
- Free allocations, introductory credits, recurring limits, and paid plans must be labeled distinctly.
- Discover or configure limits rather than embedding promotional quotas as permanent guarantees.
- Long-file support must include chunking or a supported asynchronous upload/job flow.

## 8. Translation providers and agent tools

Required connections:

| Connection | Configuration |
| --- | --- |
| OpenCode CLI / OpenCode Zen | CLI detection/setup, API key where required, explicit provider/model selection |
| OpenRouter | API key, model selection, compatible direct API or supported agent route |
| Google Antigravity CLI | Supported Google sign-in/OAuth or Gemini API-key mode |
| Pi agent | CLI detection/setup, supported authentication, provider/model selection |
| Custom provider | URL, protocol, API key, model IDs, and declared capabilities |

### Separate tools from services

OpenCode, Antigravity, and Pi are agent runtimes. Zen, OpenRouter, and cloud model endpoints are services. Model availability, billing, and audio support belong to the actual selected route.

The settings model should represent both the runtime/transport and the underlying provider/model. Do not assume installing a free CLI makes every model free or provides speech recognition.

### Integration behavior

- Use documented noninteractive, JSON/event-stream, RPC, or API modes where available.
- Antigravity, OpenCode/Zen and Pi translation must invoke their CLI executables. Direct API translation must reject every agent kind before credentials or network access. Validate separate completed answers; bound batches by cue count and text length, and split malformed or incomplete replies while preserving IDs and timestamps. Use native Antigravity JSON-schema output.
- Show cached native Google sessions explicitly. Offer an explicit account-switch action through the native logout command, with notice that it signs out the CLI account on this PC. Preserve native setup preferences, pace menu navigation, and open validated CLI OAuth links through the Windows default-browser association, including links embedded in terminal hyperlinks.
- Test supported versions and parse structured progress/results.
- Avoid showing terminal windows during normal processing.
- Use browser/interactive authentication only when required for setup.
- Hidden Antigravity sign-in must answer terminal initialization queries, advance appearance setup, show Google's terms and privacy controls, disable optional interaction sharing, and wait for explicit user confirmation before finishing setup. Poll sign-in progress; offer browser retry only for a validated Google OAuth URL. Closing the helper must not claim that provider access was verified.
- OpenCode Zen must use the managed OpenCode CLI for translation, connection checks and model refresh; never call Zen directly or fall back to its HTTP API. Retain existing Zen settings and credential IDs. Accept bare Zen model IDs or `opencode/<model-id>` and preserve user order.
- Keep the official OpenCode tool schemas available with ask permissions and launch translation with `--no-auto`, so CLI permission requests are automatically rejected while free-tier routing remains compatible.
- SubForge must automatically provision verified official agent runtimes and run automation in the background using saved settings. Users must not enter executable paths or edit CLI settings files.
- Keep app-managed runtimes and profiles separate from existing installations; do not silently change global agent settings.
- Prefer app-scoped configurations and restricted temporary task directories.
- Keep media handling and final file writes under Rust application control.
- Disable unnecessary agent tools where supported.
- Do not use undocumented credential scraping or authentication bypasses.
- Custom URL support must name the supported protocol; an arbitrary URL alone does not guarantee compatibility.
- Show Cloudflare account ID only for Cloudflare speech, and authentication controls only when relevant to the selected connection type.
- Saved API key fields must appear filled with a masked value. An unchanged masked or blank field preserves the key; an explicit delete action removes it.
- Normalize bare Gemini-compatible server URLs to the API version and filter model discovery by supported generation capability.
- Initial protocol targets can include chat-completions-compatible translation, Gemini-compatible translation/audio, and multipart transcription-compatible APIs.

## 9. Ordering, retries, cooldowns, and cost control

Maintain separate ordered configurations for:

1. Transcription.
2. Translation.

Each enabled route includes its selected model or ordered model list, authentication method, capabilities, timeouts, retry count, request delay, and cooldown policy.

- Respect the user's order.
- Skip routes that cannot perform the current stage, with an understandable reason.
- Try the next configured model/provider only when appropriate.
- Do not silently use a provider outside the configured list.
- Display the active route and fallback reason.
- During a provider cooldown, either wait or use an eligible fallback according to settings.
- Share provider rate-limit state across all files.
- Respect server-requested waits and apply bounded backoff for transient failures.
- Do not repeatedly retry invalid credentials or unsupported models as if they were temporary errors.
- Bound retries for malformed model output.
- Preserve completed work if all eligible routes fail.

### Proposed defaults

Only the subtitle language, interface language, free-by-default behavior, and parallel limit of one are explicitly confirmed defaults. Other values below are proposed starting points.

| Setting | Initial value |
| --- | --- |
| Subtitle target | Greek — confirmed |
| Interface language | English — confirmed |
| Parallel files | 1 — confirmed |
| Paid usage | Disabled — confirmed |
| Theme | Dark; also offer Light and Follow Windows |
| Transcription mode | Chosen during setup; online and optional local modes available |
| Cloud fallback from local | Disabled until enabled |
| Retries after initial attempt | 2 |
| Minimum request delay | 2 seconds |
| Retry backoff | Starts at 5 seconds, increases with a cap; server wait takes precedence |
| Folder recursion | Off |
| Suitable target subtitle exists | Skip |
| Overwrite existing files | Off |
| Main output | UTF-8 SRT beside video |

### Billing behavior

- Paid models/usage require an explicit setting.
- Do not silently move from an exhausted free allowance to a paid route.
- Introductory credit exhaustion must not silently enable paid billing.
- Show unknown pricing/remaining-credit state honestly.
- Do not claim SubForge can guarantee external account billing limits it cannot inspect or enforce.
- Where a CLI automatically spends paid credits, disable that behavior if supported or exclude the route from strict free operation until its configuration is verified.
- Keep credentials and billing setup user-owned; SubForge supplies no shared service keys.

## 10. Queue, progress, and recovery

- Persist the queue and completed chunks/stages.
- Resume interrupted jobs without retranscribing/retranslating successful chunks unnecessarily.
- Identify cached work using input identity, relevant metadata, chosen audio/subtitle stream, target language, and processing configuration.
- Invalidate stale cached results when relevant inputs change.
- Deduplicate repeated file selections.
- Pause at a safe boundary; describe whether the active request is finishing.
- Cancellation must stop owned work and child processes without corrupting completed output.
- One failed file must not stop unrelated jobs.
- Parallel file limits are separate from per-provider and local-engine concurrency limits.

Suggested statuses:

Queued → Inspecting → Finding subtitles → Extracting audio → Transcribing → Translating → Validating → Saving → Completed.

Additional statuses: Skipped, Paused, Cooling down, Needs setup, Failed, Canceled.

Use actual completed units and stage progress. When a provider exposes no reliable percentage, show an indeterminate state rather than invented progress or completion times.

## 11. Interface and settings

### Progress view

Show file name, target language, current stage, active route/model, meaningful progress, queue position, and short actionable errors. Keep detailed logs behind an expandable panel.

Queue cards must show audio part/time range or translation batch/cue count,
request attempts, cooldown and elapsed time. Show a small microphone graphic
during transcription and a Rosetta Stone graphic during translation, respecting
reduced-motion preferences. Localize all new status and log controls.

Maintain an append-only detailed log per file across retries and application
restarts, with in-app viewing and export. Record stages, route/model, attempts,
cache reuse, repairs and errors. Include local filenames/paths but omit audio,
dialogue, credentials, authorization URLs and raw terminal output. Clearing
finished queue entries must leave these logs available in the data folder.

Retry must clear the old error and queue processing automatically with current
saved settings. Validate source, chunk and translation checkpoints before reuse;
rebuild invalid checkpoints while preserving valid work. Repair zero-duration
Whisper recognition cues within the available next-cue/audio interval without
losing text. Keep external subtitle parsing and other timing checks strict.

Clip Whisper recognition cue endings to the actual audio-part duration when the
cue starts within that part, preserving IDs, text and start times. Do not move
entirely outside cues into the audio. Record original/repaired endpoints and the
audio limit in the per-file log. Final errors must retain the last processing
stage, part/batch context and elapsed time.

Avoid turning the app into a general AI chat interface, video editor, or terminal frontend.

### Settings

- General: interface language, default subtitle language, output location, theme.
- Transcription: local/online mode, Base/Small selection, downloads, cloud routes, audio language/track preferences.
- Translation: agent/API routes, keys/sign-in, model selection, order, connection tests.
- Processing: parallel files, retries, delays, timeouts, folder recursion, resume/cache behavior.
- Subtitles: discovery preferences, output format/naming, replacement policy, source transcript retention.
- Integration: right-click registration and shortcut preferences.
- Diagnostics: sanitized logs, dependency versions, connection/model tests, settings import/export.
- Help/About: local HTML documentation, version, credits, and third-party notices.

Keyboard navigation, visible focus, useful labels, readable contrast, and scaling for high-DPI displays are required quality goals.

## 12. Credentials and data handling

- Store SubForge-managed secrets in Windows-protected credential storage.
- Keep ordinary settings separate from secrets.
- Never write keys, access tokens, refresh tokens, or authorization headers to normal logs.
- Export settings without secrets by default.
- Use supported CLI credential stores for CLI-managed sign-in.
- Explain whether audio or subtitle text is sent to the chosen online service.
- Local transcription sends no audio to cloud providers unless cloud fallback is enabled.
- Online translation can still transmit the locally generated subtitle text.
- Remove owned temporary audio and uploads when feasible; provide a documented retention/cache policy.
- Do not alter original media.
- Do not upload `test.mkv` merely to produce a demo without an authorized live test and configured provider.

## 13. Architecture recommendation

**Proposed stack:**

- Rust: processing engine, adapters, orchestration, settings, persistence, and Windows integration.
- Tauri 2: desktop host and restricted interface-to-backend commands.
- Small HTML/CSS/TypeScript frontend: progress, settings, and help.
- FFmpeg/FFprobe: media operations.
- whisper.cpp: optional local speech engine.
- SQLite or another robust local store: resumable queue/checkpoints.
- Windows credential APIs: secrets.
- Tauri/NSIS: single setup EXE.

Organize modules around media inspection, subtitle parsing/discovery, transcription, translation, provider routing, queue execution, settings/secrets, and Windows integration.

The GUI must not own processing state. It should observe jobs and issue narrow commands so processing remains testable and recoverable.

Pin dependencies and external binary versions. Record redistribution licenses and notices. Resolve the appropriate FFmpeg build and model/binary packaging before release.

## 14. Branding and documentation

### Icon and credits

Create an original SubForge icon. Proposed visual direction: a subtitle frame with a forge spark, readable at small sizes and on light/dark backgrounds.

Deliver editable/source artwork plus PNG and multi-resolution ICO assets. Use the icon in:

- SubForge application executable.
- Setup executable.
- Start menu and optional desktop shortcut.
- Explorer context menu.

Use the exact credit **Sidos - info@sidos.gr** in About, documentation, installer branding, and appropriate publisher/author metadata. Retain legally required third-party credits.

### Offline HTML help

Provide detailed documentation in all ten interface languages. Open it from inside the app with local navigation and a language selector.

Cover:

- Installation and uninstallation.
- Right-click, multi-file, and folder workflows.
- Local Base/Small selection and model downloads.
- Setup/authentication for each online speech and translation route.
- Custom endpoints.
- Model/provider ordering and fallback behavior.
- Free allowances, introductory credits, and paid opt-in.
- Retry, delay, cooldown, and concurrency settings.
- Existing-subtitle selection and audio-track selection.
- Output naming, formats, and replacement behavior.
- Pause/resume/cancel and recovery.
- Common errors and sanitized diagnostics.
- Data handling, credits, and third-party notices.

Documentation must work offline after installation. Test internal links, navigation, text rendering, keyboard use, and both themes.

## 15. Installer and distribution

Final artifact: one `SubForge-Setup-x64.exe`.

- Install the app, required media tools, icons, and all offline help.
- Include or reliably provision the required WebView2 runtime; proposed release packaging embeds its offline installer.
- Keep Whisper model downloads optional within the app.
- Bundle agent components only where redistribution is permitted; otherwise provide guided installation/authentication.
- Do not require Rust, Node.js, Python, or compiler tools for normal end-user operation. A selected third-party agent may need a managed runtime that setup must handle or clearly explain.
- Support upgrades without losing settings, credentials, or resumable work.
- Register the app icon and selected shortcuts/context-menu entries.
- Uninstall only owned application components and registrations.
- Offer a clear choice about deleting downloaded models and user settings.
- Validate minimum Windows builds against the chosen dependencies.
- Signing requires an appropriate certificate/service and publisher identity. Do not claim that an unsigned build is signed or that a test certificate provides normal public trust.
- A single installer does not imply that online AI works without internet, accounts, or first-run credentials.

## 16. Build milestones

| Milestone | Reviewable result |
| --- | --- |
| M0 — Feasibility | Rust/Tauri shell; media probe; right-click/installer prototype; one real translation route; one timestamped online transcription route; Base/Small local proof |
| M1 — Subtitle engine | Discovery, embedded extraction, parsing, translation batches, validation, safe SRT saving |
| M2 — Speech backends | Both local choices, model management, all agreed cloud speech adapters, large-file handling |
| M3 — Provider resilience | Requested agent/API integrations, custom endpoints, model/provider ordering, free/paid controls, retries/cooldowns |
| M4 — Queue and interface | Multi-file handling, configurable parallelism, persisted progress, cancel/resume, settings, themes |
| M5 — Localization and help | Complete ten-language interface/help, final icon, credits, accessible layout |
| M6 — Release | Single installer, clean-machine validation, upgrade/uninstall checks, documented integration results |

Each milestone should record what was implemented, what was actually tested, and any remaining live-provider or packaging dependency.

Mocks may verify failure handling; they do not prove a real provider integration works. Do not substitute placeholder provider adapters for completed functionality.

## 17. Acceptance checks

### Core workflows

- [ ] Right-clicking one supported video queues it correctly.
- [ ] Multiple selected videos and folder input work with Unicode paths.
- [ ] Later launches forward selections to the existing instance.
- [ ] Greek is the initial subtitle target and English the initial interface language.
- [ ] Matching external or embedded text subtitles are processed before audio.
- [ ] Unrelated subtitles in the same folder are ignored.
- [ ] Suitable target-language subtitles are skipped by default.
- [ ] Forced-only/incomplete tracks are not mistaken for full dialogue.
- [ ] A video without subtitles produces timestamped subtitles through local or online transcription.
- [ ] The selected audio track is used.
- [ ] Silent, corrupt, unsupported, and unwritable inputs produce actionable outcomes.

### Local and cloud backends

- [ ] Base and Small can each be downloaded, verified, selected, and removed.
- [ ] Both local models can transcribe with network access unavailable.
- [ ] The app does not upload audio when local cloud fallback is disabled.
- [ ] Groq, Cloudflare, Gemini, Deepgram, and Speechmatics adapters are present and their verification status is recorded.
- [ ] Azure Speech and Google Translate are absent.
- [ ] OpenCode/Zen, OpenRouter, Antigravity, Pi, and custom translation routes support the agreed configuration.
- [ ] Provider/model selection is explicit and its order is respected.
- [ ] Text-only routes are excluded from audio transcription.
- [ ] Exhausted free routes never silently select paid usage.

### Reliability and output

- [ ] Simulated rate limits, invalid credentials, timeouts, network failures, and malformed responses take the correct retry/fallback path.
- [ ] Cooldowns apply across parallel files.
- [ ] Parallelism defaults to one and respects the configured limit.
- [ ] Pause/resume and application restart preserve completed work.
- [ ] Cancel stops owned work without damaging existing files.
- [ ] Long-video chunk boundaries do not create duplicated dialogue or timestamp offsets.
- [ ] Subtitle IDs and times survive translation correctly.
- [ ] Representative Greek and other-language results are reviewed for text quality and timing.
- [ ] Outputs are valid UTF-8 SRT; originals remain intact.

### Product and distribution

- [ ] All ten interface and help languages are present.
- [ ] Dark, Light, and Follow Windows modes work.
- [ ] Progress states are truthful and understandable.
- [ ] Secrets are protected and absent from logs/settings exports.
- [ ] Help works offline and all internal links resolve.
- [ ] Icon and exact Sidos credit appear in all required locations.
- [ ] A clean Windows 10 x64 installation is tested.
- [ ] A clean Windows 11 x64 installation is tested.
- [ ] Right-click integration is tested and its menu placement documented.
- [ ] Upgrade and uninstall behave correctly.
- [ ] The final deliverable is one setup EXE with documented external setup requirements.

## 18. Deferred features

- Local translation/chat models.
- Local Whisper variants other than Base and Small.
- OCR of image or burned-in subtitles.
- Subtitle burn-in, video re-encoding, or a full subtitle editor.
- macOS, Linux, 32-bit Windows, and ARM64 installers.
- Automatic purchase of credits, subscriptions, or signing services.
- Hosted user accounts, shared API keys, and a SubForge cloud backend.

## 19. Research references

The following official/project sources informed planning. Recheck capabilities, authentication, pricing, redistribution terms, and limits during implementation; the links do not represent tested integrations.

- [Tauri architecture](https://v2.tauri.app/concept/architecture/)
- [Tauri Windows installers](https://v2.tauri.app/distribute/windows-installer/)
- [Tauri external binaries](https://v2.tauri.app/develop/sidecar/)
- [Microsoft Explorer context-menu integration](https://learn.microsoft.com/windows/apps/desktop/modernize/integrate-packaged-app-with-file-explorer)
- [FFmpeg redistribution guidance](https://ffmpeg.org/legal.html)
- [whisper.cpp, model sizes, CPU/GPU support, and bindings](https://github.com/ggml-org/whisper.cpp)
- [Whisper languages and transcription/translation behavior](https://github.com/openai/whisper)
- [OpenCode CLI](https://opencode.ai/docs/cli/)
- [OpenCode Zen](https://docs.opencode.ai/docs/zen/)
- [OpenRouter free model variants](https://openrouter.ai/docs/guides/routing/model-variants/free)
- [Pi automation and embedding](https://pi.dev/docs/latest)
- [Pi custom providers](https://pi.dev/docs/latest/custom-provider)
- [Antigravity installation and authentication](https://antigravity.google/docs/cli/install)
- [Antigravity headless operation](https://antigravity.google/docs/cli/headless/)
- [Groq speech-to-text](https://console.groq.com/docs/speech-to-text)
- [Groq rate limits](https://console.groq.com/docs/rate-limits)
- [Cloudflare Whisper output](https://developers.cloudflare.com/workers-ai/models/whisper-large-v3-turbo/)
- [Cloudflare Workers AI pricing](https://developers.cloudflare.com/workers-ai/platform/pricing/)
- [Gemini audio](https://ai.google.dev/gemini-api/docs/audio)
- [Gemini pricing](https://ai.google.dev/gemini-api/docs/pricing)
- [Deepgram pricing and introductory credits](https://deepgram.com/pricing)
- [Speechmatics pricing and introductory credits](https://www.speechmatics.com/pricing)

