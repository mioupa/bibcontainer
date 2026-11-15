// 「カメラで読む」のカード（FR-SCAN-20。0029）。zxing の読み取り（decode.ts）は差し替え、
// navigator.mediaDevices はスタブにして、カメラの開閉・値の受け渡し・止める条件・失敗の文を確かめる。
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CameraPanel } from "@/features/scanScreen/camera/CameraPanel";
import { useScanSessionStore } from "@/features/scanScreen/sessionStore";
import { ja } from "@/i18n/ja";
import { useModalStore } from "@/lib/useModalGuard";

const decode = vi.hoisted(() => ({
  readFrame: vi.fn<(video: HTMLVideoElement) => Promise<string[]>>(),
  loadDecoder: vi.fn<() => Promise<unknown>>(),
}));
vi.mock("@/features/scanScreen/camera/decode", () => decode);

const ISBN = "9784101001012";
const SECOND = "1920193007008";
const t = ja.scan.screen.camera;

interface FakeTrack {
  stop: ReturnType<typeof vi.fn>;
  addEventListener: ReturnType<typeof vi.fn>;
  getSettings: () => { deviceId: string };
}

function makeStream(deviceId = "cam-a") {
  const track: FakeTrack = {
    stop: vi.fn(),
    addEventListener: vi.fn(),
    getSettings: () => ({ deviceId }),
  };
  const stream = { getTracks: () => [track], getVideoTracks: () => [track] };
  return { stream: stream as unknown as MediaStream, track };
}

function domError(name: string): Error {
  const error = new Error(name);
  error.name = name;
  return error;
}

const getUserMedia = vi.fn<(constraints: MediaStreamConstraints) => Promise<MediaStream>>();
const enumerateDevices = vi.fn<() => Promise<MediaDeviceInfo[]>>();

let hiddenNow = false;
function setHidden(hidden: boolean) {
  hiddenNow = hidden;
  act(() => {
    document.dispatchEvent(new Event("visibilitychange"));
  });
}

function device(deviceId: string, label: string): MediaDeviceInfo {
  return { kind: "videoinput", deviceId, label, groupId: "" } as MediaDeviceInfo;
}

beforeEach(() => {
  getUserMedia.mockReset();
  enumerateDevices.mockReset().mockResolvedValue([device("cam-a", "内蔵カメラ")]);
  decode.readFrame.mockReset().mockResolvedValue([]);
  decode.loadDecoder.mockReset().mockResolvedValue({});
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: { getUserMedia, enumerateDevices },
  });
  hiddenNow = false;
  Object.defineProperty(document, "hidden", { configurable: true, get: () => hiddenNow });
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue(undefined);
  useModalStore.setState({ count: 0 });
  useScanSessionStore.setState({ cameraDeviceId: null });
});

afterEach(() => {
  vi.restoreAllMocks();
});

function turnOn() {
  fireEvent.click(screen.getByRole("button", { name: t.on }));
}

describe("CameraPanel", () => {
  it("開いたときはオフで、カメラを開かない", () => {
    render(<CameraPanel listening onDigits={vi.fn()} />);
    expect(screen.getByRole("button", { name: t.on })).toHaveAttribute("aria-pressed", "false");
    expect(getUserMedia).not.toHaveBeenCalled();
    expect(decode.loadDecoder).not.toHaveBeenCalled();
  });

  it("切り替えを押したあと、フォーカスを手入力欄に戻す", () => {
    getUserMedia.mockResolvedValue(makeStream().stream);
    render(
      <>
        <CameraPanel listening onDigits={vi.fn()} />
        <input data-manual-field="" aria-label="手入力" />
      </>,
    );
    const button = screen.getByRole("button", { name: t.on });
    button.focus();
    fireEvent.click(button);
    expect(screen.getByLabelText("手入力")).toHaveFocus();
  });

  it("オンにすると映像を開き、読めた値を（映したままでも 1 回だけ）渡す", async () => {
    const { stream } = makeStream();
    getUserMedia.mockResolvedValue(stream);
    decode.readFrame.mockResolvedValue([ISBN]);
    const onDigits = vi.fn();
    render(<CameraPanel listening onDigits={onDigits} />);

    turnOn();
    await waitFor(() => expect(onDigits).toHaveBeenCalledWith(ISBN), { timeout: 3000 });
    expect(getUserMedia).toHaveBeenCalledWith(expect.objectContaining({ audio: false }));
    expect(decode.loadDecoder).toHaveBeenCalled();
    expect(screen.getByTestId("camera-frame")).toHaveAttribute("data-flash", "true");

    await new Promise((resolve) => setTimeout(resolve, 600));
    expect(onDigits).toHaveBeenCalledTimes(1);
  });

  it("2 段同時に映ると、ISBN、2段目の順に渡す", async () => {
    getUserMedia.mockResolvedValue(makeStream().stream);
    decode.readFrame.mockResolvedValue([SECOND, ISBN]);
    const onDigits = vi.fn<(digits: string) => void>();
    render(<CameraPanel listening onDigits={onDigits} />);

    turnOn();
    await waitFor(() => expect(onDigits).toHaveBeenCalledTimes(2), { timeout: 3000 });
    expect(onDigits.mock.calls.map((call) => call[0])).toEqual([ISBN, SECOND]);
  });

  it("フォームなどを開いている間（listening が偽）は渡さず、閉じた直後に前のコードを入れない", async () => {
    getUserMedia.mockResolvedValue(makeStream().stream);
    decode.readFrame.mockResolvedValue([ISBN]);
    const onDigits = vi.fn();
    const view = render(<CameraPanel listening={false} onDigits={onDigits} />);

    turnOn();
    await waitFor(() => expect(decode.readFrame.mock.calls.length).toBeGreaterThan(3), {
      timeout: 3000,
    });
    expect(onDigits).not.toHaveBeenCalled();
    expect(screen.getByText(t.paused)).toBeInTheDocument();

    view.rerender(<CameraPanel listening onDigits={onDigits} />);
    const reads = decode.readFrame.mock.calls.length;
    await waitFor(() => expect(decode.readFrame.mock.calls.length).toBeGreaterThan(reads + 2), {
      timeout: 3000,
    });
    expect(onDigits).not.toHaveBeenCalled();
  });

  it("確認ダイアログなど（useModalGuard）を開いている間も渡さない", async () => {
    getUserMedia.mockResolvedValue(makeStream().stream);
    decode.readFrame.mockResolvedValue([ISBN]);
    const onDigits = vi.fn();
    useModalStore.setState({ count: 1 });
    render(<CameraPanel listening onDigits={onDigits} />);

    turnOn();
    await waitFor(() => expect(decode.readFrame.mock.calls.length).toBeGreaterThan(3), {
      timeout: 3000,
    });
    expect(onDigits).not.toHaveBeenCalled();
  });

  it("オフにすると track を止める", async () => {
    const { stream, track } = makeStream();
    getUserMedia.mockResolvedValue(stream);
    render(<CameraPanel listening onDigits={vi.fn()} />);

    turnOn();
    await waitFor(() => expect(screen.getByText(t.reading)).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: t.off }));
    expect(track.stop).toHaveBeenCalled();
    expect(screen.getByText(t.hint)).toBeInTheDocument();
  });

  it("画面を離れる（アンマウント）と track を止める", async () => {
    const { stream, track } = makeStream();
    getUserMedia.mockResolvedValue(stream);
    const view = render(<CameraPanel listening onDigits={vi.fn()} />);

    turnOn();
    await waitFor(() => expect(screen.getByText(t.reading)).toBeInTheDocument());
    view.unmount();
    expect(track.stop).toHaveBeenCalled();
  });

  it("getUserMedia がアンマウントのあとで届いたら、すぐ止める", async () => {
    const { stream, track } = makeStream();
    let resolve: (stream: MediaStream) => void = () => {};
    getUserMedia.mockReturnValue(
      new Promise<MediaStream>((r) => {
        resolve = r;
      }),
    );
    const view = render(<CameraPanel listening onDigits={vi.fn()} />);

    turnOn();
    await waitFor(() => expect(getUserMedia).toHaveBeenCalled());
    view.unmount();
    expect(track.stop).not.toHaveBeenCalled();
    await act(async () => {
      resolve(stream);
      await Promise.resolve();
    });
    expect(track.stop).toHaveBeenCalled();
    expect(decode.loadDecoder).not.toHaveBeenCalled();
  });

  it("ウィンドウが隠れたら止め、見えるようになったら（オンのままなので）開き直す", async () => {
    const first = makeStream();
    const second = makeStream();
    getUserMedia.mockResolvedValueOnce(first.stream).mockResolvedValueOnce(second.stream);
    render(<CameraPanel listening onDigits={vi.fn()} />);

    turnOn();
    await waitFor(() => expect(screen.getByText(t.reading)).toBeInTheDocument());
    setHidden(true);
    expect(first.track.stop).toHaveBeenCalled();

    setHidden(false);
    await waitFor(() => expect(getUserMedia).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByText(t.reading)).toBeInTheDocument());
    expect(second.track.stop).not.toHaveBeenCalled();
  });

  it("許可を断られたら、理由と直し方を出す", async () => {
    getUserMedia.mockRejectedValue(domError("NotAllowedError"));
    render(<CameraPanel listening onDigits={vi.fn()} />);

    turnOn();
    expect(await screen.findByRole("alert")).toHaveTextContent(t.errors.denied);
    expect(t.errors.denied).toContain("システム設定");
  });

  it("カメラがなければ、その文を出す", async () => {
    getUserMedia.mockRejectedValue(domError("NotFoundError"));
    render(<CameraPanel listening onDigits={vi.fn()} />);

    turnOn();
    expect(await screen.findByRole("alert")).toHaveTextContent(t.errors.notFound);
  });

  it("ほかのアプリが使っているときの文を出す", async () => {
    getUserMedia.mockRejectedValue(domError("NotReadableError"));
    render(<CameraPanel listening onDigits={vi.fn()} />);

    turnOn();
    expect(await screen.findByRole("alert")).toHaveTextContent(t.errors.inUse);
  });

  it("安全な接続でない（secure context でない）ときは開かない", async () => {
    Object.defineProperty(window, "isSecureContext", { configurable: true, value: false });
    try {
      render(<CameraPanel listening onDigits={vi.fn()} />);
      turnOn();
      expect(await screen.findByRole("alert")).toHaveTextContent(t.errors.unsupported);
      expect(getUserMedia).not.toHaveBeenCalled();
    } finally {
      Reflect.deleteProperty(window, "isSecureContext");
    }
  });

  it("読む部品（wasm）を読み込めなかったら、カメラを放して、その文を出す", async () => {
    const { stream, track } = makeStream();
    getUserMedia.mockResolvedValue(stream);
    decode.loadDecoder.mockRejectedValue(new Error("wasm"));
    render(<CameraPanel listening onDigits={vi.fn()} />);

    turnOn();
    expect(await screen.findByRole("alert")).toHaveTextContent(t.errors.decoder);
    expect(track.stop).toHaveBeenCalled();
    // 失敗しても、切り替えはできる
    expect(screen.getByRole("button", { name: t.off })).toBeEnabled();
  });

  it("覚えていたカメラがなければ、既定のカメラでやり直す", async () => {
    useScanSessionStore.setState({ cameraDeviceId: "gone" });
    getUserMedia
      .mockRejectedValueOnce(domError("OverconstrainedError"))
      .mockResolvedValueOnce(makeStream().stream);
    render(<CameraPanel listening onDigits={vi.fn()} />);

    turnOn();
    await waitFor(() => expect(screen.getByText(t.reading)).toBeInTheDocument());
    expect(getUserMedia).toHaveBeenCalledTimes(2);
    const first = getUserMedia.mock.calls[0]?.[0].video as MediaTrackConstraints;
    const second = getUserMedia.mock.calls[1]?.[0].video as MediaTrackConstraints;
    expect(first.deviceId).toEqual({ exact: "gone" });
    expect(second.deviceId).toBeUndefined();
  });

  it("カメラが 2 台以上のときだけ選択欄を出し、選んだカメラを覚える", async () => {
    getUserMedia.mockResolvedValue(makeStream("cam-a").stream);
    enumerateDevices.mockResolvedValue([device("cam-a", "内蔵カメラ"), device("cam-b", "")]);
    render(<CameraPanel listening onDigits={vi.fn()} />);
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();

    turnOn();
    const select = await screen.findByRole("combobox", { name: t.deviceLabel });
    expect(screen.getByRole("option", { name: "内蔵カメラ" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: t.deviceUnnamed(2) })).toBeInTheDocument();

    getUserMedia.mockResolvedValue(makeStream("cam-b").stream);
    fireEvent.change(select, { target: { value: "cam-b" } });
    expect(useScanSessionStore.getState().cameraDeviceId).toBe("cam-b");
    await waitFor(() => expect(getUserMedia).toHaveBeenCalledTimes(2));
  });

  it("カメラが 1 台なら選択欄は出さない", async () => {
    getUserMedia.mockResolvedValue(makeStream().stream);
    render(<CameraPanel listening onDigits={vi.fn()} />);
    turnOn();
    await waitFor(() => expect(screen.getByText(t.reading)).toBeInTheDocument());
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
  });
});
