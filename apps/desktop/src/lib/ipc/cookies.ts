// The cookie jar of an environment of this window's project. `null` is the
// jar used when no environment is active.

import { invoke } from "@tauri-apps/api/core";
import type { CookieInfo } from "./bindings/CookieInfo";
import type { CookieKey } from "./bindings/CookieKey";

export function listCookies(environment: string | null): Promise<CookieInfo[]> {
  return invoke<CookieInfo[]>("list_cookies", { environment });
}

/** Replaces `original` with `cookie`, which may have another name, domain or path. */
export function saveCookie(
  environment: string | null,
  original: CookieKey,
  cookie: CookieInfo,
): Promise<void> {
  return invoke<void>("save_cookie", { environment, original, cookie });
}

export function deleteCookie(
  environment: string | null,
  key: CookieKey,
): Promise<void> {
  return invoke<void>("delete_cookie", { environment, key });
}

/** Deletes every cookie of `domain`; gives how many there were. */
export function deleteDomainCookies(
  environment: string | null,
  domain: string,
): Promise<number> {
  return invoke<number>("delete_domain_cookies", { environment, domain });
}
