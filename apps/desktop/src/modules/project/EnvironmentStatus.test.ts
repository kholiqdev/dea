import { render, screen } from "@testing-library/svelte";
import { expect, test } from "vitest";
import { setProjectApi, type ProjectApi } from "$app/contributions/project-api";
import EnvironmentStatus from "./components/EnvironmentStatus.svelte";

function api(environment: ProjectApi["environment"]): ProjectApi {
  return {
    info: null,
    environment,
    changed: [],
    setEnvironment: () => Promise.resolve(),
    refresh: () => {},
  };
}

test("the status bar names the environment", () => {
  setProjectApi(api({ name: "staging", production: false }));
  render(EnvironmentStatus);
  const item = screen.getByText("staging");
  expect(item.closest(".env-prod-status")).toBeNull();
});

test("production is marked in the status bar", () => {
  setProjectApi(api({ name: "production", production: true }));
  render(EnvironmentStatus);
  expect(
    screen.getByText("production").closest(".env-prod-status"),
  ).not.toBeNull();
});

test("nothing shows without a project", () => {
  setProjectApi(api(null));
  const { container } = render(EnvironmentStatus);
  expect(container.textContent?.trim()).toBe("");
});
