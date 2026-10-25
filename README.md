# SubForge

**Sidos - info@sidos.gr**

[English](README.md) · [Ελληνικά](README.el.md)

**English:** SubForge creates and translates video subtitles on Windows 10/11.
Right-click a video or folder, reuse existing subtitles or transcribe speech
locally with Whisper Base/Small, then translate with background AI-agent CLIs.
Greek is the default subtitle language; ten languages are supported.

**Ελληνικά:** Το SubForge δημιουργεί και μεταφράζει υπότιτλους βίντεο στα Windows
10/11. Με δεξί κλικ σε βίντεο ή φάκελο, αξιοποιεί υπάρχοντες υπότιτλους ή
απομαγνητοφωνεί τοπικά με Whisper Base/Small και μεταφράζει μέσω AI agents στο
παρασκήνιο. Προεπιλογή είναι τα ελληνικά, με υποστήριξη δέκα γλωσσών.

This public repository contains the application source, build scripts and offline
guides. Windows installers are listed on [GitHub Releases](https://github.com/sidosgr/SubForge/releases).
See [release status](RELEASE.md) for validation and packaging limitations.

Windows 10/11 x64 desktop app. Rust handles media, jobs, local recognition,
provider requests, credentials and Explorer integration; Tauri hosts the small
TypeScript interface. English is the initial interface language; Greek the subtitle target.

## Use

After building locally, install `artifacts/SubForge-Setup-x64.exe`. In Settings, choose local Whisper
Base/Small or an online speech provider. Local models are optional downloads;
Python, Node, Rust and a GPU are not required for normal local operation.
Configure a translation provider and select its models for subtitles in another
language. API keys are stored in Windows Credential Manager.

Providers and models run in their displayed order. Paid and unknown-cost routes
need explicit permission; introductory credits need separate opt-in. Free routing
also rejects known Zen/OpenRouter paid model identifiers. External account billing
settings still apply; use spending limits and disable automatic credit replenishment.

Right-click a supported video/folder and choose **Create subtitles with SubForge**.
Windows 11 places this in **Show more options**. Multiple selections forward to the
same queue. Files/folders can also be added or dragged into the app. Matching sidecar
and embedded text subtitles are checked before audio. Image subtitles use speech
recognition. Default output is UTF-8 SRT beside the video; WebVTT is also available.
Existing files are preserved unless replacement is explicitly enabled.

Queue cards show the current audio part or translation batch, cue count, active
model, request attempts, cooldown and elapsed time. A small microphone animation
accompanies transcription and a Rosetta Stone accompanies translation. Animations
follow Windows' reduced-motion preference.

Settings → Processing includes **Translation parts per file**, default 1 with
options up to 4. Healthy requests have no scheduled pauses, including when using
older settings profiles. Providers share request slots across files. HTTP 429
and recognized agent rate-limit errors immediately try another eligible enabled
provider. If every route is limited, retries use short adaptive backoffs starting
at one second, while honoring server-requested retry times. Learned spacing is
250–2000 ms and clears as successful requests recover.

Saving settings shows a persistent confirmation near the Save button. Setting
and provider info icons explain each option on hover or keyboard focus (English
and Greek, with English fallback for other interface languages). Translation
responses and cached parts are checked for foreign-script characters and icons;
invalid text is retried and cannot be written to the final subtitle.

Each active part shows its cue range, provider/model, attempt, response age and
timeout or pause countdown. Overall completion remains visible while requests
wait. Parts keep independent validated checkpoints and are assembled in source
order even when replies arrive out of order. After a failure, already active
parts finish and save their results before the file stops; no new parts start.

Each file has a **File log** button and an **Export log** action. The viewer
updates automatically while open. Per-request elapsed times, scheduled pacing,
actual limits and provider fallbacks are recorded. Append-only JSONL
logs in the application data folder's `logs` directory retain timestamps, app
version, per-file attempts, stages, media information, selected routes/models,
cache reuse, timing repairs, retries, cooldowns and failures. Logs exclude request
prompts, transcript dialogue, audio, credentials and raw terminal output. They
contain local filenames/paths; review those before sharing a log. Clearing finished
queue entries leaves these logs on disk.

Retry immediately clears the previous failure, queues the file and uses the saved
current settings. Valid transcript chunks and translation batches are reused;
invalid or truncated checkpoints are rebuilt. Whisper occasionally emits a cue
with identical start/end times. SubForge gives that recognition cue a short valid
interval bounded by the next cue and audio duration, preserving its text. Other
invalid timings remain errors with cue IDs and times in the log; external subtitle
files are still checked strictly.

A Whisper cue that starts inside an audio part but ends beyond its duration is
clipped to that part's actual end. Its text, ID and start time are retained.
Entirely outside, reversed and nonfinite cues remain errors. Boundary repair
entries include the original end, corrected end and audio limit; final failure
entries include the preceding stage, part/batch context and elapsed time.

Detailed offline help is available inside the app in all ten interface/subtitle
languages. Its source is `scripts/build-help.mjs`; generated pages are in `public/help`.

## Providers

Speech: Groq, Cloudflare Workers AI, Gemini, Deepgram, Speechmatics and custom
compatible multipart endpoints. Microsoft Azure Speech and Google Translate are absent.
Only Whisper Base/Small local transcription models are supported.

Translation: OpenAI (ChatGPT OAuth), OpenCode Zen through the managed CLI, OpenRouter, OpenCode CLI, Antigravity CLI, Pi and
custom chat-completions/Responses/Gemini endpoints. Select models explicitly;
configure credentials, retries, timeout and cooldown per route.

OpenAI (ChatGPT) uses OpenAI’s documented [Sign in with ChatGPT flow](https://developers.openai.com/siwc/token-sharing-open-source/sign-in).
In Settings → Translation, edit OpenAI (ChatGPT), choose **Continue with ChatGPT**,
and authorize plan usage in your browser. Existing profiles can add it with
**Add provider**. Refresh models, choose an account model, test the connection, save
and enable the route. Each OpenAI provider keeps its own account/workspace
registration; add another provider to retain a second account. No API key or agent
installation is required. This route translates text; it does not transcribe audio.

Eligible ChatGPT plan usage and any credits allowed in ChatGPT settings apply.
The provider displays **Using ChatGPT plan**, with **Manage usage**, reconnect and
sign-out controls. OAuth tokens stay in an app-owned file encrypted with Windows
DPAPI for this Windows account and machine. They are refreshed automatically and
excluded from settings exports, the webview and logs. Sign-out attempts remote
revocation and removes local tokens while retaining the account registration.

In Settings → **App updates**, choose **Check for updates**. The app checks the latest
published stable release in sidosgr/SubForge on GitHub and displays release notes.
**Download and install** saves settings, downloads the Windows installer, verifies
its exact size and SHA-256 against GitHub’s asset metadata, and opens the update
installer. Installation requires idle subtitle workers and model downloads;
SubForge closes and restarts after installation. The install location and profile
are preserved. Failed checks can be retried; releases without a verifiable installer
cannot be installed from this panel. Development builds cannot launch installers.

For future releases, publish a stable semantic-version tag such as v0.1.7 and
upload the build output as exactly `SubForge-Setup-x64.exe`. GitHub must expose its
SHA-256 asset digest. No separate update manifest or API token is required.

SubForge automatically downloads and verifies its own official agent runtimes
when Set up agent, Refresh models or Test connection is used, or an agent job
starts. No executable path, command entry, PATH change, Node installation or
manual CLI settings-file edit is required. Downloads are optional and pinned:
OpenCode 1.19.0, Antigravity 1.3.3 and Pi 1.1.0. Agent calls run without terminal
windows and use application-owned configuration. API keys are supplied only to
the child process, never written into its settings.

OpenCode Zen always uses the managed OpenCode CLI for translation, connection tests and model discovery. Existing Zen settings and saved keys are retained. Bare IDs such as `space-bunny-free`, `muse-spark-1.3-contributor-free` and `mimo-v2.6-flash-free` automatically become `opencode/<model-id>` inside the CLI. Model order, retries, cooldown and paid-route gates still apply. Refresh models updates the official CLI catalog. SubForge never falls back to direct Zen API calls.

The official OpenCode tool schemas remain available with ask permissions, while
`run --no-auto` automatically rejects tool permission requests. Disabling all
tool schemas caused the free-tier 403 in 0.1.2. All three listed free models
each translated 40 synthetic cues successfully through the repaired official CLI
integration in 0.1.4, preserving all cue times.

Agent translation sends only subtitle text through the selected CLI. Batches are
bounded to eight cues and 3,000 UTF-8 bytes (a single longer cue stays intact).
Malformed, empty, or incomplete replies are split and retried in smaller batches.
Every cue ID and original timestamp is validated before saving. OpenCode uses a
dedicated translator agent; Antigravity uses its native JSON-schema flag and
structured output. Complete OpenCode answers are parsed separately, rather than
concatenated into invalid JSON. Successful batches and the local transcript remain
checkpointed so a later failure can resume without retranscribing.

Antigravity defaults to Google account sign-in. Choose Sign in with Google.
SubForge initializes its hidden terminal and advances appearance setup. Review
the displayed Google terms and privacy links, then choose Continue to accept and
finish setup; optional interaction sharing is switched off. Complete Google
sign-in in the browser. Use Continue if an authentication choice is waiting, or
Open sign-in page when available. Choose Finish, then Test connection to verify
access. Closing the helper does not itself verify authentication. A saved native Google session is shown explicitly instead of waiting silently.
Use Switch Google account only when you want a new login; it signs out the native
Antigravity CLI account on this PC using its own logout command. SubForge uses
Windows URL association to open the default browser and recognizes native terminal
hyperlinks. Setup navigation is paced through screen redraws. Native setup
preferences are preserved when managed security settings are written. API key mode automatically creates
its private Gemini provider configuration using the saved key. OAuth completion
still needs a Google account; SubForge cannot bypass provider entitlement checks.

Cloudflare's account ID appears only in its speech editor. Saved API key fields
show a masked value. Leaving a masked/blank field unchanged preserves the key;
Delete saved key explicitly removes it. For Gemini-compatible custom endpoints,
a bare server URL automatically receives /v1beta. Model discovery excludes
entries that cannot generate content. Refresh the catalog if a model returns 404.

## Build

Prerequisites: Windows x64, Rust MSVC, Visual Studio Build Tools with C++ workload,
Node 22+ and npm. Python/Pillow are needed only to regenerate raster icons.
Lockfiles pin dependencies. Media provisioning and build scripts live in `scripts`.

```powershell
npm ci
node node_modules/esbuild/install.js
pwsh -File scripts/prepare-media.ps1
pwsh -File scripts/build.ps1
```

The single installer includes Microsoft's offline WebView2 installer and media
tools. Whisper model downloads are optional, pinned and SHA-256 verified.
Local recognition uses four CPU threads with one local model worker at a time.
The file queue supports one to eight parallel files, defaulting to one.
Translation can additionally overlap one to four parts within each file; local
speech recognition retains its single worker.

## Verification and release limits

`BUILD_REPORT.md` records actual tests and adapter verification status. Unit tests
cover parsing, cue IDs/timing, chunk deduplication, discovery, cost gates and safe
concurrent saving. Desktop tests exercise the actual app, both local models and
loopback mock services. Mocks do not establish live cloud compatibility.

The installer is unsigned. Public signing and clean-machine validation on both
Windows versions need release infrastructure not available in this workspace.
Third-party software retains its licenses. The installer contains license texts,
the supplier's build manifest and exact FFmpeg source archive. Review complete
corresponding sources for the GPL media build's external dependencies before public
redistribution; this workspace provides a local evaluation installer.

The existing `test.mkv` is preserved. Test fixtures/model copies live under `.tools`.
No user video or audio was uploaded to a cloud provider.

## Data and recovery

Normal data location: `%APPDATA%\gr.sidos.subforge`. Configuration exports exclude
keys. Validated transcript chunks and translation batches survive retries/restarts.
Unfinished work waits for Resume after restart. Pause takes effect at a safe work
boundary; Cancel stops owned media/agent processes and pending requests.

The uninstaller keeps user data by default. Select its data-removal checkbox to
delete SubForge settings, models, checkpoints and keys referenced by the saved
configuration. Source videos and generated subtitles are retained. App-managed agent runtimes/profiles are removed when user data is deleted.
External installations remain unchanged; official CLI OAuth sessions may be held
in the provider’s own Windows credential store. `SUBFORGE_DATA_DIR` isolates data for testing or a
separate profile and is never exported as a credential.
