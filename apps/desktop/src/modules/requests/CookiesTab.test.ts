import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import type { CookieInfo } from "$lib/ipc/bindings/CookieInfo";
import type { FakeProject } from "./fake-project.svelte";
import CookiesTab from "./CookiesTab.svelte";
import { manifest } from "./index";
import manageCookies from "./intents/manage-cookies";
import { setUpRequestTabs, type FakeShell } from "./request-tab-support.svelte";
import { fakeCookies, fakeRequestsIpc, type RequestsIpc } from "./test-support";

let shell: FakeShell;
let project: FakeProject;

beforeEach(() => {
  ({ shell, project } = setUpRequestTabs());
});

function cookie(
  name: string,
  domain: string,
  over: Partial<CookieInfo> = {},
): CookieInfo {
  return {
    name,
    value: `${name}-value`,
    domain,
    path: "/",
    expires: null,
    secure: false,
    httpOnly: false,
    hostOnly: true,
    ...over,
  };
}

function cookiesTab(environment: string | null) {
  const id = shell.openTab("requests.cookies", "Cookies", { environment });
  return render(CookiesTab, { props: { tab: shell.tab(id) } });
}

const calls = (ipc: RequestsIpc, cmd: string) =>
  ipc.calls.filter(([c]) => c === cmd).map(([, args]) => args);

const rowNames = (table: HTMLElement) =>
  within(table)
    .getAllByRole("row")
    .slice(1)
    .map((r) => within(r).getAllByRole("cell")[0]?.textContent);

test("lists_cookies_of_the_chosen_environment_and_domain", async () => {
  fakeRequestsIpc({
    cookies: fakeCookies({
      staging: [
        cookie("session", "api.test"),
        cookie("csrf", "api.test"),
        cookie("x", "other.test"),
      ],
    }),
  });
  cookiesTab("staging");
  const table = await screen.findByRole("table", { name: "Cookies" });
  expect(rowNames(table)).toEqual(["csrf", "session"]);
  expect(
    screen.getByText("Cookies are kept per environment and domain"),
  ).toBeInTheDocument();
});

test("the_domain_menu_changes_the_rows", async () => {
  fakeRequestsIpc({
    cookies: fakeCookies({
      staging: [cookie("a", "api.test"), cookie("x", "other.test")],
    }),
  });
  cookiesTab("staging");
  await screen.findByRole("table", { name: "Cookies" });
  await userEvent.click(screen.getByRole("button", { name: /api\.test/ }));
  await userEvent.click(
    screen.getByRole("menuitemradio", { name: "other.test" }),
  );
  expect(rowNames(screen.getByRole("table", { name: "Cookies" }))).toEqual([
    "x",
  ]);
});

test("changing_environment_loads_its_cookies", async () => {
  const ipc = fakeRequestsIpc({
    cookies: fakeCookies({
      staging: [cookie("a", "api.test")],
      local: [cookie("b", "local.test")],
    }),
  });
  cookiesTab("staging");
  await screen.findByRole("table", { name: "Cookies" });
  await userEvent.click(screen.getByRole("button", { name: /staging/ }));
  await userEvent.click(screen.getByRole("menuitemradio", { name: "local" }));
  await waitFor(() =>
    expect(rowNames(screen.getByRole("table", { name: "Cookies" }))).toEqual([
      "b",
    ]),
  );
  expect(ipc.cookieCalls).toEqual(["staging", "local"]);
});

test("an_empty_jar_says_so", async () => {
  fakeRequestsIpc({ cookies: fakeCookies({}) });
  cookiesTab("staging");
  expect(
    await screen.findByText(
      "No cookies in staging yet. Cookies that responses set appear here.",
    ),
  ).toBeInTheDocument();
});

test("expires_shows_session_or_a_date", async () => {
  fakeRequestsIpc({
    cookies: fakeCookies({
      staging: [
        cookie("a", "api.test"),
        cookie("b", "api.test", { expires: "2026-11-04T12:00:00Z" }),
      ],
    }),
  });
  cookiesTab("staging");
  const table = await screen.findByRole("table", { name: "Cookies" });
  const cells = within(table)
    .getAllByRole("row")
    .slice(1)
    .map((r) => within(r).getAllByRole("cell")[2]?.textContent);
  expect(cells).toEqual(["Session", "4 Nov 2026"]);
});

test("production_is_marked_in_the_environment_menu", async () => {
  project.info = {
    ...project.info!,
    environments: [
      { name: "local", production: false },
      { name: "live", production: true },
    ],
  };
  fakeRequestsIpc({ cookies: fakeCookies({}) });
  cookiesTab("local");
  await screen.findByText(/No cookies in local/);
  await userEvent.click(screen.getByRole("button", { name: /local/ }));
  const live = screen.getByRole("menuitemradio", { name: /live/ });
  expect(live).toHaveTextContent("production");
});

test("editing_saves_and_reloads", async () => {
  const ipc = fakeRequestsIpc({
    cookies: fakeCookies({
      staging: [cookie("session", "api.test", { value: "old" })],
    }),
  });
  cookiesTab("staging");
  await userEvent.click(
    await screen.findByRole("button", { name: "Edit cookie session" }),
  );
  const dialog = screen.getByRole("dialog", { name: "Edit cookie session" });
  const value = within(dialog).getByRole("textbox", { name: "Value" });
  await userEvent.clear(value);
  await userEvent.type(value, "new");
  await userEvent.click(
    within(dialog).getByRole("checkbox", { name: "Secure" }),
  );
  await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
  await waitFor(() => expect(calls(ipc, "save_cookie")).toHaveLength(1));
  expect(calls(ipc, "save_cookie")[0]).toMatchObject({
    environment: "staging",
    original: { domain: "api.test", path: "/", name: "session" },
    cookie: { name: "session", value: "new", secure: true, expires: null },
  });
  await waitFor(() => expect(ipc.cookieCalls).toHaveLength(2));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

test("a_refused_edit_stays_open_with_the_message", async () => {
  const ipc = fakeRequestsIpc({
    cookies: fakeCookies({ staging: [cookie("session", "api.test")] }),
  });
  ipc.failures.set("save_cookie", {
    kind: "invalid",
    message: "the path must start with /",
  });
  cookiesTab("staging");
  await userEvent.click(
    await screen.findByRole("button", { name: "Edit cookie session" }),
  );
  const dialog = screen.getByRole("dialog");
  await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
  expect(
    await within(dialog).findByText("The path must start with /"),
  ).toBeInTheDocument();
});

test("a_session_cookie_has_no_expiry", async () => {
  const ipc = fakeRequestsIpc({
    cookies: fakeCookies({
      staging: [cookie("a", "api.test", { expires: "2026-11-04T12:00:00Z" })],
    }),
  });
  cookiesTab("staging");
  await userEvent.click(
    await screen.findByRole("button", { name: "Edit cookie a" }),
  );
  const dialog = screen.getByRole("dialog");
  await userEvent.click(
    within(dialog).getByRole("checkbox", { name: "Session cookie" }),
  );
  await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
  await waitFor(() => expect(calls(ipc, "save_cookie")).toHaveLength(1));
  expect(calls(ipc, "save_cookie")[0]).toMatchObject({
    cookie: { expires: null },
  });
});

test("deleting_one_offers_undo", async () => {
  const ipc = fakeRequestsIpc({
    cookies: fakeCookies({ staging: [cookie("session", "api.test")] }),
  });
  cookiesTab("staging");
  await userEvent.click(
    await screen.findByRole("button", { name: "Delete cookie session" }),
  );
  await waitFor(() => expect(calls(ipc, "delete_cookie")).toHaveLength(1));
  expect(calls(ipc, "delete_cookie")[0]).toEqual({
    environment: "staging",
    key: { domain: "api.test", path: "/", name: "session" },
  });
  await waitFor(() => expect(shell.toasts).toEqual(["Deleted cookie session"]));
  const action = shell.toastActions[0];
  expect(action?.label).toBe("Undo");
  action?.run();
  await waitFor(() => expect(calls(ipc, "save_cookie")).toHaveLength(1));
  expect(calls(ipc, "save_cookie")[0]).toMatchObject({
    environment: "staging",
    original: { domain: "api.test", path: "/", name: "session" },
    cookie: { name: "session", value: "session-value" },
  });
});

test("delete_all_for_a_domain_asks_first", async () => {
  const ipc = fakeRequestsIpc({
    cookies: fakeCookies({
      staging: [cookie("a", "api.test"), cookie("b", "api.test")],
    }),
  });
  cookiesTab("staging");
  await userEvent.click(
    await screen.findByRole("button", { name: "Delete all for this domain…" }),
  );
  const dialog = screen.getByRole("dialog", {
    name: "Delete 2 cookies for api.test?",
  });
  expect(
    within(dialog).getByText("Requests in staging stop sending them."),
  ).toBeInTheDocument();
  expect(calls(ipc, "delete_domain_cookies")).toEqual([]);
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
  await userEvent.click(
    within(dialog).getByRole("button", { name: "Delete cookies" }),
  );
  await waitFor(() =>
    expect(calls(ipc, "delete_domain_cookies")).toEqual([
      { environment: "staging", domain: "api.test" },
    ]),
  );
});

test("cancelling_delete_all_deletes_nothing", async () => {
  const ipc = fakeRequestsIpc({
    cookies: fakeCookies({ staging: [cookie("a", "api.test")] }),
  });
  cookiesTab("staging");
  await userEvent.click(
    await screen.findByRole("button", { name: "Delete all for this domain…" }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(calls(ipc, "delete_domain_cookies")).toEqual([]);
});

test("manage_cookies_reuses_the_open_tab", async () => {
  fakeRequestsIpc({});
  project.environment = { name: "staging", production: false };
  await manageCookies();
  expect(shell.tabs.map((t) => [t.tabType, t.params])).toEqual([
    ["requests.cookies", { environment: "staging" }],
  ]);
  await manageCookies();
  expect(shell.tabs).toHaveLength(1);
  expect(shell.activated).toEqual([shell.tabs[0]?.id]);
});

test("the_manifest_has_the_cookie_tab_and_command", () => {
  const tab = manifest.tabTypes?.find((t) => t.id === "requests.cookies");
  expect(tab).toMatchObject({ title: "Cookies", icon: "cookie" });
  expect(tab?.environment).toBeUndefined();
  expect(
    manifest.commands?.find((c) => c.id === "requests.manageCookies"),
  ).toMatchObject({
    action: { type: "intent", intent: "requests.manageCookies" },
    scope: "project",
  });
  expect(
    manifest.intents?.some((i) => i.intent === "requests.manageCookies"),
  ).toBe(true);
});
