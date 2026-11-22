import { screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import type { RequestFile } from "$lib/ipc/bindings/RequestFile";
import { DEFAULT_SETTINGS, settings } from "$lib/settings/settings.svelte";
import {
  openRequestTab,
  setUpRequestTabs,
  type FakeShell,
} from "../../request-tab-support.svelte";
import { doneExchange, fakeRequestsIpc } from "../../test-support";
import type { ResponseInfo } from "$lib/ipc/bindings/ResponseInfo";

const PATH = "requests/shipments/list.yaml";
const file: RequestFile = {
  name: "List shipments",
  method: "GET",
  url: "https://api.test/v1/shipments",
};
const files = { [PATH]: file };

const tab = (name: string | RegExp) =>
  within(screen.getByRole("tablist", { name: "Response sections" })).getByRole(
    "tab",
    { name },
  );

let shell: FakeShell;
beforeEach(() => {
  ({ shell } = setUpRequestTabs());
  settings.apply(DEFAULT_SETTINGS);
});

async function open(over: Partial<ResponseInfo> = {}): Promise<void> {
  fakeRequestsIpc({ files, send: doneExchange("json", over) });
  openRequestTab(PATH);
  await userEvent.click(await screen.findByRole("button", { name: "Send" }));
  await screen.findByRole("tablist", { name: "Response sections" });
}

const cookie = {
  name: "session",
  value: "abc123",
  domain: "api.test",
  path: "/",
  expires: "2026-11-04T12:00:00Z",
  secure: true,
  httpOnly: true,
  hostOnly: true,
};

test("tabs_carry_the_header_and_cookie_counts", async () => {
  await open({
    headers: [
      { name: "a", value: "1" },
      { name: "b", value: "2" },
    ],
    cookies: [cookie],
  });
  const tabs = within(
    screen.getByRole("tablist", { name: "Response sections" }),
  ).getAllByRole("tab");
  expect(tabs.map((t) => t.textContent?.replace(/\s+/g, " ").trim())).toEqual([
    "Body",
    "Headers 2",
    "Cookies 1",
  ]);
  expect(tabs[0]).toHaveAttribute("aria-selected", "true");
});

test("cookies_tab_has_no_count_without_cookies", async () => {
  await open();
  expect(tab("Cookies")).toBeInTheDocument();
});

test("arrow_keys_move_between_the_tabs", async () => {
  await open();
  tab("Body").focus();
  await userEvent.keyboard("{ArrowRight}");
  expect(tab(/^Headers/)).toHaveFocus();
  expect(tab(/^Headers/)).toHaveAttribute("aria-selected", "true");
  await userEvent.keyboard("{End}");
  expect(tab("Cookies")).toHaveFocus();
});

test("headers_keep_their_order_and_duplicates", async () => {
  await open({
    headers: [
      { name: "set-cookie", value: "a=1" },
      { name: "x-id", value: "7" },
      { name: "set-cookie", value: "b=2" },
    ],
  });
  await userEvent.click(tab(/^Headers/));
  const table = screen.getByRole("table", { name: "Response headers" });
  expect(
    within(table)
      .getAllByRole("columnheader")
      .map((c) => c.textContent),
  ).toEqual(["Name", "Value"]);
  const rows = within(table).getAllByRole("row").slice(1);
  expect(
    rows.map((r) =>
      within(r)
        .getAllByRole("cell")
        .map((c) => c.textContent),
    ),
  ).toEqual([
    ["set-cookie", "a=1"],
    ["x-id", "7"],
    ["set-cookie", "b=2"],
  ]);
});

test("redirects_are_listed_above_the_headers", async () => {
  await open({
    redirects: [
      { status: 301, url: "http://api.test/old" },
      { status: 302, url: "https://api.test/older" },
    ],
  });
  await userEvent.click(tab(/^Headers/));
  expect(screen.getByText("Redirected 2 times")).toBeInTheDocument();
  expect(screen.getByText("301 http://api.test/old")).toBeInTheDocument();
  expect(screen.getByText("302 https://api.test/older")).toBeInTheDocument();
});

test("one_redirect_is_once", async () => {
  await open({ redirects: [{ status: 301, url: "http://api.test/old" }] });
  await userEvent.click(tab(/^Headers/));
  expect(screen.getByText("Redirected 1 time")).toBeInTheDocument();
});

test("no_redirects_no_line", async () => {
  await open();
  await userEvent.click(tab(/^Headers/));
  expect(screen.queryByText(/^Redirected/)).toBeNull();
});

test("cookies_tab_lists_received_cookies", async () => {
  await open({
    cookies: [
      cookie,
      {
        ...cookie,
        name: "theme",
        expires: null,
        secure: false,
        httpOnly: false,
      },
    ],
  });
  await userEvent.click(tab(/^Cookies/));
  const table = screen.getByRole("table", { name: "Received cookies" });
  expect(
    within(table)
      .getAllByRole("columnheader")
      .map((c) => c.textContent),
  ).toEqual(["Name", "Value", "Domain", "Path", "Expires", "Flags"]);
  const rows = within(table).getAllByRole("row").slice(1);
  const cells = rows.map((r) =>
    within(r)
      .getAllByRole("cell")
      .map((c) => c.textContent?.replace(/\s+/g, " ").trim()),
  );
  expect(cells[0]).toEqual([
    "session",
    "abc123",
    "api.test",
    "/",
    "4 Nov 2026",
    "Secure HttpOnly",
  ]);
  expect(cells[1]).toEqual(["theme", "abc123", "api.test", "/", "Session", ""]);
});

test("cookies_tab_is_empty_and_manages", async () => {
  await open();
  await userEvent.click(tab("Cookies"));
  expect(screen.getByText("This response set no cookies.")).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Manage cookies" }));
  expect(shell.commands).toContain("requests.manageCookies");
});

test("body_views_sit_on_the_second_row", async () => {
  await open();
  const row = screen.getByRole("tablist", {
    name: "Response sections",
  }).parentElement;
  expect(row).toContainElement(
    screen.getByRole("radiogroup", { name: "Body view" }),
  );
  expect(row).not.toContainElement(
    within(screen.getByRole("region", { name: "Response" })).getByRole(
      "tabpanel",
    ),
  );
});

test("the_view_buttons_hide_outside_the_body_tab", async () => {
  await open();
  expect(
    screen.getByRole("radiogroup", { name: "Body view" }),
  ).toBeInTheDocument();
  await userEvent.click(tab(/^Headers/));
  expect(screen.queryByRole("radiogroup", { name: "Body view" })).toBeNull();
  expect(
    screen.queryByRole("textbox", { name: "Filter with JSONPath" }),
  ).toBeNull();
  await userEvent.click(tab("Body"));
  expect(
    screen.getByRole("radiogroup", { name: "Body view" }),
  ).toBeInTheDocument();
});
