VAULT — PROJECT CONTEXT FOR CLAUDE CODE

What Vault is
Vault is a cross-platform desktop application (macOS first, Windows later) that replaces traditional filename-based file search with AI-powered semantic search. Instead of needing to remember what you named a file, you describe what it contains in natural language and Vault finds it. A photo of you and your team at dinner returns when you search "team dinner" even if the file is named IMG_4821.jpg. A PDF about quarterly budgets returns when you search "Q3 spending" even if it's named finance_final_v2.pdf.
The core insight is that file search has always been metadata-dependent — name, date, tags — and Vault replaces that with content understanding using vector embeddings.

The tech stack

App shell: Tauri (Rust backend, React + TypeScript frontend)
Styling: Tailwind CSS, system fonts (SF Pro on macOS, Segoe UI on Windows)
Vector database: LanceDB (embedded, local, stored as a folder on disk in the app's data directory)
Embeddings: Google Gemini Embedding 2 API (gemini-embedding-2-preview)
Transcription: Whisper.cpp (local, runs on device, no API calls)
Platform: macOS first


How each file type gets processed
Images (jpg, jpeg, png, webp, heic) → sent directly to Gemini Embedding 2 API → vector stored in LanceDB
Documents and PDFs (pdf, txt, md, docx) → sent directly to Gemini Embedding 2 API → vector stored in LanceDB
Audio files (mp3, wav, m4a, aac) → transcribed locally using Whisper.cpp → transcript text sent to Gemini Embedding 2 API → vector stored in LanceDB
Video files (mp4, mov) → audio track extracted → transcribed locally using Whisper.cpp → transcript text sent to Gemini Embedding 2 API → vector stored in LanceDB
The reason audio and video go through Whisper first is to avoid the 80s audio cap in the Gemini API. Whisper handles files of any length by chunking, then the full transcript is embedded as text with no length constraint.

How search works
The user types a natural language query. The query text is sent to Gemini Embedding 2 API with task_type set to RETRIEVAL_QUERY (this tells the model to optimise the vector for retrieval rather than storage). The returned query vector is compared against all stored vectors in LanceDB using cosine similarity. The top 20 closest matches are returned and rendered as file cards. The whole search operation should feel near-instant — under 500ms.

The embedding vectors
Gemini Embedding 2 produces 3072-dimension vectors. Each vector stored in LanceDB should be accompanied by: file path (full absolute path), file name, file type, last modified date, and the transcript text (for audio/video files, for future use). Use task_type RETRIEVAL_DOCUMENT when embedding files and RETRIEVAL_QUERY when embedding search queries — this asymmetric setup significantly improves retrieval quality.

Re-indexing and the background daemon
A file system watcher runs as long as the app is open. It monitors all folders the user has selected. When a new file is added, it gets queued and embedded. When a file is modified, its vector is updated. When a file is deleted, its vector is removed from LanceDB. File events should be debounced to avoid redundant processing when files are saved multiple times rapidly.

Initial indexing
The first time a user indexes their files, there could be tens of thousands of files. The approach is: scan the filesystem first (fast, no API calls), count files by type, estimate time and API cost, show the user a summary screen before starting. Indexing then runs in priority order — Documents first (fastest), then Images, then Audio/Video last (slowest due to transcription). The app should be usable for document and image search within minutes even if audio/video is still processing.

API key model (MVP)
For the MVP, the user provides their own Google Gemini API key. It is validated on first launch by making a lightweight test call to the API. It is stored securely using Tauri's secure storage (Keychain on macOS). The user should never have to enter it again after onboarding. When the product moves beyond MVP, this switches to the developer's own API key with a subscription model — but for now the user's key is used.

Design language

Follows system light/dark mode — no forced theme
Finder-style layout: sidebar on left, results panel on right
Single accent color: purple #7C3AED used sparingly — active sidebar item left border, search bar focus ring, progress indicators, primary buttons, file type pills
System fonts throughout — SF Pro on macOS
Functional and minimal — no decorative elements, no gradients, no animations beyond necessary transitions
App name: Vault


What each stage delivers
Stage 1: App shell with layout — nothing works, just the UI
Stage 2: Onboarding — API key entry, validation, folder selection, persisted config
Stage 3: File scanner — filesystem crawl, file categorisation, count and estimate screen
Stage 4: Embedding pipeline — full processing pipeline, vectors in LanceDB
Stage 5: Search — query embedding, LanceDB similarity search, results UI, file preview panel
Stage 6: Background daemon — file watcher, automatic re-indexing
Stage 7: Polish — progress UI, error handling, settings screen, performance, edge cases
Each stage is independently testable. Do not start a stage until the previous one is confirmed working. If something in a previous stage needs to change to support the current stage, make that change explicitly rather than working around it.