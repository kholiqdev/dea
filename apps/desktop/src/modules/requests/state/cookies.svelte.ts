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

  async chooseEnvironment(environment: string | null): Promise<void> {
    this.environment = environment;
    this.chosen = null;
    await this.load();
  }

  async save(original: CookieKey, cookie: CookieInfo): Promise<void> {
    await saveCookie(this.environment, original, cookie);
    await this.load();
  }

  async remove(cookie: CookieInfo): Promise<void> {
    await deleteCookie(this.environment, keyOf(cookie));
    await this.load();
  }

  /** Puts a deleted cookie back. */
  async restore(cookie: CookieInfo): Promise<void> {
    await this.save(keyOf(cookie), cookie);
  }

  async removeDomain(domain: string): Promise<void> {
    await deleteDomainCookies(this.environment, domain);
    await this.load();
  }
}
