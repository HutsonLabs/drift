// Red stub.
import type { CertificatePrompt } from "../bindings";

export interface CertificateIntents {
  trust(pin: boolean): void;
  cancel(): void;
}

export function renderCertificatePrompt(
  _root: HTMLElement,
  _prompt: CertificatePrompt,
  _profileName: string,
  _on: CertificateIntents,
): void {
  throw new Error("Red: not implemented yet");
}
