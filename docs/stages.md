VAULT — BUILD STAGES

Stage 1 — Project Scaffold
Goal: Tauri app boots on macOS, window opens, Finder-style layout renders with sidebar and search bar. Nothing functional yet, just the shell.
Tasks:

Initialise a new Tauri project with React + TypeScript frontend
Set up Tailwind CSS with system font stack (SF Pro on macOS)
Build the sidebar with sections: All Files, Images, Videos, Audio, Documents — each with a simple icon
Build the main area with a prominent search bar (placeholder: "Search anything… try 'team dinner'")
Apply subtle purple (#7C3AED) as the only accent color — used on active sidebar item left border and search bar focus ring
App follows system light/dark mode
Empty results area with centred message: "Start typing to search your files"

Done when: App launches, layout matches the Finder-style mockup, sidebar items are clickable and change active state, no errors in console.

Stage 2 — Onboarding Flow
Goal: First-time user is guided to enter their Gemini API key, validate it, and select folders to index. App remembers this on relaunch.
Tasks:

Detect if this is a first launch (no stored config)
Show onboarding screen: Vault wordmark, API key input field with a Validate button
Call Gemini API with the key to verify it's valid, show success/error state
Folder picker — user selects one or more folders to index
Store API key and folder list securely using Tauri's secure storage
On subsequent launches, skip onboarding and go straight to main app

Done when: User can enter an API key, it gets validated against the real Gemini API, folders are selected, and on relaunch the app goes directly to the main screen with settings intact.

Stage 3 — File Scanner
Goal: App crawls the selected folders, identifies every file by type, counts them, and estimates indexing time and API cost. No embedding yet — purely the filesystem layer.
Tasks:

Rust backend crawls all selected folders recursively
Categorise every file into: Images (jpg, jpeg, png, webp, heic), Videos (mp4, mov), Audio (mp3, wav, m4a, aac), Documents (pdf, txt, md, docx), Other (skip)
Count files per category and total
Estimate indexing time based on file counts (use conservative estimates: ~0.5s per image, ~2s per audio/video due to transcription, ~0.3s per document)
Estimate API cost based on file counts and Gemini Embedding 2 pricing
Surface this data to the frontend
Show the estimate screen: file count breakdown, estimated time, estimated cost, "Indexing runs in the background. You can close the app.", Start Indexing button in purple

Done when: Point app at a real folder, it correctly counts and categorises all files, estimate screen shows accurate breakdown, Start Indexing button is visible and clickable (doesn't do anything yet).

Stage 4 — Embedding Pipeline
Goal: Every file gets processed through the correct path and its vector gets stored in LanceDB. This is the core of the product.
Tasks:

Set up LanceDB as the local vector store — stored as a folder on disk in the app's data directory
Build the embedding queue — a Rust async task queue that processes files in priority order: Documents first, then Images, then Audio/Video last
Documents/Images/PDFs: send directly to Gemini Embedding 2 API, store the returned 3072-dimension vector in LanceDB alongside file path, file name, file type, last modified date
Audio files: run through Whisper.cpp locally to transcribe to text first, then send transcript to Gemini Embedding 2 API, store vector
Video files: extract audio track, run through Whisper.cpp to transcribe, send transcript to Gemini Embedding 2 API, store vector
Handle API rate limiting with a queue throttle — no more than X requests per minute
Handle errors gracefully — if a file fails, log it and continue, don't stop the whole queue
Respect the Gemini API limits: images max 6 per request, video max 120s (use audio fallback regardless), audio max 80s (Whisper handles longer files by chunking)

Done when: Run indexing on a test folder containing at least one image, one PDF, one audio file, and one video. All four produce vectors stored in LanceDB. Verify by querying LanceDB directly and confirming vectors exist for each file.

Stage 5 — Search
Goal: User types a natural language query, it gets embedded, LanceDB returns the closest matching files, results render as file cards in the UI.
Tasks:

On search input (debounced 300ms), send the query text to Gemini Embedding 2 API with task_type: RETRIEVAL_QUERY to get a query vector
Run cosine similarity search against LanceDB, return top 20 results
Render results as a grid of file cards — each card shows: thumbnail (for images), file type icon (for others), filename, file type pill (JPEG / PDF / MP4 etc.) in light purple, file path on hover
Clicking a card opens a right-side preview panel: larger thumbnail or file icon, full filename, file path, date modified, file size, Open File button in purple
Clicking Open File opens the file in the OS default app
Empty state if no results: "No files found for '[query]'. Try different words."
Filter by file type works — clicking Images in the sidebar filters results to images only

Done when: Search "team dinner" and a photo of people eating returns even if the filename is IMG_4821.jpg. Search "budget" and a PDF with budget content returns. Results feel fast (under 500ms for the search itself).

Stage 6 — Background Daemon
Goal: App watches the indexed folders for changes and automatically re-indexes new, modified, or deleted files without any user action.
Tasks:

Use Tauri's file system watcher to monitor all indexed folders
On new file detected: add to embedding queue, process through correct pipeline, add vector to LanceDB
On file modified: re-embed and update the vector in LanceDB
On file deleted: remove the vector from LanceDB
Daemon runs as long as the app is open (including when minimised)
Status indicator in the top right of the app: "Index up to date" normally, "Indexing 3 new files…" when processing
Daemon should be lightweight — debounce file events so rapid changes (like saving a document multiple times) don't trigger multiple re-indexes

Done when: Drop a new image into an indexed folder, wait a few seconds, search for something in that image — it shows up. Delete a file, search for it — it's gone from results.

Stage 7 — Polish
Goal: App feels complete, handles edge cases gracefully, and is ready to be used daily.
Tasks:

Indexing progress bar at the bottom of the window during initial index: "Indexing in progress — Audio files (340 remaining)" with thin purple progress bar, checkmarks for completed categories, Pause button
Error states: what happens if the Gemini API key expires mid-index, if a file is corrupted, if LanceDB runs out of space
Settings screen: change API key, add/remove folders, trigger manual re-index, see storage used by the index
Onboarding edge case: user enters wrong API key, clear error message with a link to get a key
App icon, proper macOS menu bar integration
Performance: initial render under 200ms, search results under 500ms, no UI freezing during background indexing
Test on a library of 10,000+ files and confirm nothing breaks

Done when: App can be handed to someone with no context, they install it, complete onboarding, and find a file using natural language within 5 minutes. No crashes, no confusing states.