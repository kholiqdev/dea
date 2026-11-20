import { fireEvent, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import VariableField from "./components/VariableField.svelte";
import { lookupFrom } from "./test-support";

const SHIPPING = {
  variables: [
    {
      name: "baseUrl",
      shown: "https://harbor.test",
      source: "Environment staging",
      secret: false,
    },
    {
      name: "vault:token",
      shown: "••••••••",
      source: "Vault, staging. The value stays hidden",
      secret: true,
    },
  ],
  undefined: ["carrierCode"],
};

test("an undefined variable is red and its popover says where it is missing", async () => {
  const lookup = lookupFrom(
    { variables: [], undefined: ["carrierCode"] },
    "staging",
  );
  render(VariableField, {
    value: "?carrier={{carrierCode}}",
    label: "URL",
    variables: lookup,
    oninput: () => {},
  });
  expect(document.querySelector(".undefined-var")).toHaveTextContent(
    "{{carrierCode}}",
  );
  const input = screen.getByRole<HTMLInputElement>("textbox", {
    name: "URL",
  });
  input.focus();
  input.setSelectionRange(12, 12);
  await userEvent.keyboard("{ArrowRight}");
  expect(screen.getByRole("tooltip")).toHaveTextContent(
    "Not defined in staging or in this collection.",
  );
  expect(input).toHaveAttribute(
    "aria-describedby",
    screen.getByRole("tooltip").id,
  );
});

test("without an environment the popover says no environment is active", async () => {
  render(VariableField, {
    value: "{{carrierCode}}",
    label: "URL",
    variables: lookupFrom({ undefined: ["carrierCode"] }, null),
    oninput: () => {},
  });
  await fireEvent.mouseEnter(
    document.querySelector(".undefined-var") as Element,
  );
  expect(screen.getByRole("tooltip")).toHaveTextContent(
    "Not defined. No environment is active.",
  );
});

test("a secret shows dots and its vault source", async () => {
  render(VariableField, {
    value: "{{vault:token}}",
    label: "Value",
    variables: lookupFrom(SHIPPING, "staging"),
    oninput: () => {},
  });
  await fireEvent.mouseEnter(document.querySelector(".variable") as Element);
  const tip = screen.getByRole("tooltip");
  expect(tip).toHaveTextContent("••••••••");
  expect(tip).toHaveTextContent("Vault, staging. The value stays hidden");
});

test("a Vault name the inspection leaves undefined is marked, however it is spaced", async () => {
  // Dea names an undefined placeholder as written; a found secret as
  // `vault:<name>`.
  const lookup = lookupFrom(
    { ...SHIPPING, undefined: ["vault: tokn"] },
    "staging",
  );
  render(VariableField, {
    value: "{{vault: tokn}} {{ vault: token }}",
    label: "Value",
    variables: lookup,
    oninput: () => {},
  });
  const [misspelt, spaced] = document.querySelectorAll(".variable");
  expect(misspelt).toHaveClass("undefined-var");
  expect(spaced).not.toHaveClass("undefined-var");
  await fireEvent.mouseEnter(spaced as Element);
  expect(screen.getByRole("tooltip")).toHaveTextContent(
    "Vault, staging. The value stays hidden",
  );
});

test("hovering a chip shows its value and source", async () => {
  render(VariableField, {
    value: "{{baseUrl}}/ships",
    label: "URL",
    variables: lookupFrom(SHIPPING, "staging"),
    oninput: () => {},
  });
  expect(screen.queryByRole("tooltip")).toBeNull();
  const chip = document.querySelector(".variable") as Element;
  await fireEvent.mouseEnter(chip);
  const tip = screen.getByRole("tooltip");
  expect(tip).toHaveTextContent("{{baseUrl}}");
  expect(tip).toHaveTextContent("https://harbor.test");
  expect(tip).toHaveTextContent("From");
  expect(tip).toHaveTextContent("Environment staging");
  await fireEvent.mouseLeave(chip);
  expect(screen.queryByRole("tooltip")).toBeNull();
});

test("escape hides the popover", async () => {
  render(VariableField, {
    value: "{{baseUrl}}/ships",
    label: "URL",
    variables: lookupFrom(SHIPPING, "staging"),
    oninput: () => {},
  });
  const input = screen.getByRole<HTMLInputElement>("textbox", {
    name: "URL",
  });
  input.focus();
  input.setSelectionRange(4, 4);
  await fireEvent.keyUp(input, { key: "ArrowRight" });
  expect(screen.getByRole("tooltip")).toBeInTheDocument();
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("tooltip")).toBeNull();
});

test("a variable is neutral while the inspection is loading", () => {
  render(VariableField, {
    value: "{{baseUrl}}",
    label: "URL",
    variables: lookupFrom(null, "staging"),
    oninput: () => {},
  });
  expect(document.querySelector(".variable")).not.toHaveClass("undefined-var");
});

test("dynamic variables are always defined", async () => {
  render(VariableField, {
    value: "{{$guid}}",
    label: "URL",
    variables: lookupFrom({}, "staging"),
    oninput: () => {},
  });
  const chip = document.querySelector(".variable") as Element;
  expect(chip).not.toHaveClass("undefined-var");
  await fireEvent.mouseEnter(chip);
  expect(screen.getByRole("tooltip")).toHaveTextContent(
    "Dynamic, new on each send",
  );
});

test("typing reports the new value, and enter is reported", async () => {
  const oninput = vi.fn();
  const onenter = vi.fn();
  render(VariableField, {
    value: "",
    label: "URL",
    variables: lookupFrom({}, null),
    oninput,
    onenter,
  });
  const input = screen.getByRole("textbox", { name: "URL" });
  await userEvent.type(input, "ab{Enter}");
  expect(oninput).toHaveBeenLastCalledWith("ab");
  expect(onenter).toHaveBeenCalledOnce();
});

test("a defined variable with no description yet has no popover until it is known", async () => {
  const { unmount } = render(VariableField, {
    value: "{{baseUrl}}/ships",
    label: "URL",
    variables: lookupFrom(null, "staging"),
    oninput: () => {},
  });
  const input = screen.getByRole<HTMLInputElement>("textbox", { name: "URL" });
  await fireEvent.mouseEnter(document.querySelector(".variable") as Element);
  expect(screen.queryByRole("tooltip")).toBeNull();
  input.focus();
  input.setSelectionRange(4, 4);
  await fireEvent.keyUp(input, { key: "ArrowRight" });
  expect(screen.queryByRole("tooltip")).toBeNull();
  unmount();

  render(VariableField, {
    value: "{{baseUrl}}/ships",
    label: "URL",
    variables: lookupFrom(SHIPPING, "staging"),
    oninput: () => {},
  });
  await fireEvent.mouseEnter(document.querySelector(".variable") as Element);
  expect(screen.getByRole("tooltip")).toHaveTextContent("Environment staging");
});
