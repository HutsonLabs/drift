# Manual acceptance checklist

Steps that cannot be automated (they need a human, a GUI permission prompt, physical network
changes or a clean machine). Each milestone's owner appends its "Done (manual …)" items here.

## M0 — Foundation

- [ ] `cargo tauri dev` opens a window titled "Drift" showing the connections screen ("Add a GNOME computer" on first launch).

## M1-6 / M3-2 / M7-3 / M9-4 — Connect UI (stream D)

Rendering and intents are covered by `bun test`; these need eyes, VoiceOver or a real prompt.

- [ ] Light and dark appearance (System Settings › Appearance): the connections screen, the
      certificate prompt and the error screens use system colours and stay readable in both.
- [ ] VoiceOver (Cmd+F5): every field of the profile form is announced with its label; the
      connection type is announced as a radio group “Connection type”; an invalid field is
      announced as invalid together with its message; the certificate prompt is announced as an
      alert dialog with its title; the reconnect overlay countdown is announced politely.
- [ ] Switching Connection type between Remote Login, Headless session and Desktop Sharing
      swaps the credential fields without losing the typed name and host.
- [ ] Certificate prompt for the homelab system daemon shows the same fingerprint as
      `sudo grdctl --system status` (“TLS fingerprint: f3:e7:a2:…”) on 10.1.2.40.
- [ ] Local Network Privacy screen (a freshly ad-hoc-signed build connecting to 10.1.2.40
      directly, errno 65): “Open Local Network Settings” opens System Settings › Privacy &
      Security › Local Network.
