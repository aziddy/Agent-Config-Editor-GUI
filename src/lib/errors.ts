import { isAppError } from "./types";

/** Human-readable message for anything thrown by an RPC call. */
export function describeError(err: unknown): string {
  if (isAppError(err)) {
    return err.path ? `${err.message} (${err.path})` : err.message;
  }
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return "Unknown error";
}
