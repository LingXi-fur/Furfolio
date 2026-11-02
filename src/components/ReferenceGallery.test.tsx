import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../i18n";
import type { Asset } from "../data/assets";
import { ReferenceGallery } from "./ReferenceGallery";

const data = vi.hoisted(() => ({ listAssets: vi.fn(), importAsset: vi.fn(), readAssetPreview: vi.fn(), updateAsset: vi.fn(), deleteAsset: vi.fn() }));
vi.mock("../data/assets", async (importOriginal) => ({ ...await importOriginal<typeof import("../data/assets")>(), ...data }));

const asset: Asset = {
  id: "ref-one", characterId: "nova", originalFilename: "reference.png", mimeType: "image/png", byteSize: 1234,
  width: 640, height: 480, artistName: "Artist", sourceUrl: "https://example.com/source", notes: "Private reference note", createdAt: "2026-10-01T00:00:00Z",
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function props() {
  return { characterId: "nova", onChanged: vi.fn(), onBusyChange: vi.fn(), onCleanupPending: vi.fn() };
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  Object.values(data).forEach((mock) => mock.mockReset());
  data.listAssets.mockResolvedValue([]);
  data.importAsset.mockResolvedValue(null);
  data.readAssetPreview.mockResolvedValue(new Blob(["preview"], { type: "image/png" }));
  vi.stubGlobal("URL", URL);
  Object.defineProperty(URL, "createObjectURL", { configurable: true, value: vi.fn().mockReturnValue("blob:preview") });
  Object.defineProperty(URL, "revokeObjectURL", { configurable: true, value: vi.fn() });
});

describe("ReferenceGallery", () => {
  it("shows the empty state and treats native-dialog cancellation as no change", async () => {
    const user = userEvent.setup();
    const callbacks = props();
    render(<ReferenceGallery {...callbacks} />);
    await screen.findByText("No reference images yet.");
    await user.click(screen.getByRole("button", { name: "Import reference image" }));
    expect(data.importAsset).toHaveBeenCalledWith("nova");
    expect(callbacks.onChanged).not.toHaveBeenCalled();
    expect(callbacks.onBusyChange.mock.calls).toEqual([["nova", true], ["nova", false]]);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("imports one image and reports busy until the native dialog completes", async () => {
    const importing = deferred<Asset | null>();
    data.importAsset.mockReturnValue(importing.promise);
    const callbacks = props();
    render(<ReferenceGallery {...callbacks} />);
    const button = await screen.findByRole("button", { name: "Import reference image" });
    await waitFor(() => expect(button).toBeEnabled());
    fireEvent.click(button); fireEvent.click(button);
    expect(data.importAsset).toHaveBeenCalledTimes(1);
    expect(button).toBeDisabled();
    await act(async () => { importing.resolve(asset); });
    expect(await screen.findByRole("img", { name: "reference.png" })).toHaveAttribute("src", "blob:preview");
    expect(screen.getByText("640 × 480")).toBeInTheDocument();
    expect(callbacks.onChanged).toHaveBeenCalledWith("nova");
    expect(callbacks.onBusyChange).toHaveBeenLastCalledWith("nova", false);
  });

  it("displays metadata as text and saves an inline, trimmed metadata form", async () => {
    const user = userEvent.setup();
    data.listAssets.mockResolvedValue([asset]);
    data.updateAsset.mockResolvedValue({ ...asset, artistName: "New artist", notes: null });
    const callbacks = props();
    render(<ReferenceGallery {...callbacks} />);
    await screen.findByText("Private reference note");
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Edit image details" }));
    const artist = screen.getByRole("textbox", { name: "Artist" });
    expect(artist).toHaveAttribute("maxlength", "200");
    expect(screen.getByRole("textbox", { name: "Source URL" })).toHaveAttribute("maxlength", "2048");
    expect(screen.getByRole("textbox", { name: "Image notes" })).toHaveAttribute("maxlength", "10000");
    await user.clear(artist); await user.type(artist, "  New artist  ");
    await user.clear(screen.getByRole("textbox", { name: "Image notes" }));
    await user.click(screen.getByRole("button", { name: "Save image details" }));
    expect(data.updateAsset).toHaveBeenCalledWith("nova", asset.id, { artistName: "New artist", sourceUrl: asset.sourceUrl, notes: null });
    expect(await screen.findByText("New artist")).toBeInTheDocument();
    expect(callbacks.onChanged).toHaveBeenCalledWith("nova");
  });

  it("rejects credentialed source URLs without submitting", async () => {
    const user = userEvent.setup();
    data.listAssets.mockResolvedValue([asset]);
    render(<ReferenceGallery {...props()} />);
    await user.click(await screen.findByRole("button", { name: "Edit image details" }));
    const url = screen.getByRole("textbox", { name: "Source URL" });
    await user.clear(url); await user.type(url, "https://user:pass@example.com");
    await user.click(screen.getByRole("button", { name: "Save image details" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Enter valid image details");
    expect(data.updateAsset).not.toHaveBeenCalled();
  });

  it("reuses native confirmation, focuses cancel, and protects an in-flight removal from Escape", async () => {
    const user = userEvent.setup();
    data.listAssets.mockResolvedValue([asset]);
    const deleting = deferred<null>(); data.deleteAsset.mockReturnValue(deleting.promise);
    render(<ReferenceGallery {...props()} />);
    await user.click(await screen.findByRole("button", { name: "Remove image" }));
    const dialog = screen.getByRole("dialog", { name: "Remove reference.png?" });
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    fireEvent(dialog, new Event("cancel", { cancelable: true }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: "Remove image" })).toHaveFocus());
    await user.click(screen.getByRole("button", { name: "Remove image" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Remove image" }));
    fireEvent(screen.getByRole("dialog"), new Event("cancel", { cancelable: true }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await act(async () => { deleting.resolve(null); });
    expect(await screen.findByText("No reference images yet.")).toBeInTheDocument();
  });

  it("removes committed deletions even when file cleanup is pending", async () => {
    const user = userEvent.setup(); data.listAssets.mockResolvedValue([asset]);
    data.deleteAsset.mockRejectedValue({ code: "cleanupPending", applied: true, message: "fixed" });
    const callbacks = props(); render(<ReferenceGallery {...callbacks} />);
    await user.click(await screen.findByRole("button", { name: "Remove image" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Remove image" }));
    expect(await screen.findByText("No reference images yet.")).toBeInTheDocument();
    expect(callbacks.onCleanupPending).toHaveBeenCalled();
    expect(callbacks.onChanged).toHaveBeenCalledWith("nova");
  });

  it("keeps images and metadata on ordinary failures and allows retry", async () => {
    const user = userEvent.setup(); data.listAssets.mockResolvedValue([asset]);
    data.deleteAsset.mockRejectedValue({ code: "storageError", message: "/private/path" });
    render(<ReferenceGallery {...props()} />);
    await user.click(await screen.findByRole("button", { name: "Remove image" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Remove image" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Furfolio could not access the local image library.");
    expect(screen.getByText("Private reference note")).toBeInTheDocument();
    expect(screen.queryByText("/private/path")).not.toBeInTheDocument();
    expect(within(screen.getByRole("dialog")).getByRole("button", { name: "Remove image" })).toBeEnabled();
  });

  it("ignores late lists when the character changes", async () => {
    const oldList = deferred<Asset[]>(); data.listAssets.mockReturnValueOnce(oldList.promise).mockResolvedValueOnce([]);
    const callbacks = props(); const { rerender } = render(<ReferenceGallery {...callbacks} />);
    rerender(<ReferenceGallery {...callbacks} characterId="echo" />);
    await screen.findByText("No reference images yet.");
    await act(async () => { oldList.resolve([asset]); });
    expect(screen.queryByText("reference.png")).not.toBeInTheDocument();
    expect(data.readAssetPreview).not.toHaveBeenCalled();
  });

  it("reads only the selected preview and revokes Blob URLs on selection and unmount", async () => {
    const user = userEvent.setup();
    const second = { ...asset, id: "ref-two", originalFilename: "second.webp" };
    data.listAssets.mockResolvedValue([asset, second]);
    vi.mocked(URL.createObjectURL).mockReturnValueOnce("blob:first").mockReturnValueOnce("blob:second");
    const { unmount } = render(<ReferenceGallery {...props()} />);
    expect(await screen.findByRole("img", { name: "reference.png" })).toHaveAttribute("src", "blob:first");
    expect(data.readAssetPreview).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "View second.webp" }));
    expect(await screen.findByRole("img", { name: "second.webp" })).toHaveAttribute("src", "blob:second");
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:first");
    unmount(); expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:second");
  });

  it("does not create Blob URLs for previews that return after a switch or unmount", async () => {
    const late = deferred<Blob>(); data.listAssets.mockResolvedValue([asset]); data.readAssetPreview.mockReturnValue(late.promise);
    const callbacks = props(); const { rerender, unmount } = render(<ReferenceGallery {...callbacks} />);
    await waitFor(() => expect(data.readAssetPreview).toHaveBeenCalled());
    data.listAssets.mockResolvedValue([]); rerender(<ReferenceGallery {...callbacks} characterId="echo" />);
    unmount();
    await act(async () => { late.resolve(new Blob(["late"], { type: "image/png" })); });
    expect(URL.createObjectURL).not.toHaveBeenCalled();
  });

  it("ignores late imports after navigation but releases the character-scoped lock", async () => {
    const user = userEvent.setup(); const importing = deferred<Asset | null>(); data.importAsset.mockReturnValue(importing.promise);
    const callbacks = props(); const { rerender } = render(<ReferenceGallery {...callbacks} />);
    await screen.findByText("No reference images yet.");
    await user.click(screen.getByRole("button", { name: "Import reference image" }));
    rerender(<ReferenceGallery {...callbacks} characterId="echo" />);
    await act(async () => { importing.resolve(asset); });
    expect(screen.queryByText("reference.png")).not.toBeInTheDocument();
    expect(callbacks.onBusyChange).toHaveBeenLastCalledWith("nova", false);
  });

  it("releases a failed rendered preview and does not revoke it twice", async () => {
    data.listAssets.mockResolvedValue([asset]);
    const { unmount } = render(<ReferenceGallery {...props()} />);
    fireEvent.error(await screen.findByRole("img", { name: "reference.png" }));
    expect(screen.getByText("This image cannot be previewed.")).toBeInTheDocument();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:preview");
    unmount();
    expect(URL.revokeObjectURL).toHaveBeenCalledTimes(1);
  });

  it("bounds preview IPC to one request and skips superseded selections", async () => {
    const user = userEvent.setup();
    const late = deferred<Blob>();
    data.listAssets.mockResolvedValue([asset, { ...asset, id: "two", originalFilename: "two.png" }, { ...asset, id: "three", originalFilename: "three.png" }]);
    data.readAssetPreview.mockReturnValueOnce(late.promise).mockResolvedValue(new Blob(["latest"], { type: "image/png" }));
    render(<ReferenceGallery {...props()} />);
    await waitFor(() => expect(data.readAssetPreview).toHaveBeenCalledTimes(1));
    await user.click(screen.getByRole("button", { name: "View two.png" }));
    await user.click(screen.getByRole("button", { name: "View three.png" }));
    expect(data.readAssetPreview).toHaveBeenCalledTimes(1);
    await act(async () => { late.resolve(new Blob(["old"], { type: "image/png" })); });
    await screen.findByRole("img", { name: "three.png" });
    expect(data.readAssetPreview.mock.calls).toEqual([["nova", "ref-one"], ["nova", "three"]]);
    expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
  });

  it("reloads after an operation owned by a previous gallery session finishes", async () => {
    data.listAssets.mockResolvedValueOnce([]).mockResolvedValueOnce([asset]);
    const callbacks = props();
    const { rerender } = render(<ReferenceGallery {...callbacks} busy />);
    await screen.findByText("No reference images yet.");
    rerender(<ReferenceGallery {...callbacks} busy={false} />);
    await screen.findByRole("img", { name: "reference.png" });
    expect(data.listAssets).toHaveBeenCalledTimes(2);
  });

  it("does not let an automatic reload overwrite a newer import", async () => {
    const user = userEvent.setup(); const snapshot = deferred<Asset[]>();
    data.listAssets.mockResolvedValueOnce([]).mockReturnValueOnce(snapshot.promise);
    data.importAsset.mockResolvedValue(asset);
    const callbacks = props(); const { rerender } = render(<ReferenceGallery {...callbacks} busy />);
    await screen.findByText("No reference images yet.");
    rerender(<ReferenceGallery {...callbacks} busy={false} />);
    await waitFor(() => expect(data.listAssets).toHaveBeenCalledTimes(2));
    await user.click(screen.getByRole("button", { name: "Import reference image" }));
    await screen.findByRole("img", { name: "reference.png" });
    await act(async () => { snapshot.resolve([]); });
    expect(screen.getByRole("img", { name: "reference.png" })).toBeInTheDocument();
  });

  it("does not resurrect a removed image from an automatic reload snapshot", async () => {
    const user = userEvent.setup(); const snapshot = deferred<Asset[]>();
    data.listAssets.mockResolvedValueOnce([asset]).mockReturnValueOnce(snapshot.promise);
    data.deleteAsset.mockResolvedValue(null);
    const callbacks = props(); const { rerender } = render(<ReferenceGallery {...callbacks} busy />);
    await screen.findByRole("img", { name: "reference.png" });
    rerender(<ReferenceGallery {...callbacks} busy={false} />);
    await waitFor(() => expect(data.listAssets).toHaveBeenCalledTimes(2));
    await user.click(screen.getByRole("button", { name: "Remove image" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Remove image" }));
    await screen.findByText("No reference images yet.");
    await act(async () => { snapshot.resolve([asset]); });
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.getByText("No reference images yet.")).toBeInTheDocument();
  });

  it("shows recoverable list and preview failures and bilingual copy", async () => {
    const user = userEvent.setup(); data.listAssets.mockRejectedValueOnce({ code: "mediaBusy" }).mockResolvedValueOnce([asset]);
    data.readAssetPreview.mockRejectedValueOnce({ code: "invalidMedia" }).mockResolvedValueOnce(new Blob(["ok"], { type: "image/png" }));
    render(<ReferenceGallery {...props()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Another image operation is in progress.");
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("This image cannot be previewed.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry preview" }));
    await screen.findByRole("img", { name: "reference.png" });
    await act(async () => { await i18n.changeLanguage("zh-CN"); });
    expect(screen.getByRole("button", { name: "导入参考图" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "编辑图片资料" })).toBeInTheDocument();
  });
});
