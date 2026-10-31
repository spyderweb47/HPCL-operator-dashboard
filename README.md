# HPCL Dealership — Operator (Desktop)

A Windows desktop app (Tauri 2) for the office operator: the daily cash register, day end and stock, history, and settings.
It works **fully offline**. Every entry is saved on the PC first, then uploaded to the cloud MongoDB
in real time whenever the internet is available.

- **No login.** Each installation is identified by its **Machine ID** (shown in Settings). Every record in the cloud carries it.
- **Same UI** as the web version: same keyboard flow, same rules (6-digit consumer number, Other product, ghost price, soft delete with audit, finalize lock).
- The previous Next.js web version is kept in `legacy-web/` for reference and isn't used by this app.

## How offline and sync work

```
 Operator ──► React UI ──invoke("call")──► Rust backend ──► SQLite on this PC  (always, instantly)
                                               │
                                               └─ sync worker ──► MongoDB in the cloud  (when online)
```

| Situation | What happens |
| --- | --- |
| Internet on | Each save wakes the sync worker; the entry is in the cloud within about 0.3 s. A 30 s check keeps the status pill accurate. |
| Internet off | Saving, editing, day end and history keep working from the local file. Changes queue up; the header shows `Offline · N waiting`, and each waiting row has a small upload icon. |
| Internet back | The worker retries every 5 → 30 s, and also right away after any new entry. It uploads the queue in batches of 200 with 8 uploads in parallel. |
| Fresh install on the same PC | The Machine ID comes from the Windows machine GUID, so a reinstall gets the same ID. On first connect, it restores this PC's delivery boys, product catalog and the last 180 days from the cloud. |
| Looking back in time | History reads the last **180 days** from this PC, so it works offline. Older days load from the cloud when online. |

**Keeping the PC light.** Once a day is older than 180 days *and* already in the cloud, it's removed from the PC (checked every 6 hours) and the space is returned to Windows. Unsynced rows are never removed. If cloud sync isn't set up at all, nothing is ever removed.
For scale: 200 entries a day for 180 days is about 36,000 rows, roughly 15–20 MB.

**Why sync can't lose or duplicate data.**
- Every row has `version`, which goes up on each local change, and `synced_version`, the last version the cloud confirmed.
- Upload is a full-document upsert keyed by a stable ID, so retries and crashes in the middle of a sync are harmless.
- A row edited while it's uploading keeps its newer version and goes up in the next round.

The local file is `%APPDATA%\com.hpcl.operator\operator.db`: SQLite in WAL mode. Only one copy of the app can run at a time; opening it again focuses the existing window.

## Setting the cloud database

Nothing is built into the installer. A fresh install runs as **This PC only** until the cloud is set up:

1. Open **Settings → Cloud Database**.
2. Paste the connection string from MongoDB Atlas (**Connect → Drivers**), for example
   `mongodb+srv://myuser:<db_password>@cluster0.xxxx.mongodb.net/`.
3. Type the **password** in the Password box. You only need to fill in Username if it differs from the one in the string. Special characters like `@`, `:` and `/` are handled for you.
4. Optionally set the **database name** (default `hpcl_dealership`), then click **Test connection** and **Save & connect**.

It takes effect immediately; no restart is needed.

- **New password:** type it and save; nothing else changes. Leaving Password blank keeps the saved one.
- **Different cluster or database:** everything on this PC (the last 180 days, delivery boys and catalog) is uploaded to the new one automatically.
- **Turn off cloud sync:** the app keeps working on this PC only.

The settings are saved only on this PC, in `%APPDATA%\com.hpcl.operator\cloud.json`. For development, the `HPCL_MONGODB_URI` and `HPCL_MONGODB_DB` environment variables are used when that file doesn't exist.

**Atlas checklist:**
- Create a database user with **readWrite** on the database only.
- Under **Network Access**, allow the office IP or `0.0.0.0/0`. A blocked IP shows as `Offline`; hover over the status pill or open Settings to see the exact reason.

## Cloud data structure (for the Owner/Admin dashboard)

Every document has `machineId`. Times are BSON dates in UTC; `dateString` is the business day (`YYYY-MM-DD`, India time).

**`machines`**: one document per installed PC, updated on every sync and at least every 5 minutes while online.
```js
{ _id: "90D7-C168-32F2-9F5D", deviceName: "OFFICE-PC", os: "windows", appVersion: "1.0.0",
  retentionDays: 180, pendingCount: 0, firstSeenAt: Date, lastSeenAt: Date }
```

**`transactions`**: `_id` is the UUID made on the PC. Never hard-deleted.
```js
{ _id: "uuid", machineId, dateString: "2026-10-30", type: "CASH_IN" | "CASH_OUT",
  productName: "Regulator", price: 350.0, consumerNo: "123456" | null, isOtherProduct: false,
  status: "ACTIVE" | "EDITED" | "DELETED", isDayFinalized: false,
  auditLog: [{ action: "CREATED"|"UPDATED"|"DELETED", previousProductName, newProductName, previousPrice, newPrice,
               previousConsumerNo, newConsumerNo, timestamp: Date }],
  createdAt: Date, updatedAt: Date, syncedAt: Date }
```
Indexes: `{machineId, dateString, createdAt}`, `{dateString}`, `{consumerNo}`.

**`dayendlogs`**: one per machine per day. Unique index: `{machineId, dateString}`.
```js
{ machineId, dateString, localId: "uuid",
  deliveries: [{ deliveryBoyId: "uuid", deliveryBoyName: "Suresh Patil", billsPrinted: 12, cylindersDelivered: 11 }],
  cylinderStock: { kg5: { full, empty }, kg14: { full, empty }, kg19: { full, empty } },
  isFinalized: true, finalizedAt: Date | null, createdAt: Date, updatedAt: Date, syncedAt: Date }
```

**`deliveryboys`**: managed in this app's Settings. Deactivated, never deleted.
```js
{ _id: "uuid", machineId, name: "Suresh Patil", isActive: true, createdAt: Date, updatedAt: Date, syncedAt: Date }
```

**`products`**: the adaptive catalog (ghost prices). Unique index: `{machineId, nameKey}`.
```js
{ machineId, name: "Regulator", nameKey: "regulator", lastUsedPrice: 350.0, localId: "uuid",
  createdAt: Date, updatedAt: Date, syncedAt: Date }
```

**Rules for the dashboard:**
- Treat these collections as **written by the PCs**. If the dashboard edits a document, the PC will overwrite it the next time that row changes locally.
- Totals exclude `status: "DELETED"`.
- A day is closed when its `dayendlogs.isFinalized` is true.

## Keyboard flow

| Key | Where | Action |
| --- | --- | --- |
| `F2` | anywhere | Switch Cash In / Cash Out (goes back to Cash In after each save) |
| `F3` | anywhere | Tick or untick **Other product**: Cash In without a consumer number |
| type, `↑`/`↓`, `Tab`/`Enter` | Product | Autocomplete from the catalog, then go to Consumer No (Cash In) or Price |
| `Esc` | Product | Keep the typed text as a **new item** |
| `Enter`/`Tab` | Consumer No | Go to Price. Blocked until it's exactly 6 digits (or empty with Other product). |
| `Enter` | Price | Save. Empty Price uses the grey ghost price. |
| `Esc` | Consumer No / Price | Go back one box |
| `Enter` / `Esc` | inline row edit | Save / cancel |
| `Enter` | Day End count cell | Next delivery boy, same column |
| `Ctrl+S` | Day End | Save progress |

## Development

Requirements: Node 20+, Rust (`rustup`).

```bash
npm install
npm run app:dev          # the real desktop window with hot reload
```

UI work in a normal browser, using the same Rust backend over HTTP:
```bash
npm run bridge           # backend on :1430 (data in ./.dev-data; set HPCL_MONGODB_URI to test sync)
npm run dev              # UI on http://localhost:1420
```

Tests: `cargo test --manifest-path src-tauri/Cargo.toml --lib` (business rules and pruning), and `npm run typecheck`.

## Building the Windows installer

**On a Windows PC** (simplest), install Rust, Node, and the "Desktop development with C++" build tools, then run:
```bash
npm install
npm run app:build
```
The output is `src-tauri\target\release\bundle\nsis\HPCL Operator_<version>_x64-setup.exe` (plus an `.msi`).

**From a Mac** (cross-compile, NSIS installer only):
```bash
brew install nsis llvm lld
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
PATH="/opt/homebrew/opt/llvm/bin:$PATH" npx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis
```
The output is `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/HPCL Operator_<version>_x64-setup.exe`.

**Windows 10 or 11 (64-bit) is required.** Windows 7, 8 and 8.1 can't run it: Microsoft's WebView2 (the app's screen) and current Rust no longer support them. On those versions the installer now stops with a clear message instead of the "PackageIdFromFullName could not be located in KERNEL32.dll" error.

The installer needs Microsoft WebView2. It's built into Windows 10 and 11, and the installer downloads it if it's missing. The app isn't code-signed, so Windows SmartScreen shows "Windows protected your PC" on first install: click **More info → Run anyway**. Signing needs a code-signing certificate.

## Layout

```
src/                 React UI (Vite + Tailwind v4): pages/, components/, lib/api.ts (backend calls)
src-tauri/src/
  cloud_config.rs    Cloud settings (Settings page / cloud.json), connection string handling
  store.rs           Business rules over SQLite (entries, edit/delete audit, day end, delivery boys)
  cloud.rs           Sync worker, cloud indexes, restore, history-from-cloud, pruning
  core.rs            Machine ID, shared state, the single `call` command dispatcher
  db.rs              SQLite schema + migrations      time.rs   business day (IST) + retention
  bin/dev_bridge.rs  Dev only: the backend over HTTP for browser testing
legacy-web/          Previous Next.js web version (not used)
```
