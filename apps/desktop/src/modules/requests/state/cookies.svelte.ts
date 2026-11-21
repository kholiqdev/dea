// The cookie manager's data: the jar of one environment, read again after
// every change and after every send.

import type { CookieInfo } from "$lib/ipc/bindings/CookieInfo";
import type { CookieKey } from "$lib/ipc/bindings/CookieKey";
import {
  deleteCookie,
  deleteDomainCookies,
  listCookies,
  saveCookie,
} from "$lib/ipc/cookies";

export function keyOf(cookie: CookieInfo): CookieKey {
  return { domain: cookie.domain, path: cookie.path, name: cookie.name };
}

export class CookiesState {
  environment = $state<string | null>(null);
  cookies = $state.raw<CookieInfo[]>([]);
  /** The domain shown; null when none is chosen. */
  chosen = $state<string | null>(null);
  loaded = $state(false);
  // Drops an answer that a later load has overtaken.
  #loads = 0;

  constructor(environment: string | null) {
    this.environment = environment;
  }

  /** The distinct domains of the cookies, sorted. */
  domains = $derived(
    this.cookies
      .map((c) => c.domain)
      .filter((d, i, all) => all.indexOf(d) === i)
      .sort((a, b) => a.localeCompare(b)),
  );

  /** The domain shown: the chosen one, else the first. */
  domain = $derived(
    this.chosen !== null && this.domains.includes(this.chosen)
      ? this.chosen
      : (this.domains[0] ?? null),
  );

  /** The cookies of the domain shown, by name. */
  rows = $derived(
    this.cookies
      .filter((c) => c.domain === this.domain)
      .sort((a, b) => a.name.localeCompare(b.name)),
  );

  async load(): Promise<void> {
    const load = ++this.#loads;
    const cookies = await listCookies(this.environment);
    if (load !== this.#loads) return;
    this.cookies = cookies;
    this.loaded = true;
  }

  /**
   * Switches jar. Nothing of the old one stays to be shown under the new
   * name, also when the new one cannot be read.
   */
  async chooseEnvironment(environment: string | null): Promise<void> {
    this.environment = environment;
    this.chosen = null;
    this.cookies = [];
    this.loaded = false;
    await this.load();
  }

  // The mutations do not read the jar again, so a failed read cannot hide
  // that they worked. They take the environment the caller meant.
  save(
    original: CookieKey,
    cookie: CookieInfo,
    environment = this.environment,
  ): Promise<void> {
    return saveCookie(environment, original, cookie);
  }

  remove(cookie: CookieInfo, environment = this.environment): Promise<void> {
    return deleteCookie(environment, keyOf(cookie));
  }

  /** Puts a deleted cookie back into the jar it was deleted from. */
  restore(cookie: CookieInfo, environment: string | null): Promise<void> {
    return this.save(keyOf(cookie), cookie, environment);
  }

  removeDomain(domain: string, environment = this.environment): Promise<void> {
    return deleteDomainCookies(environment, domain).then(() => undefined);
  }
}
