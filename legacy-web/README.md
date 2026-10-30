# HPCL Dealership — Operator Portal

Keyboard-first operator app for the daily cash register, day-end reconciliation, and history.
It shares a MongoDB database with the separate Owner/Admin Dashboard.

**Stack:** Next.js 16 (App Router, Server Actions, Cache Components) · Tailwind CSS v4 · Mongoose 9 · Lucide · JWT session cookie (`jose`) + `bcryptjs`.

## Setup

```bash
cp .env.example .env.local      # set MONGODB_URI and AUTH_SECRET (openssl rand -base64 48)
npm install
# Users normally come from the Owner Dashboard. For local dev, create one:
node --env-file=.env.local scripts/create-user.mjs ravi secret123 "Ravi Kumar"
# Delivery boys also come from the Owner Dashboard. For local dev:
node --env-file=.env.local scripts/create-delivery-boy.mjs "Suresh Patil" "Mahesh Yadav"
npm run dev                     # http://localhost:3000
```

## Keyboard flow (`/dashboard`)

| Key | Where | Action |
| --- | --- | --- |
| `F3` | anywhere | Tick or untick **Other product**: Cash In without a consumer number (unticks after each save) |
| type | Product | Live autocomplete from the product catalog |
| `↑` / `↓` | Product | Move through suggestions |
| `Tab` / `Enter` | Product | Pick the highlighted product and move on: to Consumer No for Cash In, or straight to Price for Cash Out or Other product. If suggestions haven't loaded yet, it waits for them, so fast typing still autocompletes. |
| `Esc` | Product | Close suggestions and keep the typed text as a **new item** |
| `Enter` (Price empty) | Price | Save using the grey ghost price (`lastUsedPrice`) |
| `Enter` (Price typed) | Price | Save with that price and update the product's `lastUsedPrice` |
| `Enter` / `Tab` | Consumer No | Go to Price. Blocked until it's exactly 6 digits (or empty, if Other product is ticked). |
| `Esc` | Consumer No / Price | Go back one box |
| `F2` | anywhere | Switch between Cash In and Cash Out (goes back to Cash In after each save) |
| `Enter` / `Esc` | inline row edit | Save / cancel |
| `Enter` | Day End count cell | Next delivery boy, same column |
| `Ctrl/⌘+S` | Day End | Save progress |

## Layout

```
lib/models/        User, DeliveryBoy (both read-only), Product, Transaction, DayEndLog (Mongoose)
lib/auth.ts        getSession / requireSession / requireActiveUser (re-checks isActive in the DB on every write)
lib/session.ts     JWT sign/verify (also used by proxy.ts)
lib/queries.ts     Read helpers + DTO mapping + totals
app/actions/       Server Actions: auth, transactions (create/edit/soft-delete), dayEnd (save/finalize)
app/api/products/  GET autocomplete (a route handler because server actions run one at a time)
components/        EntryBar, RegisterTable, TotalsBar, DayEndForm, DayEndViews, HistoryDatePicker
proxy.ts           Redirects for logged-in and logged-out users (Next 16's name for middleware)
```

## Data rules

- **Consumer No** is **exactly 6 digits**. It's required for every Cash In unless **Other product** is ticked; with Other product ticked it's optional, but anything typed must still be 6 digits. Cash Out never stores one. It's stored as a string of digits (`consumerNo`, so leading zeros are kept) together with `isOtherProduct`, and the server enforces the same rules. Consumer No can be edited inline (it can't be blanked on a normal Cash In), and the change is recorded in the audit log as `previousConsumerNo`/`newConsumerNo`.
- **No hard deletes.** A delete sets `status: 'DELETED'` and adds a `DELETED` audit entry. An edit sets `status: 'EDITED'` and adds an `UPDATED` entry with old and new values. Every row also starts with a `CREATED` entry.
- Edits use optimistic concurrency: the update only applies if the row still has the values the operator saw.
- Only **today's** rows can be edited, and only while the day isn't finalized. The server checks this on every action.
- **Finalize** saves the day-end form, sets `DayEndLog.isFinalized`, and sets `isDayFinalized: true` on all of that day's transactions. After that, new entries for the day are rejected.
- **Delivery boys** come from the Owner Dashboard roster. Day End lists every active delivery boy automatically. Operators can only enter counts; they can't add or rename anyone. The server checks every row against the roster, and the name is read from the database. If someone is deactivated during the day, their saved counts are kept for that day.
- `dateString` uses the business timezone `NEXT_PUBLIC_APP_TIMEZONE` (default `Asia/Kolkata`), not the server's timezone.

## Notes for the Owner Dashboard (shared DB)

- Collections: `users`, `deliveryboys`, `products`, `transactions`, `dayendlogs`.
- `deliveryboys` documents need `name` (unique) and `isActive` (deactivate instead of deleting). Each `DayEndLog.deliveries[]` entry stores `deliveryBoyId` plus a `deliveryBoyName` snapshot.
- `products.name` has a **case-insensitive unique index** (collation `{locale:'en', strength:2}`), so "Regulator" and "regulator" are the same product.
- Passwords must be **bcrypt** hashes in `users.passwordHash`.
- `Transaction` and `DayEndLog` also have Mongoose `createdAt`/`updatedAt` timestamps. The register is sorted by `createdAt`.
