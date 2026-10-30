import "server-only";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { Types } from "mongoose";
import { connectDB } from "./db";
import { User } from "./models";
import { SESSION_COOKIE, verifySession, type SessionPayload } from "./session";

export async function getSession(): Promise<SessionPayload | null> {
  const store = await cookies();
  return verifySession(store.get(SESSION_COOKIE)?.value);
}

/** For server components: bounce to /login when there is no valid session. */
export async function requireSession(): Promise<SessionPayload> {
  const session = await getSession();
  if (!session) redirect("/login");
  return session;
}

export class AuthError extends Error {}

/**
 * For mutations: verifies the session AND that the account is still active in the
 * shared Users collection, so a deactivated operator can't keep writing with an old cookie.
 * Returns the identity stamped into transactions and audit entries.
 */
export async function requireActiveUser() {
  const session = await getSession();
  if (!session || !Types.ObjectId.isValid(session.userId)) throw new AuthError("Session expired. Please log in again.");
  await connectDB();
  const user = await User.findById(session.userId).select("name username isActive").lean();
  if (!user || user.isActive === false) throw new AuthError("Your account is inactive. Contact the owner.");
  return { id: new Types.ObjectId(session.userId), name: user.name || user.username };
}
