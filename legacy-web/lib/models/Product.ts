import { Schema, type InferSchemaType } from "mongoose";
import { defineModel } from "./define";

/** Case-insensitive collation so "Regulator" and "regulator" are the same product. */
export const PRODUCT_COLLATION = { locale: "en", strength: 2 } as const;

const ProductSchema = new Schema(
  {
    name: { type: String, required: true, trim: true },
    lastUsedPrice: { type: Number, required: true, min: 0 },
  },
  { timestamps: true }
);

ProductSchema.index({ name: 1 }, { unique: true, collation: PRODUCT_COLLATION });

export type ProductDoc = InferSchemaType<typeof ProductSchema>;

export const Product = defineModel<ProductDoc>("Product", ProductSchema);
