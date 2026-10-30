import { NextResponse, type NextRequest } from "next/server";
import { getSession } from "@/lib/auth";
import { connectDB } from "@/lib/db";
import { Product } from "@/lib/models";

const escapeRegex = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/**
 * Autocomplete lookup. A route handler (not a server action) because server actions
 * are queued one-at-a-time on the client, which would make typing feel laggy.
 */
export async function GET(req: NextRequest) {
  if (!(await getSession())) return NextResponse.json({ error: "Unauthorized" }, { status: 401 });

  const q = (req.nextUrl.searchParams.get("q") ?? "").replace(/\s+/g, " ").trim().slice(0, 60);
  if (!q) return NextResponse.json({ products: [] });

  await connectDB();
  const pattern = escapeRegex(q);
  const LIMIT = 8;
  const prefix = await Product.find({ name: { $regex: `^${pattern}`, $options: "i" } })
    .sort({ updatedAt: -1 })
    .limit(LIMIT)
    .select("name lastUsedPrice")
    .lean();

  let rows = prefix;
  if (rows.length < LIMIT) {
    const contains = await Product.find({
      name: { $regex: pattern, $options: "i" },
      _id: { $nin: rows.map((r) => r._id) },
    })
      .sort({ updatedAt: -1 })
      .limit(LIMIT - rows.length)
      .select("name lastUsedPrice")
      .lean();
    rows = rows.concat(contains);
  }

  return NextResponse.json(
    { products: rows.map((p) => ({ name: p.name, lastUsedPrice: p.lastUsedPrice })) },
    { headers: { "Cache-Control": "no-store" } }
  );
}
