// Red stub.
import type { ConnectionProfile_Serialize, ProfileIssue, SecretsUpdate } from "../bindings";

export interface FormModel {
  profile: ConnectionProfile_Serialize;
  isNew: boolean;
  hasRdpPassword: boolean;
  hasLinuxPassword: boolean;
  typeLinuxPassword: boolean;
  rdpPassword: string;
  linuxPassword: string;
  issues: ProfileIssue[];
  error: string | null;
  busy: boolean;
}

export interface FormDraft {
  profile: ConnectionProfile_Serialize;
  rdpPassword: string;
  linuxPassword: string;
  typeLinuxPassword: boolean;
}

export interface FormIntents {
  change(draft: FormDraft): void;
  save(draft: FormDraft): void;
  cancel(): void;
  remove(): void;
  forgetCertificate(): void;
}

export function formModel(
  _profile: ConnectionProfile_Serialize,
  _opts: { isNew: boolean; hasRdpPassword: boolean; hasLinuxPassword: boolean },
): FormModel {
  throw new Error("Red: not implemented yet");
}

export function renderProfileForm(_root: HTMLElement, _model: FormModel, _on: FormIntents): void {
  throw new Error("Red: not implemented yet");
}

export function toSecretsUpdate(_draft: FormDraft, _hasLinuxPassword: boolean): SecretsUpdate {
  throw new Error("Red: not implemented yet");
}
