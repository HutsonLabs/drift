// Controller: holds the webview's small UI state, renders the screen chosen by Rust
// (`SessionView.screen`) or the profiles screen, and turns clicks into IPC intents.
// The API is injected (the generated tauri-specta `commands` in production, a fake in tests).
import type {
  CommandError,
  ConnectionProfile_Serialize,
  ConnectMode,
  ErrorAction,
  ProfileEntry_Serialize,
  SessionView_Serialize,
  commands,
} from "./bindings";
import { h, mount } from "./dom";
import { renderCertificatePrompt } from "./views/certificate";
import { renderConnecting } from "./views/connecting";
import { renderError } from "./views/error";
import { renderGreeterHint } from "./views/greeter";
import { type FormDraft, type FormModel, formModel, renderProfileForm, toSecretsUpdate } from "./views/profileForm";
import { profileList } from "./views/profileList";
import { renderReconnectOverlay } from "./views/reconnect";
import { renderStatsHud } from "./views/stats";

/** The IPC commands the UI uses. */
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
  | "disconnect"
>;

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: CommandError };

/** Human text for a command error. */
export function describeError(e: CommandError): string {
  switch (e.kind) {
    case "invalid":
      return "Please fix the highlighted fields.";
    case "not-found":
      return "This connection no longer exists.";
    case "storage":
    case "platform":
      return e.message;
    case "no-session":
      return "This tab is not connected.";
    case "not-implemented":
      return `${e.what} is not available yet.`;
  }
}

/** Default mode for new connections. */
const DEFAULT_MODE: ConnectMode = "remote-login";

/**
 * Which HUD floats over the live picture, mirroring `present::hud_for` in Rust: the greeter hint
 * banner, the statistics panel, or nothing.
 */
export function hud(view: SessionView_Serialize): "" | "banner" | "stats" {
  if (view.screen === "greeter-hint") return "banner";
  if (view.screen === "live" && view.show_stats && view.stats) return "stats";
  return "";
}

/** The webview controller. */
export class DriftApp {
  private entries: ProfileEntry_Serialize[] = [];
  private form: FormModel | null = null;
  private notice: string | null = null;
  private session: SessionView_Serialize | null = null;
  private sessionAt = 0;
  private activeProfileId: string | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;

  constructor(
    private readonly root: HTMLElement,
    private readonly api: Api,
    private readonly now: () => number = () => Date.now(),
  ) {}

  /** Loads profiles and shows the profiles screen. */
  async start(): Promise<void> {
    await this.reload();
    if (this.entries.length === 0) {
      await this.createProfile(DEFAULT_MODE);
    } else {
      const first = this.entries[0];
      if (first) this.edit(first);
    }
    this.render();
  }

  /** A new view snapshot for this window's session (the `sessionViewChanged` event). */
  onSessionView(view: SessionView_Serialize): void {
    this.session = view;
    this.sessionAt = this.now();
    this.render();
  }

  /** Stops timers. */
  dispose(): void {
    this.stopTimer();
  }

  // ---- profiles -------------------------------------------------------------------------

  private async reload(): Promise<void> {
    const r = (await this.api.listProfiles()) as Result<ProfileEntry_Serialize[]>;
    if (r.status === "ok") {
      this.entries = r.data;
    } else {
      this.notice = describeError(r.error);
    }
  }

  private edit(e: ProfileEntry_Serialize): void {
    this.form = formModel(e.profile, {
      isNew: false,
      hasRdpPassword: e.has_rdp_password,
      hasLinuxPassword: e.has_linux_password,
    });
  }

  private async createProfile(mode: ConnectMode): Promise<void> {
    const p = (await this.api.newProfile(mode)) as ConnectionProfile_Serialize;
    this.form = formModel(p, { isNew: true, hasRdpPassword: false, hasLinuxPassword: false });
  }

  private async save(draft: FormDraft): Promise<void> {
    const form = this.form;
    if (!form) return;
    this.form = { ...form, ...draftFields(draft), busy: true, error: null };
    this.render();
    const r = (await this.api.saveProfile(draft.profile, toSecretsUpdate(draft, form.hasLinuxPassword))) as Result<ProfileEntry_Serialize>;
    if (r.status === "ok") {
      await this.reload();
      this.edit(r.data);
      this.notice = null;
    } else {
      const issues = r.error.kind === "invalid" ? r.error.issues : [];
      this.form = { ...form, ...draftFields(draft), busy: false, issues, error: r.error.kind === "invalid" ? null : describeError(r.error) };
    }
    this.render();
  }

  private async remove(): Promise<void> {
    const form = this.form;
    if (!form || form.isNew) return;
    const r = (await this.api.deleteProfile(form.profile.id)) as Result<null>;
    if (r.status === "error") {
      this.form = { ...form, error: describeError(r.error) };
      this.render();
      return;
    }
    await this.reload();
    const first = this.entries[0];
    if (first) this.edit(first);
    else await this.createProfile(DEFAULT_MODE);
    this.render();
  }

  private async forgetCertificate(): Promise<void> {
    const form = this.form;
    if (!form) return;
    const r = (await this.api.forgetCertificate(form.profile.id)) as Result<ProfileEntry_Serialize>;
    if (r.status === "ok") {
      await this.reload();
      this.form = { ...form, profile: { ...form.profile, cert_pin: null } };
    } else {
      this.form = { ...form, error: describeError(r.error) };
    }
    this.render();
  }

  private async connect(id: string): Promise<void> {
    this.activeProfileId = id;
    this.notice = null;
    await this.intent(this.api.connect(id));
  }

  /** Runs a session intent; errors become the notice on the profiles screen. */
  private async intent(p: Promise<Result<null>>): Promise<void> {
    const r = await p;
    if (r.status === "error") {
      this.notice = describeError(r.error);
      this.render();
    }
  }

  private act(action: ErrorAction): void {
    switch (action) {
      case "reconnect":
        void this.intent(this.api.reconnectNow() as Promise<Result<null>>);
        break;
      case "close":
        void this.intent(this.api.closeSession() as Promise<Result<null>>);
        break;
      case "open-local-network-settings":
        void this.intent(this.api.openLocalNetworkSettings() as Promise<Result<null>>);
        break;
      case "edit-profile": {
        const e = this.entries.find((x) => x.profile.id === this.activeProfileId);
        if (e) this.edit(e);
        this.session = null;
        this.render();
        break;
      }
    }
  }

  // ---- rendering ------------------------------------------------------------------------

  private render(): void {
    const view = this.session;
    const screen = view && view.screen !== "profiles" ? view.screen : "profiles";
    document.body.dataset.screen = screen;
    // Which panel (if any) floats over the live picture; Rust shrinks the web view to match.
    document.body.dataset.hud = view ? hud(view) : "";
    if (screen === "reconnecting") this.startTimer();
    else this.stopTimer();

    if (!view || screen === "profiles") {
      this.renderProfiles();
      return;
    }
    switch (screen) {
      case "live":
        if (view.show_stats && view.stats) renderStatsHud(this.root, view.stats);
        else mount(this.root);
        break;
      case "connecting":
        // Cancelling while connecting ends the session but keeps the tab open.
        renderConnecting(this.root, view, { cancel: () => void this.intent(this.api.disconnect() as Promise<Result<null>>) });
        break;
      case "certificate":
        if (view.certificate) {
          const fp = view.certificate.fingerprint;
          renderCertificatePrompt(this.root, view.certificate, view.profile_name, {
            trust: (pin) => void this.intent(this.api.acceptCertificate(fp, pin) as Promise<Result<null>>),
            cancel: () => void this.intent(this.api.rejectCertificate() as Promise<Result<null>>),
          });
        }
        break;
      case "greeter-hint":
        renderGreeterHint(this.root, view);
        break;
      case "reconnecting":
        this.renderReconnect();
        break;
      case "error":
        if (view.explanation) renderError(this.root, view.explanation, view.profile_name, (a) => this.act(a));
        break;
    }
  }

  private renderReconnect(): void {
    const view = this.session;
    if (!view) return;
    renderReconnectOverlay(this.root, view, this.now() - this.sessionAt, {
      now: () => void this.intent(this.api.reconnectNow() as Promise<Result<null>>),
      cancel: () => void this.intent(this.api.cancelReconnect() as Promise<Result<null>>),
    });
  }

  private startTimer(): void {
    if (this.timer === null) this.timer = setInterval(() => this.renderReconnect(), 250);
  }

  private stopTimer(): void {
    if (this.timer !== null) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }

  private renderProfiles(): void {
    const detail = h("div", { class: "detail" });
    const form = this.form;
    if (form) {
      renderProfileForm(detail, form, {
        change: (d) => {
          this.form = { ...form, ...draftFields(d), issues: [] };
          this.render();
        },
        save: (d) => void this.save(d),
        cancel: () => {
          const e = this.entries.find((x) => x.profile.id === form.profile.id) ?? this.entries[0];
          if (e) this.edit(e);
          else this.form = { ...form, ...formModel(form.profile, { isNew: true, hasRdpPassword: false, hasLinuxPassword: false }) };
          this.render();
        },
        remove: () => void this.remove(),
        forgetCertificate: () => void this.forgetCertificate(),
      });
    }
    const empty = this.entries.length === 0;
    const main = h(
      "div",
      { class: "profiles" },
      empty
        ? h(
            "header",
            { class: "welcome" },
            h("h1", {}, "Add a GNOME computer"),
            h("p", {}, "Drift connects to GNOME Remote Desktop 50 or later. Enter the host and the RDP credentials set on it."),
          )
        : profileList(this.entries, form?.profile.id ?? null, {
            select: (id) => {
              const e = this.entries.find((x) => x.profile.id === id);
              if (e) this.edit(e);
              this.render();
            },
            connect: (id) => void this.connect(id),
            create: () => void this.createProfile(form?.profile.mode ?? DEFAULT_MODE).then(() => this.render()),
          }),
      h(
        "div",
        { class: "content" },
        this.notice ? h("p", { class: "notice", role: "alert" }, this.notice) : "",
        detail,
      ),
    );
    mount(this.root, main);
  }
}

function draftFields(d: FormDraft): Pick<FormModel, "profile" | "rdpPassword" | "linuxPassword" | "typeLinuxPassword"> {
  return { profile: d.profile, rdpPassword: d.rdpPassword, linuxPassword: d.linuxPassword, typeLinuxPassword: d.typeLinuxPassword };
}
