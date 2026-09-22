// Red stub.
import type { commands, SessionView_Serialize } from "./bindings";

export type Api = Pick<
  typeof commands,
  | "listProfiles"
  | "newProfile"
  | "validateProfile"
  | "saveProfile"
  | "deleteProfile"
  | "forgetCertificate"
  | "openLocalNetworkSettings"
  | "connect"
  | "acceptCertificate"
  | "rejectCertificate"
  | "reconnectNow"
  | "cancelReconnect"
  | "closeSession"
>;

export class DriftApp {
  constructor(
    readonly root: HTMLElement,
    readonly api: Api,
  ) {}

  async start(): Promise<void> {
    throw new Error("Red: not implemented yet");
  }

  onSessionView(_view: SessionView_Serialize): void {
    throw new Error("Red: not implemented yet");
  }

  dispose(): void {}
}
