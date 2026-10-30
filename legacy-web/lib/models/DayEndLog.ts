import { Schema, type InferSchemaType } from "mongoose";
import { defineModel } from "./define";

const StockPairSchema = new Schema(
  {
    full: { type: Number, default: 0, min: 0 },
    empty: { type: Number, default: 0, min: 0 },
  },
  { _id: false }
);

const DeliverySchema = new Schema(
  {
    deliveryBoyId: { type: Schema.Types.ObjectId, ref: "DeliveryBoy" },
    // Name snapshot at save time, so history still reads correctly if the owner renames someone.
    deliveryBoyName: { type: String, required: true, trim: true },
    billsPrinted: { type: Number, default: 0, min: 0 },
    cylindersDelivered: { type: Number, default: 0, min: 0 },
  },
  { _id: false }
);

const DayEndLogSchema = new Schema(
  {
    dateString: { type: String, required: true, unique: true },
    isFinalized: { type: Boolean, default: false },
    finalizedBy: { type: Schema.Types.ObjectId, ref: "User" },
    finalizedAt: { type: Date },
    deliveries: { type: [DeliverySchema], default: [] },
    cylinderStock: {
      kg5: { type: StockPairSchema, default: () => ({}) },
      kg14: { type: StockPairSchema, default: () => ({}) },
      kg19: { type: StockPairSchema, default: () => ({}) },
    },
  },
  { timestamps: true }
);

export type DayEndLogDoc = InferSchemaType<typeof DayEndLogSchema>;

export const DayEndLog = defineModel<DayEndLogDoc>("DayEndLog", DayEndLogSchema);
