import { Schema, type InferSchemaType } from "mongoose";
import { defineModel } from "./define";

/**
 * Read-only in this app — users are created and managed by the Owner Dashboard.
 */
const UserSchema = new Schema(
  {
    username: { type: String, unique: true, required: true, trim: true },
    passwordHash: { type: String, required: true },
    name: { type: String },
    role: { type: String, enum: ["operator", "admin"], default: "operator" },
    isActive: { type: Boolean, default: true },
  },
  { timestamps: true }
);

export type UserDoc = InferSchemaType<typeof UserSchema>;

export const User = defineModel<UserDoc>("User", UserSchema);
