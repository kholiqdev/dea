import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import {
  setUpRequestTabs,
  type FakeShell,
} from "../request-tab-support.svelte";
import { fakeRequestsIpc } from "../test-support";
import ResolvedUrl from "./ResolvedUrl.svelte";

let shell: FakeShell;

beforeEach(() => {
  ({ shell } = setUpRequestTabs());
});

const URL = "https://api.harbor.test/v1/shipments/shp_4821";

test("the_final_url_is_a_button_that_copies_it", async () => {
  const ipc = fakeRequestsIpc();
  render(ResolvedUrl, { props: { url: URL, undefinedNames: [] } });
  // Its name says what it does and reads the URL, without the arrow.
  const button = screen.getByRole("button", {
    name: `Copy the final URL, ${URL}`,
  });
  expect(button).not.toHaveAttribute("aria-label");
  expect(button).toHaveAttribute("data-final-url");
  expect(button).toHaveTextContent(`→ ${URL}`);
  expect(button).toHaveAttribute("title", URL);
  expect(button.querySelector('[aria-hidden="true"]')).toHaveTextContent("→");
  await userEvent.click(button);
  expect(ipc.copiedTexts).toEqual([URL]);
  await waitFor(() => expect(shell.toasts).toEqual(["URL copied"]));
});

test("a_failed_copy_says_why", async () => {
  const ipc = fakeRequestsIpc();
  ipc.failures.set("copy_text", {
    kind: "io",
    message: "the clipboard is not available",
  });
  render(ResolvedUrl, { props: { url: URL, undefinedNames: [] } });
  await userEvent.click(
    screen.getByRole("button", { name: /^Copy the final URL/ }),
  );
  await waitFor(() =>
    expect(shell.toasts).toEqual(["The clipboard is not available"]),
  );
});

test("an_empty_url_leaves_the_row_without_a_button", () => {
  fakeRequestsIpc();
  const { container, rerender } = render(ResolvedUrl, {
    props: { url: "", undefinedNames: [] },
  });
  expect(screen.queryByRole("button")).toBeNull();
  // The row stays, so what is under it does not move.
  expect(container.querySelector(".resolved")).toBeInTheDocument();
  void rerender({ url: null, undefinedNames: [] });
  expect(screen.queryByRole("button")).toBeNull();
  expect(container.querySelector(".resolved")).toBeInTheDocument();
});

test("undefined_names_are_marked_in_the_final_url", () => {
  fakeRequestsIpc();
  const { container } = render(ResolvedUrl, {
    props: {
      url: "https://api.test/x?c={{missing}}&d={{known}}",
      undefinedNames: ["missing"],
    },
  });
  const marked = [...container.querySelectorAll("span.undefined")];
  expect(marked.map((m) => m.textContent)).toEqual(["{{missing}}"]);
  const hosts = [...container.querySelectorAll("span.host")];
  expect(hosts.map((h) => h.textContent)).toEqual(["api.test"]);
});

test("the_host_ends_at_the_query_or_the_fragment", () => {
  fakeRequestsIpc();
  const { container, rerender } = render(ResolvedUrl, {
    props: { url: "http://localhost:8080?q=1", undefinedNames: [] },
  });
  expect(container.querySelector("span.host")).toHaveTextContent(
    /^localhost:8080$/,
  );
  void rerender({ url: "https://api.test#top", undefinedNames: [] });
  expect(container.querySelector("span.host")).toHaveTextContent(/^api\.test$/);
});

test("an_undefined_host_is_marked_too", () => {
  fakeRequestsIpc();
  const { container } = render(ResolvedUrl, {
    props: {
      url: "http://{{baseUrl}}/v1/shipments",
      undefinedNames: ["baseUrl"],
    },
  });
  const marked = container.querySelector("span.undefined");
  expect(marked).toHaveTextContent(/^\{\{baseUrl\}\}$/);
  expect(marked).toHaveClass("host");
});
