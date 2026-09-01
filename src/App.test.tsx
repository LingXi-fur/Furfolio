import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act } from "react";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "./i18n";
import App from "./App";
import { setSystemTheme } from "./test/setup";

describe("App", () => {
  beforeEach(async () => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
    await i18n.changeLanguage("en");
  });

  it("shows the private local-first empty state", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: /keep every character detail/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /create first character/i })).toBeInTheDocument();
    expect(screen.getByText(/no account and no cloud upload/i)).toBeInTheDocument();
  });

  it("switches between English and Simplified Chinese", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.selectOptions(screen.getByRole("combobox", { name: "Language" }), "zh-CN");

    expect(screen.getByRole("heading", { name: "把每一份角色设定，安心整理在一处。" })).toBeInTheDocument();
    expect(localStorage.getItem("furfolio-language")).toBe("zh-CN");
  });

  it("uses the system theme without storing a preference", () => {
    render(<App />);

    expect(document.documentElement.dataset.theme).toBe("light");
    expect(localStorage.getItem("furfolio-theme")).toBeNull();
  });

  it("follows system theme changes without a saved preference", () => {
    render(<App />);

    act(() => setSystemTheme(true));

    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem("furfolio-theme")).toBeNull();
  });

  it("stores and applies the selected theme", async () => {
    const user = userEvent.setup();
    render(<App />);

    const themeButton = screen.getByRole("button", { name: /theme/i });
    await user.click(themeButton);

    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem("furfolio-theme")).toBe("dark");

    act(() => setSystemTheme(false));

    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});
