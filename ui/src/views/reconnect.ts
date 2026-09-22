// Red stub.
import type { SessionView_Serialize } from "../bindings";

export function secondsLeft(_nextInMs: number, _elapsedMs: number): number {
  throw new Error("Red: not implemented yet");
}

export function reconnectMessage(_seconds: number): string {
  throw new Error("Red: not implemented yet");
}

export function renderReconnectOverlay(
  _root: HTMLElement,
  _view: SessionView_Serialize,
  _elapsedMs: number,
  _on: { now(): void; cancel(): void },
): void {
  throw new Error("Red: not implemented yet");
}
