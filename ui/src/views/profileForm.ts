// Connect / profile form (M1-6, M3-2): renders a FormModel, emits intents. The connection mode
// decides which credential fields exist:
// - Remote Login: the *system* RDP credentials, an optional Linux user (greeter hint) and an
//   explicit opt-in to store the Linux password so Drift types it at the GNOME login screen.
// - Headless / Desktop Sharing: one RDP credential set.
// Validation happens in Rust (`validate_profile` / `save_profile`); issues come back in the model.
import type {
  ClipboardPrefs,
  CmdAs,
  ConnectionProfile_Serialize,
  ConnectMode,
  ProfileField,
  ProfileIssue,
  SecretsUpdate,
} from "../bindings";
import { actionButton, h, mount } from "../dom";

/** Everything the form renders. */
export interface FormModel {
  /** The draft profile being edited. */
  profile: ConnectionProfile_Serialize;
  /** Not saved yet. */
  isNew: boolean;
  /** An RDP password is stored for this profile. */
  hasRdpPassword: boolean;
  /** A Linux greeter password is stored (the user opted in). */
  hasLinuxPassword: boolean;
  /** Opt-in checkbox state: store the Linux password and type it at the greeter. */
  typeLinuxPassword: boolean;
  /** Typed (unsaved) RDP password; empty = keep the stored one. */
  rdpPassword: string;
  /** Typed (unsaved) Linux password; empty = keep the stored one. */
  linuxPassword: string;
  /** Per-field problems from Rust. */
  issues: ProfileIssue[];
  /** A form-level error (storage, backend). */
  error: string | null;
  /** A save is in flight. */
  busy: boolean;
}

/** The form's current values. */
export interface FormDraft {
  profile: ConnectionProfile_Serialize;
  rdpPassword: string;
  linuxPassword: string;
  typeLinuxPassword: boolean;
}

/** What the form can ask for. */
export interface FormIntents {
  /** A value that changes the form's shape changed (mode, opt-in): re-render with `draft`. */
  change(draft: FormDraft): void;
  save(draft: FormDraft): void;
  cancel(): void;
  remove(): void;
  forgetCertificate(): void;
}

/** A fresh model for `profile`. */
export function formModel(
  profile: ConnectionProfile_Serialize,
  opts: { isNew: boolean; hasRdpPassword: boolean; hasLinuxPassword: boolean },
): FormModel {
  return {
    profile,
    isNew: opts.isNew,
    hasRdpPassword: opts.hasRdpPassword,
    hasLinuxPassword: opts.hasLinuxPassword,
    typeLinuxPassword: opts.hasLinuxPassword,
    rdpPassword: "",
    linuxPassword: "",
    issues: [],
    error: null,
    busy: false,
  };
}

/** Password changes to send with `save_profile`. */
export function toSecretsUpdate(draft: FormDraft, hasLinuxPassword: boolean): SecretsUpdate {
  const rdp_password = draft.rdpPassword === "" ? null : draft.rdpPassword;
  if (draft.profile.mode !== "remote-login") return { rdp_password, linux_password: { action: "forget" } };
  if (!draft.typeLinuxPassword) {
    return { rdp_password, linux_password: hasLinuxPassword ? { action: "forget" } : { action: "keep" } };
  }
  if (draft.linuxPassword !== "") return { rdp_password, linux_password: { action: "store", password: draft.linuxPassword } };
  return { rdp_password, linux_password: { action: "keep" } };
}

const MODES: { mode: ConnectMode; label: string; about: string }[] = [
  {
    mode: "remote-login",
    label: "Remote Login",
    about: "Log in at the GNOME login screen, as if you were at the computer. After a reconnect you log in again and return to your running session.",
  },
  {
    mode: "headless",
    label: "Headless session",
    about: "A GNOME session that keeps running on the host without a screen. Reconnects go straight back into it.",
  },
  {
    mode: "desktop-sharing",
    label: "Desktop Sharing",
    about: "Show and control a screen that someone is already logged in to.",
  },
];

const CREDENTIALS: Record<ConnectMode, { legend: string; user: string; password: string; hint: string }> = {
  "remote-login": {
    legend: "System RDP credentials",
    user: "System RDP user",
    password: "System RDP password",
    hint: "The Remote Login credentials set on the host with “sudo grdctl --system rdp set-credentials”. They open the GNOME login screen; you then log in with your own account.",
  },
  headless: {
    legend: "RDP credentials",
    user: "RDP user",
    password: "RDP password",
    hint: "The credentials set for that user on the host with “grdctl --headless rdp set-credentials”.",
  },
  "desktop-sharing": {
    legend: "Desktop Sharing credentials",
    user: "RDP user",
    password: "RDP password",
    hint: "Set on the host in GNOME Settings › System › Remote Desktop › Desktop Sharing.",
  },
};

const FIELD_IDS: Record<ProfileField, string> = {
  name: "profile-name",
  host: "host",
  port: "port",
  "rdp-username": "rdp-user",
  "linux-username": "linux-user",
};

const KEEP = "Saved — leave blank to keep";

/** Renders the form into `root`, replacing its contents. */
export function renderProfileForm(root: HTMLElement, model: FormModel, on: FormIntents): void {
  const p = model.profile;
  const issueFor = (f: ProfileField) => model.issues.find((i) => i.field === f);
  const cred = CREDENTIALS[p.mode];
  const isLogin = p.mode === "remote-login";
  const needsPassword = model.isNew || !model.hasRdpPassword;

  const form = h("form", { class: "profile-form", novalidate: true, "aria-labelledby": "form-title" });
  const draft = () => readDraft(form, p);
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    if (!model.busy) on.save(draft());
  });

  const modeGroup = h(
    "fieldset",
    { class: "mode" },
    h("legend", {}, "Connection type"),
    h(
      "div",
      { class: "segmented" },
      MODES.map((m) =>
        h(
          "label",
          { class: "segment" },
          h("input", {
            type: "radio",
            name: "mode",
            value: m.mode,
            checked: m.mode === p.mode,
            onchange: () => on.change(draft()),
          }),
          m.label,
        ),
      ),
    ),
    h("p", { class: "hint", id: "mode-hint" }, MODES.find((m) => m.mode === p.mode)?.about ?? ""),
  );

  const credentials = h(
    "fieldset",
    { class: "credentials", "aria-describedby": "credentials-hint" },
    h("legend", {}, cred.legend),
    h("p", { class: "hint", id: "credentials-hint" }, cred.hint),
    textField("rdp-user", cred.user, p.rdp_username, issueFor("rdp-username"), { autocomplete: "username" }),
    textField("rdp-password", cred.password, model.rdpPassword, undefined, {
      type: "password",
      autocomplete: "current-password",
      required: needsPassword,
      placeholder: needsPassword ? "Required" : KEEP,
    }),
  );

  const greeter = isLogin
    ? h(
        "fieldset",
        { class: "greeter" },
        h("legend", {}, "GNOME login screen"),
        textField("linux-user", "Linux user (optional)", p.linux_username ?? "", issueFor("linux-username"), {
          autocomplete: "off",
          hint: "Shown as a reminder while the login screen is open.",
        }),
        h(
          "label",
          { class: "check" },
          h("input", {
            type: "checkbox",
            id: "type-linux-password",
            checked: model.typeLinuxPassword,
            "aria-describedby": "type-linux-password-hint",
            onchange: () => on.change(draft()),
          }),
          "Type my Linux password at the login screen",
        ),
        h(
          "p",
          { class: "hint", id: "type-linux-password-hint" },
          "Off by default. When on, the password is kept in your Keychain and Drift types it into the GNOME password field for you.",
        ),
        model.typeLinuxPassword &&
          textField("linux-password", "Linux password", model.linuxPassword, undefined, {
            type: "password",
            autocomplete: "off",
            required: !model.hasLinuxPassword,
            placeholder: model.hasLinuxPassword ? KEEP : "Required",
          }),
      )
    : null;

  const pin = p.cert_pin
    ? h(
        "div",
        { class: "pin" },
        h("span", { class: "pin-label", id: "pin-label" }, "Trusted certificate (SHA-256)"),
        h("code", { class: "fingerprint", "aria-labelledby": "pin-label" }, p.cert_pin),
        actionButton("Forget Certificate", () => on.forgetCertificate()),
      )
    : null;

  const sharing = p.mode === "desktop-sharing";
  const advanced = h(
    "details",
    { class: "advanced" },
    h("summary", {}, "Keyboard, display and clipboard"),
    selectField("cmd-as", "Command key sends", p.keyboard.cmd_as ?? "super", [
      ["super", "Super (Windows key)"],
      ["ctrl", "Control"],
    ]),
    checkField("mac-layout", "Type using Mac layout", p.keyboard.type_with_mac_layout ?? false,
      "Sends typed characters instead of key positions, so the Mac keyboard layout applies."),
    checkField("adaptive", "Resize the remote desktop to fit the window", sharing ? false : (p.display.adaptive ?? true),
      sharing ? "Desktop Sharing can’t resize the remote screen; Drift scales it to fit." : null, sharing),
    checkField("retina", "Use Retina resolution", p.display.retina ?? true,
      "Renders GNOME at 2× on Retina displays for sharp text."),
    selectField("clipboard", "Clipboard", p.clipboard ?? "text-and-images", [
      ["text-and-images", "Text and images"],
      ["text", "Text only"],
      ["off", "Off"],
    ]),
  );

  const actions = h(
    "div",
    { class: "actions" },
    !model.isNew && h("button", { type: "button", class: "destructive", onclick: () => on.remove() }, "Delete"),
    h("span", { class: "spacer" }),
    h("button", { type: "button", onclick: () => on.cancel() }, "Cancel"),
    h("button", { type: "submit", class: "primary", disabled: model.busy, "aria-busy": model.busy ? "true" : null }, "Save"),
  );

  form.append(
    h("h1", { id: "form-title" }, model.isNew ? "New Connection" : p.name || "Connection"),
    model.error ? h("p", { class: "form-error", role: "alert" }, model.error) : "",
    textField("profile-name", "Name", p.name, issueFor("name"), { autocomplete: "off", placeholder: "Homelab" }),
    modeGroup,
    h(
      "div",
      { class: "row" },
      textField("host", "Host", p.host, issueFor("host"), {
        autocomplete: "off",
        placeholder: "gnome.local or 192.168.1.20",
        spellcheck: "false",
        class: "grow",
      }),
      textField("port", "Port", String(p.port), issueFor("port"), { type: "number", min: "1", max: "65535", class: "port" }),
    ),
    credentials,
    greeter ?? "",
    pin ?? "",
    advanced,
    actions,
  );
  mount(root, form);
}

interface FieldOpts {
  type?: string;
  autocomplete?: string;
  placeholder?: string;
  required?: boolean;
  hint?: string;
  min?: string;
  max?: string;
  spellcheck?: string;
  class?: string;
}

function textField(id: string, label: string, value: string, issue: ProfileIssue | undefined, o: FieldOpts = {}): HTMLElement {
  const describedBy = [o.hint ? `${id}-hint` : null, issue ? `${id}-error` : null].filter(Boolean).join(" ");
  return h(
    "div",
    { class: ["field", o.class].filter(Boolean).join(" ") },
    h("label", { for: id }, label),
    h("input", {
      id,
      name: id,
      type: o.type ?? "text",
      value,
      autocomplete: o.autocomplete,
      placeholder: o.placeholder,
      required: o.required ?? false,
      min: o.min,
      max: o.max,
      spellcheck: o.spellcheck,
      "aria-invalid": issue ? "true" : null,
      "aria-describedby": describedBy || null,
    }),
    o.hint ? h("p", { class: "hint", id: `${id}-hint` }, o.hint) : "",
    issue ? h("p", { class: "field-error", id: `${id}-error` }, issue.message) : "",
  );
}

function checkField(id: string, label: string, checked: boolean, hint: string | null, disabled = false): HTMLElement {
  return h(
    "div",
    { class: "field check-field" },
    h(
      "label",
      { class: "check" },
      h("input", { type: "checkbox", id, checked, disabled, "aria-describedby": hint ? `${id}-hint` : null }),
      label,
    ),
    hint ? h("p", { class: "hint", id: `${id}-hint` }, hint) : "",
  );
}

function selectField(id: string, label: string, value: string, options: [string, string][]): HTMLElement {
  const select = h("select", { id, name: id }, options.map(([v, text]) => h("option", { value: v }, text)));
  select.value = value;
  return h("div", { class: "field" }, h("label", { for: id }, label), select);
}

function input(form: HTMLFormElement, id: string): HTMLInputElement | null {
  return form.querySelector<HTMLInputElement>(`[id="${id}"]`);
}

/** Reads the form's current values over `base` (fields that are not rendered keep `base`). */
export function readDraft(form: HTMLFormElement, base: ConnectionProfile_Serialize): FormDraft {
  const val = (id: string, fallback: string) => input(form, id)?.value ?? fallback;
  const checked = (id: string, fallback: boolean) => {
    const el = input(form, id);
    return el ? el.checked : fallback;
  };
  const mode = (form.querySelector<HTMLInputElement>("input[name=mode]:checked")?.value ?? base.mode) as ConnectMode;
  const port = Number.parseInt(val("port", String(base.port)), 10);
  const linuxUser = val("linux-user", base.linux_username ?? "").trim();
  const sharing = mode === "desktop-sharing";
  const adaptiveEl = input(form, "adaptive");
  const profile: ConnectionProfile_Serialize = {
    ...base,
    name: val("profile-name", base.name),
    host: val("host", base.host).trim(),
    port: Number.isFinite(port) && port >= 0 && port <= 65535 ? port : 0,
    mode,
    rdp_username: val("rdp-user", base.rdp_username),
    linux_username: mode === "remote-login" && linuxUser !== "" ? linuxUser : null,
    keyboard: {
      cmd_as: val("cmd-as", base.keyboard.cmd_as ?? "super") as CmdAs,
      type_with_mac_layout: checked("mac-layout", base.keyboard.type_with_mac_layout ?? false),
    },
    display: {
      // Desktop Sharing shows the box disabled; keep the stored preference untouched.
      adaptive: sharing || !adaptiveEl || adaptiveEl.disabled ? (base.display.adaptive ?? true) : adaptiveEl.checked,
      retina: checked("retina", base.display.retina ?? true),
    },
    clipboard: val("clipboard", base.clipboard ?? "text-and-images") as ClipboardPrefs,
  };
  return {
    profile,
    rdpPassword: val("rdp-password", ""),
    linuxPassword: val("linux-password", ""),
    typeLinuxPassword: mode === "remote-login" && checked("type-linux-password", false),
  };
}
