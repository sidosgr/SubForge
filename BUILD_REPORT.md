# SubForge 0.1.6 — build and verification report

Date: 14 October 2026
Credit: **Sidos - info@sidos.gr**

## Deliverables

- `artifacts/SubForge-Setup-x64.exe`: one Windows x64 setup executable.
- `artifacts/SHA256SUMS.txt`: SHA-256 checksum for that executable.
- Rust application/backend in `src-tauri`, Tauri desktop interface in `src`.
- Original icon source and executable icons in `public` and `src-tauri/icons`.
- Ten offline HTML user guides in `public/help`, accessible inside Help.
- `README.md`: setup, build, privacy and recovery instructions.

Final setup size: **359,639,686 bytes** (approximately 343.0 MiB).  
SHA-256: `571d7ad82dc97bb00bbec2af1b3ca76a3b550042410dfef27045daa1b56884f1`.

This is a functional first version for local evaluation. The target platforms are
Windows 10 and Windows 11 x64; verification here used Windows 11 Pro build 26200.
The acceptance checklist in `SPEC.md` remains a release checklist, rather than a
claim that every environment and external account has been validated.

## Implemented behavior

The app searches matching external subtitles and embedded text subtitle tracks
before extracting audio. It skips suitable subtitles already in the target
language. Otherwise it preserves cue times during translation, or creates timed
text through local/cloud recognition before translation. SRT and WebVTT output
use UTF-8 and collision-safe filenames; overwriting requires an explicit setting.
Image subtitles and burned-in captions require OCR and are outside this version.
Input must contain a decodable audio stream or usable text subtitles.

The interface defaults to English, Greek output, dark mode and one file at a
time. All ten requested interface/help languages are present. Settings cover
audio/subtitle selection, output, local/cloud recognition, ordered providers and
models, retries, cooldown, paid-route opt-in and one to eight parallel files.
Unfinished jobs and validated chunks/batches persist for explicit resume.

Explorer integration registers per-user video/folder verbs with the app icon and
forwards new selections into the existing instance. On Windows 11 this classic
verb appears under **Show more options**. There is no modern Explorer shell
extension. The queue can also receive selected files through its file picker or
drag and drop.

Keys use Windows Credential Manager. Settings exports and diagnostics exclude
keys. Local Whisper processing uses one CPU worker and four threads; downloading
Base or Small is optional. Each model is pinned and SHA-256 verified. Cloud
fallback from local recognition requires its explicit setting.

## Tests actually completed

### Current 0.1.6 recognition boundary repair

The supplied log failed in local recognition at part 8 of 10, before any
translation request. Retry reused parts 1–7 correctly, then repeated the same
`subtitle_out_of_bounds` error. Read-only inspection confirmed a 300-second cached
WAV and a final cue from 299.52 to 309.52 seconds. The normalized local recognition
now intersects that cue with the real audio boundary, retaining all text, IDs and
start times. Entirely outside, reversed and nonfinite cues remain errors. External
subtitles and provider output are not silently rewritten.

Boundary repair logs include the cue ID, start, original end, corrected end and
audio duration. Failure messages now include the offending cue/range/duration;
final log events retain the last processing stage, part/batch activity and elapsed
time. The new repair status and offline instructions are translated into all ten
languages.

36 Rust tests cover the exact overrun, absolute offset restoration, preservation
of every cue in a 75-cue timing-only regression fixture, and strict rejection of
cues entirely outside the audio (including the previous two-second tolerance).
The fixture contains synthetic dialogue only.

Five desktop checks in `artifacts/boundary-retry-016-regression.json` exercise a
ten-part synthetic file. They verify a detailed part-8 error, retry reusing seven
completed parts, boundary-repair metadata, CLI translation of all 84 synthetic
cues with corrected absolute timings, and the localized file-log display.

`artifacts/boundary-local-016-live.json` records the actual bundled Whisper Base
CPU processing a local copy of the reported failing 300-second cached audio.
All online speech/translation routes were disabled. Timed output completed,
boundary repairs were logged, and the original cache files were unchanged. This
checks the failed audio part, not full-episode Greek translation; no episode audio
or dialogue was uploaded.

### Historical 0.1.5 progress, logs and retry repair

A cached Whisper result contained cue 34 with identical start/end timestamps,
00:02:03,840. Strict validation rejected that recognition artifact with
`invalid_timestamp`. Only local recognition output now receives a short interval
bounded by the next cue and the audio duration, preserving its text. Reversed,
negative, nonfinite, out-of-bounds timings and external subtitles stay strict.

Retry clears the prior error and queues the file automatically with current saved
settings. Source, audio-chunk and translation checkpoints are validated before
reuse; invalid or truncated ones are rebuilt. Valid completed work is retained.
Queued jobs cannot be launched again while the previous worker is stopping.

Queue cards show stage-specific activity, audio part/time range or translation
batch/cue count, provider/model, request attempts, cooldown and elapsed time.
Transcription has a small animated microphone; translation has a Rosetta Stone.
Reduced-motion preferences disable animations. New status/log labels and help
instructions are translated into all ten supported languages.

Each job has an append-only JSONL log and in-app File log/Export log controls.
Entries record time, app version, file attempt, stage, selected media stream,
route/model/attempt, checkpoint reuse, timing repairs, quota delays and failures.
Prompts, dialogue, audio, credentials and raw terminal output are excluded. Local
filenames and paths remain. Logs survive restarts and clearing completed entries.

34 Rust tests passed, including zero-duration repair, strict timing rejection,
invalid/truncated transcript recovery, translation checkpoint validation, log
append/persistence, redaction and traversal prevention. Eight desktop checks in
`artifacts/progress-retry-015-regression.json` exercise the complete local parsing
path using a synthetic FFmpeg stand-in and a synthetic agent. They verify Retry
from a legacy `invalid_timestamp` state, rebuilding corrupt source/chunk caches,
timing preservation, both graphics, log viewing/export, distinct per-file logs,
reuse after a later failure and persistence across restart. This fixture does not
claim live AI translation quality.

`artifacts/progress-local-015-live.json` records a separate actual bundled
FFmpeg/Whisper Base CPU transcription of generated speech with no online calls.
It verifies timed output and a persistent file log without dialogue.

Eight additional checks in `artifacts/cli-reply-015-regression.json` verify that
logging preserves hidden OpenCode, Antigravity and Pi process calls, complete JSON
answer parsing, adaptive malformed/empty/missing-cue recovery, quota fallback and
zero direct endpoint requests from the agent adapters.

### Historical 0.1.4 repair: 28 backend tests and 18 desktop checks

All four agent provider types (Zen, OpenCode, Antigravity and Pi) are rejected by
the direct API adapter before credentials or network access. They invoke managed
official CLI executables in the background. The desktop fixture HTTP trap
received zero requests for these agent routes.

Translation batches now contain at most eight cues and 3,000 UTF-8 bytes (a
single larger cue stays intact). OpenCode receives a dedicated translator agent
and complete text answers are parsed independently. Antigravity receives its
native JSON schema and its structured output is read directly. Malformed, empty
and incomplete replies are split into smaller requests with the configured
cooldown; quota errors go to normal retry/provider fallback. Exact cue coverage
and original timestamps remain required, and completed batches are checkpointed.

`artifacts/cli-reply-regression.json` records eight passing desktop checks for
OpenCode, Antigravity and Pi, malformed/empty/missing-cue recovery, quota fallback,
hidden processes and the direct-network guard. These use synthetic CLI fixtures
to exercise failures reproducibly; they do not prove authenticated Pi or
Antigravity translation.

`artifacts/cli-transcript-live.json` records real official OpenCode 1.19.0 calls
with generated text and a keyless isolated profile. Each of the three requested
free models translated 40 cues into Greek with timestamps preserved (120 cues
total). Local Whisper Base also transcribed generated audio into three cues and
OpenCode translated that transcript successfully. The short transcription stage
was missed by UI polling; its local transcript checkpoint and finished output
were verified. No user video, audio or credentials were used. These live checks
ran the final translation implementation before the release-version bump.

`artifacts/signin-014-regression.json` records ten passing desktop checks. Google
setup advances native appearance/terms screens with paced key input. Native
Google hyperlinks are recognized without exposing terminal text or authorization
URLs. Browser launch uses Windows' default URL handler. An actual visible Thorium
window opened the public Google privacy page, recorded in
`artifacts/browser-launch-014.json`; this is browser-launch evidence, not completed
OAuth consent. Cached native sessions show a generic explanation and an explicit
Switch Google account action. The logout command was exercised only against an
owned fixture and is never issued by a normal sign-in click. Existing native
preferences are preserved. The official Antigravity runtime reached its real
terms screen; real-account consent/callback and authenticated translation remain
unverified.

The 28 Rust tests cover the prior parsing, media, credential, cost and recovery
checks plus independent complete JSON answers, incomplete replies, bounded
translation ranges, agent transport guards, terminal hyperlinks, native session
detection and preservation of native preferences.

### Historical 0.1.3 backend baseline: 24 passing tests

Parsing Unicode SRT/VTT and ASS with commas; cue ID preservation; chunk-boundary
deduplication; external subtitle discovery; main audio selection; valid empty
cloud chunks; speech language normalization; structured CLI errors without
response headers/body; free/paid/credit gates; collision-safe output and eight
concurrent writes; Windows rename fallback refusing an existing destination.
Additional checks cover Zen/OpenCode sharing one managed runtime, bare and qualified free-model IDs, rejection of direct Zen API calls before network/credential access, normalized Gemini URLs, Antigravity authentication-error
redaction, imported provider IDs staying inside the profile directory, rejection
of incomplete runtime installations, and ZIP traversal prevention. New tests cover fragmented terminal cursor-query replies and validation of Google OAuth/public legal URLs.

### Historical 0.1.3 OpenCode Zen CLI update: 12 passing desktop checks

`artifacts/zen-cli-013-regression.json` records checks against the actual 0.1.3
WebView2 desktop app with an explicitly isolated test executable, synthetic
credentials and loopback endpoint. This verifies application integration and
failure handling; it is not a live-provider translation success claim.

- Existing Zen provider IDs retain their saved-key reference and show the masked
  field. The editor exposes managed-agent setup and hides executable/endpoint fields.
- Connection tests, translation and catalog refresh all invoke the background CLI.
  The fixture verifies that there is no console window, the saved key arrives only
  in the child environment, and its configuration uses an environment reference.
- The three requested bare IDs and already-qualified `opencode/` IDs are accepted.
  CLI model discovery refreshes its cache and returns only Zen IDs.
- An error event with a zero process exit status is treated as failure. A model
  403 tries the next configured OpenCode model; malformed output also falls back.
- A known paid model is blocked before CLI launch in free-only mode. A keyless
  profile does not receive another saved provider key.
- A full subtitle job retries a transient CLI error, observes its delay, and
  preserves cue times. Exhausted CLI models move to the next provider. No direct
  Zen HTTP fallback occurs. Temporary request attachments are cleaned up.

`artifacts/zen-cli-013-live.json` records live checks using official OpenCode
1.19.0, a unique keyless profile and generated text only. All three requested
models (`space-bunny-free`, `muse-spark-1.3-contributor-free`, and
`mimo-v2.6-flash-free`) passed real Greek translation connection tests. A full
synthetic subtitle job produced two Greek cues with original timestamps intact.
No user key or media was used. The successful keyless route does not establish
that a particular saved account key has valid access.

The 0.1.2 free-tier rejection came from our blanket tool-deny configuration:
removing the CLI tool schemas triggered Zen's free-tier gateway. The repaired
configuration preserves the official schemas with ask permissions and supplies
`run --no-auto`, which automatically rejects permission requests. A separate
live test requested a harmless marker-file write in an owned fixture directory:
the official CLI reported auto-rejection, the tool returned a permission error,
and no file was created. The request reported cost zero. Evidence is recorded in
`artifacts/opencode-permission-check.json`.

### Historical 0.1.3 Google setup repair: 7 passing desktop checks

`artifacts/signin-fix-regression.json` records five checks with an isolated native
console fixture and two checks with the official Antigravity 1.3.3 executable.
The hidden terminal answers fragmented cursor-position queries and advances
appearance setup; missing this response caused the previous startup hang.
SubForge turns optional interaction sharing off and pauses at Google's terms
screen until the user clicks Continue. Public terms/privacy controls and sign-in
progress are shown in the app. A browser retry is offered only when the CLI
supplies a validated Google OAuth URL; raw terminal data and authorization URLs
are not returned to the UI. Finish closes the helper and asks the user to test
access rather than reporting successful authentication. Closing the editor stops
the owned helper and polling.

The official native executable reached the actual terms screen after the fix.
We did not accept Google's real terms or complete a real account's browser
consent during testing. The full OAuth callback and authenticated Antigravity
translation remain unverified. Existing valid native account sessions may avoid
opening a browser. `artifacts/SubForge-Google-setup-fixed.png` shows the setup UI.

### Previous 0.1.1 provider setup baseline: 15 passing checks

`artifacts/provider-regression.json` records actual desktop checks for the
0.1.1 update. Cloudflare ID and executable fields are absent from the Gemini
editor; the Cloudflare ID appears for Cloudflare speech. Stored keys display a
masked filled value, blank/unchanged values preserve the actual credential, and
explicit deletion clears both the field and secure store. A bare Gemini endpoint
and a `models/` prefix produced the correct request against a loopback service;
catalog filtering excluded an embedding-only model. A 404 received actionable
text with the raw response folded into Details.

SubForge automatically downloaded, hash-verified and launched official
Antigravity 1.3.3, OpenCode 1.19.0 and Pi 1.1.0 runtimes and retrieved their model
catalogs in the background. The private Antigravity key profile was created
automatically without a persisted key. With a synthetic invalid key, the official
agent reached Gemini's API and reported authentication failure, proving the
runtime/configuration setup path rather than successful authorized translation.
Hidden pseudo-terminal sign-in process control was exercised with an owned mock
command process. Completing Google browser consent with a real account remains
unverified; it is not counted as a successful OAuth integration test.

### Initial-version desktop baseline: 23 passing checks

The tests connect to the app's own WebView2 window, rather than a browser mock of
the frontend. Results are in `artifacts/desktop-tests.json` (13 checks) and
`artifacts/recovery-tests.json` (10 checks).

- Both Whisper Base and Small recognized generated English speech and produced
  timed SRT output locally. Auto language detection worked.
- External English subtitles translated to Greek through a synthetic loopback
  service with preserved timestamps. Ordered 429 fallback, 401 handling, 503
  retry and malformed JSON fallback passed.
- Paid providers produced no request without explicit opt-in. Export omitted
  the synthetic test key.
- Embedded target-language text subtitles were found before audio. Silent and
  corrupt inputs produced failures without invented output.
- Pause/restart/resume, queued cancellation and active recognition cancellation
  passed. Owned recognition stopped before model removal.
- In-app Base download resumed from a partial file and passed its hash check.
  Small download/removal also passed.
- A second instance forwarded a Unicode filename; the job produced WebVTT.
  File inspection remained open across progress refreshes.
- All ten interface languages and all ten offline help guides loaded. Light and
  dark appearance were exercised; screenshots are in `artifacts`.

These fixtures use synthetic speech, short clips and simulated translations.
They do not establish human translation quality across all ten languages, long
feature-film timing, performance on older hardware, or successful cloud speech.
Follow Windows mode is implemented but was not tested by changing the host OS
theme. A network-disconnected operating-system test was not performed; local
recognition itself does not call a transcription service.

### Updated installer package

The 0.1.6 NSIS package was inspected and its application executable extracted.
Windows version metadata reports 0.1.6 and the Sidos credit. The embedded program
matches the final compiled executable apart from the expected NSIS bundle marker.
The delivery checksum matches the built 0.1.6 setup. The update is not installed
over the user's currently running app; the existing app and credentials were left
in place. Close SubForge and run the new setup to upgrade.

### Initial-version installer baseline: 9 passing checks

The isolated installation launched successfully, read its retained Greek UI /
German target settings, contained the media tools/license notices, and opened
Greek help offline. Its executable contains an extractable icon. Registry
inspection confirmed the localized Explorer verb, quoted command/icon paths and
multi-selection registration. The final setup silently upgraded the isolated
installation, retaining settings, a model-folder marker and the saved test key.
The installed release passed a focused 429 test: it waited for Retry-After,
retried once and accepted a normalized Greek language response.

Default silent uninstall removed the executable, Add/Remove Programs entry and
Explorer registration while preserving settings, model files and the credential.
The optional data-removal backend then removed the isolated profile and its
referenced credential; a fresh launch restored defaults and confirmed the key
was absent. The graphical data-removal checkbox itself was not clicked. All test
app processes, installer registrations and the isolated profile were cleaned up.
Results are in `artifacts/installer-tests.json`.

The installed executable matches the final compiled program except for Tauri's
three-byte bundle marker (`UNK` becomes `NSS` for an NSIS installation). This is
the expected packaging change, verified against Tauri's bundle-type code.

Manual Explorer menu clicking and clean-machine installs were not performed.
The installed copy was isolated from the normal user profile with
`SUBFORGE_DATA_DIR`; temporary credentials contained only fixture text.

## Provider verification status

All routes below contain request/response implementations, not placeholder
adapters. Account credentials and current provider entitlements are still needed
to verify a particular user's route. No shared keys or guaranteed free quota are
included in SubForge.

| Route | Implemented integration | Evidence / remaining validation |
|---|---|---|
| Local Base / Small | FFmpeg's whisper.cpp filter; CPU recognition | Both models transcribed synthetic speech in the desktop app; download hashes verified |
| Groq | Audio multipart upload, verbose segments | Implemented; no authenticated live speech call |
| Cloudflare Workers AI | Account/token, Whisper audio and timed output | Implemented; no authenticated live speech call |
| Gemini | Inline audio and structured timed response; text translation | Implemented; no authenticated live call |
| Deepgram | Prerecorded audio, utterance/word timing | Implemented; introductory-credit route requires explicit enablement; no live call |
| Speechmatics | Create/poll/download/delete transcription job | Implemented; introductory-credit route requires explicit enablement; no live call |
| Custom speech | Compatible transcription endpoint | Implemented; a chosen endpoint must support the documented response format |
| OpenCode CLI / Zen | Managed official CLI only; saved key through child environment; normalized model IDs; dedicated translator agent; CLI model refresh | Three requested free models each translated 40 synthetic cues into Greek with timing preserved; a locally generated transcript also translated. User saved-key entitlement remains unverified |
| Pi | Headless restricted agent; provider/model configuration; validated adaptive replies | Current hidden-CLI/reply fixtures passed. Earlier live Zen free-model requests received 403; a deprecated model returned 410. A configured supported account/provider requires live validation |
| OpenRouter | Chat API and model discovery; explicit `:free` routing | Implemented; no authenticated live translation call |
| Antigravity CLI | Managed official runtime; native structured output; hidden Google sign-in helper or automatically configured Gemini-key mode | Ten current sign-in checks passed; actual terms screen and visible Windows default-browser launch verified. Hidden CLI, malformed-output recovery and quota fallback fixtures passed. Real-account OAuth/translation remains unverified |
| Custom translation | Chat Completions, Responses or Gemini-compatible endpoint | Chat route exercised through a loopback service; other protocols implemented but not live validated |

The OpenCode 403 observed in 0.1.2 was resolved by preserving its official CLI
tool schemas while automatically rejecting permission requests. Free/paid gates,
model order and fallback remain in force. Provider availability, quotas and
introductory credits can change.
Azure Speech and Google Translate are absent. Local translation models are absent.

## Packaging, dependencies and release limits

The installer embeds media executables, license texts and Microsoft's offline
WebView2 installer. End users do not need Rust, Node or Python for the app or
local recognition. Agent runtimes are automatic optional downloads managed by
SubForge. Users do not need to install CLIs or locate executables. Google browser
sign-in still needs the user's consent. Existing external CLI configurations
remain unchanged; keys are supplied only to child processes. Whisper weights are
optional downloads, approximately
148 MB for Base and 488 MB for Small; they are not inside the setup executable.

The app and installer are **unsigned**. No signing certificate was supplied.
Clean Windows 10 and clean Windows 11 VM testing remains a release task. The
offline WebView2 payload is packaged; its first installation on a machine with
no WebView2 runtime has not been exercised here.

The bundled Gyan FFmpeg 9.0.2 full static build is GPL v3. Its license, supplier
build manifest and exact FFmpeg source archive are included alongside notices
for Rust/frontend dependencies and Whisper. A complete corresponding-source
bundle for the media build's external dependencies has not been assembled.
Review and supply those sources before public redistribution; this artifact is
provided for local evaluation and is not marked as a cleared public release.

Personal input media and their fingerprints are excluded from this public source repository.
No user video/audio was uploaded. Only synthetic text was sent in the live agent
checks. Temporary development tools, models and synthetic fixtures live under
the ignored `.tools` directory and are not included as developer runtimes in the
installed app.
