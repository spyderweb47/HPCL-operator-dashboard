# HPCL Dealership — Operator (Desktop)

A Windows desktop app (Tauri 2) for the office operator: the daily cash register, day end and stock, history, and settings.
It works **fully offline**. Every entry is saved on the PC first, then uploaded to the cloud MongoDB
in real time whenever the internet is available.

- **No login.** Each installation is identified by its **Machine ID** (shown in Settings). Every record in the cloud carries it.
- **Same UI** as the web version: same keyboard flow, same rules (6-digit consumer number, Other product, ghost price, soft delete with audit, finalize lock).
- **Several PCs, one office.** Every PC set to the same cloud database shares one register, product catalog, delivery boys and day end, updated within a few seconds. See [Several PCs in one office](#several-pcs-in-one-office).
- **Payment type** on every entry: Cash by default, or tick **Online** / **System**. Net Cash counts cash only (what should be in the drawer); the Cash In / Cash Out cards show the split on days with online or system entries.
- The previous Next.js web version is kept in `legacy-web/` for reference and isn't used by this app.

## How offline and sync work

```
 Operator ──► React UI ──invoke("call")──► Rust backend ──► SQLite on this PC  (always, instantly)
                                               │
                                               └─ sync worker ──► MongoDB in the cloud  (when online)
```

| Situation | What happens |
| --- | --- |
| Internet on | Each save wakes the sync worker; the entry is in the cloud within about 0.3 s. Every 4 s it also downloads what the office's other PCs changed. |
| Internet off | Saving, editing, day end and history keep working from the local file. Changes queue up; the header shows `Offline · N waiting`, and each waiting row has a small upload icon. |
| Internet back | The worker retries every 5 → 30 s, and also right away after any new entry. It uploads the queue in batches of 200 with 8 uploads in parallel. |
| Fresh install, or a new PC in the office | On first connect it downloads the office's delivery boys, product catalog and the last 180 days from the cloud. (The Machine ID comes from the Windows machine GUID, so a reinstall keeps its ID.) |
| Looking back in time | History reads the last **180 days** from this PC, so it works offline. Older days load from the cloud when online. |

**Keeping the PC light.** Once a day is older than 180 days *and* already in the cloud, it's removed from the PC (checked every 6 hours) and the space is returned to Windows. Unsynced rows are never removed. If cloud sync isn't set up at all, nothing is ever removed.
For scale: 200 entries a day for 180 days is about 36,000 rows, roughly 15–20 MB.

**Why sync can't lose or duplicate data.**
- Every row has `version`, which goes up on each local change, and `synced_version`, the last version the cloud confirmed.
- Upload is a full-document upsert keyed by a stable ID, so retries and crashes in the middle of a sync are harmless.
- A row edited while it's uploading keeps its newer version and goes up in the next round.

The local file is `%APPDATA%\com.hpcl.operator\operator.db`: SQLite in WAL mode. Only one copy of the app can run at a time; opening it again focuses the existing window.

## Several PCs in one office

All PCs that use the **same cloud database** are one office. They work on one shared register:
entries, edits and deletes, the product catalog (ghost prices), delivery boys and the Day End counts.

**Adding a PC:** install HPCL Operator on it, open **Settings → Cloud Database** and enter the same details
as on the first PC. Within a few seconds it has the office's last 180 days. **Settings → Office PCs** lists every
PC and when it was last online. All PCs need version 1.5 or newer; the list warns about older ones.

| Situation | What happens |
| --- | --- |
| Entry made on one PC | Appears on the others within about 5 s, without reloading. When the register has entries from more than one PC, each row shows the PC's name under the time. |
| Same entry edited on two PCs | The later edit wins on every PC (by the time of the edit). The audit trail is kept. |
| Day End filled on two PCs | Merged **count by count**: each box keeps the number typed last, wherever it was typed, so two people can fill different rows at the same time. A form open on one PC updates with the other PC's numbers as they arrive. |
| Finalize on one PC | Locks the day on every PC. Entries that a PC made offline before it heard about the finalize are still kept and counted. |
| One PC offline | It keeps working from its own file. When it reconnects, it uploads its changes and downloads the others'. |
| Same delivery boy added on two PCs while offline | Both show up; deactivate one in Settings. |

How it works: each upload is stamped by the database server (`syncedAt`), and each PC downloads everything
stamped after its last look (with a 30 s overlap), so different PC clocks never cause a missed change.

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
`syncedAt` is set by the database server when the document was last written (v1.5+; the uploading PC's clock before).

**`machines`**: one document per installed PC, updated on every sync and at least every 5 minutes while online.
```js
{ _id: "90D7-C168-32F2-9F5D", deviceName: "OFFICE-PC", os: "windows", appVersion: "1.0.0",
  retentionDays: 180, pendingCount: 0, firstSeenAt: Date, lastSeenAt: Date }
```

**`transactions`**: `_id` is the UUID made on the PC. Never hard-deleted.
```js
{ _id: "uuid", machineId, dateString: "2026-10-30", type: "CASH_IN" | "CASH_OUT",
  productName: "Regulator", price: 350.0, consumerNo: "123456" | null, isOtherProduct: false,
  paymentMode: "CASH" | "ONLINE" | "SYSTEM",   // missing on entries from before v1.4 = "CASH"
  status: "ACTIVE" | "EDITED" | "DELETED", isDayFinalized: false,
  auditLog: [{ action: "CREATED"|"UPDATED"|"DELETED", previousProductName, newProductName, previousPrice, newPrice,
               previousConsumerNo, newConsumerNo, previousPaymentMode, newPaymentMode, timestamp: Date }],
  createdAt: Date, updatedAt: Date, syncedAt: Date,
  updatedBy: "machine id" }   // machineId = the PC that made the entry; updatedBy = the PC that changed it last
```
Indexes: `{machineId, dateString, createdAt}`, `{dateString}`, `{consumerNo}`, `{syncedAt, _id}`.

**`dayendlogs`**: one copy per PC per day (unique index `{machineId, dateString}`). The PCs merge their copies
count by count, so once synced every copy of a day holds the same numbers; read any one, or merge by the newest `stamps.at` per cell.
```js
{ machineId, dateString, localId: "uuid",
  deliveries: [{ deliveryBoyId: "uuid", deliveryBoyName: "Suresh Patil", billsPrinted: 12, cylindersDelivered: 11 }],
  cylinderStock: { kg5: { full, empty }, kg14: { full, empty }, kg19: { full, empty } },
  isFinalized: true, finalizedAt: Date | null, createdAt: Date, updatedAt: Date, syncedAt: Date,
  stamps: [{ cell: "bills:<deliveryBoyId>" | "cyl:<deliveryBoyId>" | "kg14.full" | …, at: Date }] | null }  // when each count was typed (v1.5+)
```

**`deliveryboys`**: managed in this app's Settings. Deactivated, never deleted.
```js
{ _id: "uuid", machineId, updatedBy, name: "Suresh Patil", isActive: true, createdAt: Date, updatedAt: Date, syncedAt: Date }
```

**`products`**: the adaptive catalog (ghost prices), one copy per PC. Unique index: `{machineId, nameKey}`. The newest `updatedAt` per `nameKey` is the office's current price.
```js
{ machineId, name: "Regulator", nameKey: "regulator", lastUsedPrice: 350.0, localId: "uuid",
  createdAt: Date, updatedAt: Date, syncedAt: Date }
```

**Rules for the dashboard:**
- Treat these collections as **written by the PCs**. If the dashboard edits a document, the PC will overwrite it the next time that row changes locally.
- Totals exclude `status: "DELETED"`.
- A day is closed when any of its `dayendlogs` copies has `isFinalized: true`.
- An office's totals are over all `machineId`s in the database.

## Keyboard flow

| Key | Where | Action |
| --- | --- | --- |
| `F2` | anywhere | Switch Cash In / Cash Out (goes back to Cash In after each save) |
| `F3` | anywhere | Tick or untick **Other product**: Cash In without a consumer number (the box then takes any text) |
| `F4` / `F6` | anywhere | Tick or untick **Online** / **System** payment. Neither ticked = Cash; goes back to Cash after each save. |
| type, `↑`/`↓`, `Tab`/`Enter` | Product | Autocomplete from the catalog, then go to Consumer No (Cash In) or Price |
| `Esc` | Product | Keep the typed text as a **new item** |
| `Enter`/`Tab` | Consumer No | Go to Price. Blocked until it's exactly 6 digits (or empty with Other product). |
| `Enter` | Price | Save. Empty Price uses the grey ghost price. |
| `Esc` | Consumer No / Price | Go back one box |
| `Enter` / `Esc` | inline row edit | Save / cancel |
| `Enter` | Day End count cell | Next delivery boy, same column |
| `Ctrl+S` | Day End | Save now (it also saves by itself a moment after each change) |

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

Two office PCs on one computer: run a second bridge and UI with their own data folder and identity, both with the same `HPCL_MONGODB_URI`/`HPCL_MONGODB_DB`:
```bash
HPCL_DATA_DIR=.dev-data-2 HPCL_MACHINE_ID=BBBB-0000-0000-0002 HPCL_DEVICE_NAME=COUNTER-2 HPCL_BRIDGE_PORT=1431 npm run bridge
HPCL_BRIDGE_PORT=1431 npx vite --port 1421
```

Tests: `cargo test --manifest-path src-tauri/Cargo.toml --lib` (business rules, two-PC merging, pruning, upgrades), and `npm run typecheck`.

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

The installer needs Microsoft WebView2. It's built into Windows 10 and 11, and the installer downloads it if it's missing. The app isn't code-signed, so Windows SmartScreen shows "Windows protected your PC" on first install: click **More info → Run anyway**. Signing needs a code-signing certificate.

## Windows 7, 8 and 8.1

Each release has three installers:

| Installer | For | Size |
| --- | --- | --- |
| `HPCL-Operator-<version>-x64-setup.exe` | Windows 10 and 11 | about 3 MB; uses the WebView2 already in Windows |
| `HPCL-Operator-<version>-win7-x64-setup.exe` | Windows 7 SP1, 8 and 8.1, **64-bit** | about 150 MB; WebView2 109 is inside, so no internet is needed to install |
| `HPCL-Operator-<version>-win7-x86-setup.exe` | Windows 7 SP1, 8 and 8.1, **32-bit** | about 145 MB; same, with the 32-bit app and WebView2 |

The Windows 10/11 installer stops on older Windows with a message pointing to the win7 one. (Microsoft's current WebView2
setup can't run there; it fails with "PackageIdFromFullName could not be located in KERNEL32.dll".)

How the win7 build differs:
- **Rust target `x86_64-win7-windows-msvc`**, or `i686-win7-windows-msvc` for 32-bit (nightly + `-Zbuild-std`). Normal Rust stopped supporting Windows 7 in 1.78.
- **C runtime linked into the exe** (`+crt-static`), so it doesn't depend on the Windows 7 Universal CRT update.
- **A few Windows 8+ imports, delay-loaded.** These come from the WebView2 loader and the `windows` crates: `combase.dll`, `EventSetInformation` and `RoOriginateErrorW`. On Windows 7, `src-tauri/src/win7.rs` uses `ole32.dll` for the first and harmless stand-ins for the other two.
- **WebView2 109 bundled.** It's the last version that runs on Windows 7/8/8.1 (Chromium 109), set in `src-tauri/tauri.win7.conf.json` (32-bit: `tauri.win7-x86.conf.json`).
  The UI is built to run on Chromium 109 (`vite.config.ts`: newer CSS colors get plain fallbacks).

**Building it on a Mac:**
1. One-time setup:
   ```bash
   rustup toolchain install nightly --component rust-src
   brew install cabextract
   ```
2. Download `Microsoft.WebView2.FixedVersionRuntime.109.0.1518.78.x64.cab` (`.x86.cab` for 32-bit). Microsoft no longer lists it; a copy is at
   github.com/westinyang/WebView2RuntimeArchive (release 109.0.1518.78).
3. Check that it's Microsoft-signed (`osslsigncode verify`). The SHA-256 of the copies used here:
   - x64: `7622281cf83de1a35e3a471f432f7a897d65f0a7d3975df08512b7b253dd45c7`
   - x86: `c507e0df03fe941f6669b74faf713545708653d568d1dce1e683cbe707382253`
4. Extract it with `cabextract -d src-tauri <cab>` (the folder is git-ignored), then run:
   ```bash
   scripts/build-win7.sh        # 64-bit
   scripts/build-win7.sh x86    # 32-bit
   ```

Windows 7 itself no longer gets security updates from Microsoft, and WebView2 109 doesn't either. Use it only for this app, and keep the PC's antivirus up to date.

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
  win7.rs            Windows 7 only: stand-ins for Windows 8+ functions (see "Windows 7, 8 and 8.1")
scripts/build-win7.sh  Builds the Windows 7/8/8.1 installer
legacy-web/          Previous Next.js web version (not used)
```
