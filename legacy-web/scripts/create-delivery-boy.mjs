#!/usr/bin/env node
/**
 * Dev helper: delivery boys are normally managed by the Owner Dashboard.
 * Usage: node --env-file=.env.local scripts/create-delivery-boy.mjs "Suresh" ["Mahesh" ...]
 */
import mongoose from "mongoose";

const names = process.argv.slice(2).map((n) => n.trim()).filter(Boolean);
if (!names.length) {
  console.error('Usage: node --env-file=.env.local scripts/create-delivery-boy.mjs "Name" ["Name" ...]');
  process.exit(1);
}
if (!process.env.MONGODB_URI) {
  console.error("MONGODB_URI is not set (did you pass --env-file=.env.local?)");
  process.exit(1);
}

await mongoose.connect(process.env.MONGODB_URI);
const col = mongoose.connection.collection("deliveryboys");
for (const name of names) {
  await col.updateOne(
    { name },
    { $set: { isActive: true, updatedAt: new Date() }, $setOnInsert: { name, createdAt: new Date() } },
    { upsert: true }
  );
  console.log(`Delivery boy "${name}" is active.`);
}
await mongoose.disconnect();
