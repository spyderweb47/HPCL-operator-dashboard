import { Schema, type InferSchemaType } from "mongoose";
import { defineModel } from "./define";

export const TRANSACTION_TYPES = ["CASH_IN", "CASH_OUT"] as const;
export type TransactionType = (typeof TRANSACTION_TYPES)[number];

const AuditEntrySchema = new Schema(
  {
    action: { type: String, enum: ["CREATED", "UPDATED", "DELETED"], required: true },
    previousProductName: { type: String },
    newProductName: { type: String },
    previousPrice: { type: Number },
    newPrice: { type: Number },
    previousConsumerNo: { type: String },
    newConsumerNo: { type: String },
    changedBy: { type: Schema.Types.ObjectId, ref: "User" },
    changedByName: { type: String },
    timestamp: { type: Date, default: Date.now },
  },
  { _id: false }
);

const TransactionSchema = new Schema(
  {
    dateString: { type: String, required: true, index: true },
    type: { type: String, enum: TRANSACTION_TYPES, default: "CASH_IN" },
    productName: { type: String, required: true, trim: true },
    price: { type: Number, required: true, min: 0 },
    // Required for Cash In unless isOtherProduct; always null for Cash Out. Stored as a
    // string so leading zeros survive.
    consumerNo: { type: String, trim: true, default: null },
    isOtherProduct: { type: Boolean, default: false },
    createdBy: { type: Schema.Types.ObjectId, ref: "User" },
    createdByName: { type: String },
    status: { type: String, enum: ["ACTIVE", "EDITED", "DELETED"], default: "ACTIVE" },
    isDayFinalized: { type: Boolean, default: false },
    auditLog: { type: [AuditEntrySchema], default: [] },
  },
  { timestamps: true }
);

TransactionSchema.index({ dateString: 1, createdAt: -1 });
TransactionSchema.index({ consumerNo: 1 }, { sparse: true });

export type TransactionDoc = InferSchemaType<typeof TransactionSchema>;

export const Transaction = defineModel<TransactionDoc>("Transaction", TransactionSchema);
