#!/usr/bin/env node
/**
 * Dev helper: users are normally managed by the Owner Dashboard.
 * Usage: node --env-file=.env.local scripts/create-user.mjs <username> <password> "<Full Name>" [operator|admin]
 */
import bcrypt from "bcryptjs";
import mongoose from "mongoose";

const [username, password, name = username, role = "operator"] = process.argv.slice(2);
if (!username || !password) {
  console.error('Usage: node --env-file=.env.local scripts/create-user.mjs <username> <password> "<Full Name>" [operator|admin]');
  process.exit(1);
}
if (!process.env.MONGODB_URI) {
  console.error("MONGODB_URI is not set (did you pass --env-file=.env.local?)");
  process.exit(1);
}

await mongoose.connect(process.env.MONGODB_URI);
const users = mongoose.connection.collection("users");
const passwordHash = await bcrypt.hash(password, 10);
await users.updateOne(
  { username },
  { $set: { passwordHash, name, role, isActive: true, updatedAt: new Date() }, $setOnInsert: { createdAt: new Date() } },
  { upsert: true }
);
console.log(`User "${username}" (${role}) is ready.`);
await mongoose.disconnect();
