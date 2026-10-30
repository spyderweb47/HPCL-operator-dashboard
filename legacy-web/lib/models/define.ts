import mongoose, { type Model, type Schema } from "mongoose";

/**
 * Re-registers the model each time its module is evaluated. In production that's once;
 * in dev it means hot reloads pick up schema edits instead of silently keeping the old
 * schema (which strips any newly added fields on save).
 */
export function defineModel<T>(name: string, schema: Schema): Model<T> {
  if (mongoose.models[name]) mongoose.deleteModel(name);
  return mongoose.model(name, schema) as unknown as Model<T>;
}
