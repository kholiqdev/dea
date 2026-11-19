import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import WelcomeView from "./WelcomeView.svelte";
import { windowActions } from "./api";
import { newProjectPage, relativeDay, welcome } from "./state/welcome.svelte";
import { DAY, fakeWelcomeIpc, type WelcomeIpc } from "./test-support";

const NOW = 1_800_000_000;
let ipc: WelcomeIpc;
let reload: ReturnType<typeof vi.fn<() => void>>;

beforeEach(() => {
  ipc = fakeWelcomeIpc([
    {
      root: "/code/harbor-api",
      name: "harbor-api",
      requestCount: 42,
      lastOpened: NOW - 60,
      missing: false,
    },
    {
      root: "/code/courier-app",
      name: "courier-app",
      requestCount: 1,
      lastOpened: NOW - DAY - 60,
      missing: false,
    },
    {
      root: "/work/billing-proto",
      name: "billing-proto",
      requestCount: 0,
      lastOpened: NOW - 9 * DAY,
      missing: true,
    },
  ]);
  welcome.page = "start";
  reload = vi.fn<() => void>();
  windowActions.reload = reload;
  vi.spyOn(Date, "now").mockReturnValue(NOW * 1000);
});

test("Welcome offers three ways to start and says data stays on this device", () => {
  render(WelcomeView);
  expect(
    screen.getByRole("heading", { name: "Start a project", level: 1 }),
  ).toBeInTheDocument();
  expect(
    screen.getByText(
      "A project is a folder with your requests, connections and hosts. Everything is saved on this device and nothing is uploaded.",
    ),
  ).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /Start empty/ })).toBeEnabled();
  expect(
    screen.getByRole("button", { name: /Import what you have/ }),
  ).toBeDisabled();
  expect(
    screen.getByText("Comes with import in a later version"),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: /Open a project folder/ }),
  ).toBeEnabled();
});

test("recent projects show request counts and when they were opened", async () => {
  render(WelcomeView);
  const list = await screen.findByRole("list", { name: "Recent projects" });
  const rows = within(list).getAllByRole("listitem");
  expect(rows).toHaveLength(3);
  expect(rows[0]).toHaveTextContent("harbor-api");
  expect(rows[0]).toHaveTextContent("42 requests");
  expect(rows[0]).toHaveTextContent("Opened today");
  expect(rows[1]).toHaveTextContent("1 request");
  expect(rows[1]).toHaveTextContent("Yesterday");
  await userEvent.click(
    within(rows[0]!).getByRole("button", { name: /harbor-api/ }),
  );
  expect(ipc.calls).toContainEqual({
    cmd: "open_project",
    args: { root: "/code/harbor-api" },
  });
  expect(reload).toHaveBeenCalled();
});

test("a missing folder offers Locate and Remove from list", async () => {
  render(WelcomeView);
  const list = await screen.findByRole("list", { name: "Recent projects" });
  const row = within(list).getAllByRole("listitem")[2]!;
  expect(row).toHaveTextContent("Folder not found");
  ipc.picked = "/work/billing-proto-moved";
  await userEvent.click(within(row).getByRole("button", { name: "Locate" }));
  expect(ipc.calls).toContainEqual({
    cmd: "open_project",
    args: { root: "/work/billing-proto-moved" },
  });

  await userEvent.click(
    within(row).getByRole("button", { name: "Remove from list" }),
  );
  expect(ipc.calls).toContainEqual({
    cmd: "remove_recent",
    args: { root: "/work/billing-proto" },
  });
  expect(await within(list).findAllByRole("listitem")).toHaveLength(2);
});

test("Start empty creates the project and opens it in this window", async () => {
  render(WelcomeView);
  await userEvent.click(screen.getByRole("button", { name: /Start empty/ }));
  expect(
    screen.getByRole("heading", { name: "New project", level: 1 }),
  ).toBeInTheDocument();
  await userEvent.type(
    screen.getByRole("textbox", { name: "Project name" }),
    "harbor-api",
  );
  ipc.picked = "/code/harbor-api";
  await userEvent.click(screen.getByRole("button", { name: "Choose…" }));
  expect(screen.getByRole("textbox", { name: "Location" })).toHaveValue(
    "/code/harbor-api",
  );
  await userEvent.click(screen.getByRole("button", { name: "Create project" }));
  expect(ipc.calls).toContainEqual({
    cmd: "create_project",
    args: { location: "/code/harbor-api", name: "harbor-api" },
  });
  expect(reload).toHaveBeenCalled();
});

test("Create project needs a name and a location", async () => {
  welcome.page = newProjectPage();
  render(WelcomeView);
  const create = screen.getByRole("button", { name: "Create project" });
  expect(create).toBeDisabled();
  await userEvent.type(
    screen.getByRole("textbox", { name: "Project name" }),
    "x",
  );
  expect(create).toBeDisabled();
  await userEvent.type(
    screen.getByRole("textbox", { name: "Location" }),
    "/code/x",
  );
  expect(create).toBeEnabled();
  await userEvent.click(screen.getByRole("button", { name: "Back" }));
  expect(
    screen.getByRole("heading", { name: "Start a project" }),
  ).toBeInTheDocument();
});

test("a folder without dea.yaml shows why and offers to create a project there", async () => {
  render(WelcomeView);
  ipc.picked = "/code/notes";
  ipc.notProjects.add("/code/notes");
  await userEvent.click(
    screen.getByRole("button", { name: /Open a project folder/ }),
  );
  expect(await screen.findByText(/has no/)).toHaveTextContent(
    "/code/notes has no dea.yaml, so it is not a Dea project.",
  );
  expect(reload).not.toHaveBeenCalled();
  await userEvent.click(
    screen.getByRole("button", { name: "Create a project there" }),
  );
  expect(screen.getByRole("textbox", { name: "Location" })).toHaveValue(
    "/code/notes",
  );
  expect(screen.getByRole("textbox", { name: "Project name" })).toHaveValue(
    "notes",
  );
});

test("relativeDay", () => {
  expect(relativeDay(NOW, NOW)).toBe("Opened today");
  expect(relativeDay(NOW - DAY, NOW)).toBe("Yesterday");
  expect(relativeDay(NOW - 6 * DAY, NOW)).toBe("6 days ago");
  expect(relativeDay(NOW - 7 * DAY, NOW)).toBe("Last week");
  expect(relativeDay(NOW - 13 * DAY, NOW)).toBe("Last week");
  expect(relativeDay(NOW - 14 * DAY, NOW)).toMatch(/\d{4}/);
});
