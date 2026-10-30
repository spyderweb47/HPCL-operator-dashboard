import { SignJWT, jwtVerify } from "jose";

/** Kept free of DB/Node-only imports so proxy.ts can use it. */
export const SESSION_COOKIE = "hpcl_operator_session";
export const SESSION_TTL_SECONDS = 60 * 60 * 14; // one long shift

export type SessionPayload = {
  userId: string;
  username: string;
  name: string;
  role: "operator" | "admin";
};

function secretKey() {
  const secret = process.env.AUTH_SECRET;
  if (!secret || secret.length < 32) {
    throw new Error("AUTH_SECRET must be set and at least 32 characters long");
  }
  return new TextEncoder().encode(secret);
}

export async function signSession(payload: SessionPayload) {
  return new SignJWT({ ...payload })
    .setProtectedHeader({ alg: "HS256" })
    .setSubject(payload.userId)
    .setIssuedAt()
    .setExpirationTime(`${SESSION_TTL_SECONDS}s`)
    .sign(secretKey());
}

export async function verifySession(token: string | undefined): Promise<SessionPayload | null> {
  if (!token) return null;
  try {
    const { payload } = await jwtVerify(token, secretKey(), { algorithms: ["HS256"] });
    if (typeof payload.userId !== "string") return null;
    return {
      userId: payload.userId,
      username: String(payload.username ?? ""),
      name: String(payload.name ?? ""),
      role: payload.role === "admin" ? "admin" : "operator",
    };
  } catch {
    return null;
  }
}
