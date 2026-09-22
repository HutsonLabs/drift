// Red stub.
import type { ErrorAction, ErrorExplanation } from "../bindings";

export function renderError(
  _root: HTMLElement,
  _explanation: ErrorExplanation,
  _profileName: string,
  _act: (action: ErrorAction) => void,
): void {
  throw new Error("Red: not implemented yet");
}
