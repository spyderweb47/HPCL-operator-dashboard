import { Schema, type InferSchemaType } from "mongoose";
import { defineModel } from "./define";

/**
 * Read-only in this app — delivery boys are added/deactivated from the Owner Dashboard.
 */
const DeliveryBoySchema = new Schema(
  {
    name: { type: String, required: true, trim: true, unique: true },
    isActive: { type: Boolean, default: true },
  },
  { timestamps: true }
);

export type DeliveryBoyDoc = InferSchemaType<typeof DeliveryBoySchema>;

export const DeliveryBoy = defineModel<DeliveryBoyDoc>("DeliveryBoy", DeliveryBoySchema);
