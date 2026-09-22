# M9-1 — The performance bench: what is measured, and what fails a run

Status: accepted (2026-09-22)

## Context

Plan M9-1 gives Drift five budgets and one gate:

| Budget | Value |
|---|---|
| frame rate at 1280×800 | 60 fps with `anim.py` |
| frame rate at 2560×1600 | ≥ 55 fps |
| decode + present | < 8 ms p95 |
| input to wire | < 2 ms p99 |
| RSS per session | ≤ 300 MB |

"`cargo xtask e2e --bench` reports these; a regression of more than 10 % fails the nightly run."

Nothing measured them: `SessionStats::frame_latency_p95_ms` was hard-coded to `0.0` and there
was no input-latency number at all.

## Decision

### What each number means

* **decode + present** is measured from the moment a graphics payload comes **off the wire**
  (`drift_rdp::actor`, before `ActiveStage::process`) to the moment the tab's `FrameSink`
  reports the frame **presented** — the same callback that sends the `FrameAcknowledge`. It
  therefore covers ZGFX, the codec, the compositor and the present, which is exactly the
  budget's wording. The measurement lives in `drift_rdp::graphics::SharedSink::end_frame`,
  which wraps the `presented` callback; the samples are drained once per statistics tick.
* **input to wire** is measured in the actor from the arrival of a `SessionCommand::Input` to
  the completed `write_all` of its fast-path frame. Encoding is included; the greeter typist
  and the post-reconnect `ReleaseAll` are not (they are not user input).
* Percentiles are **nearest rank** over a bounded reservoir (`drift_rdp::stats::Percentiles`,
  4096 samples, oldest dropped). One statistics window holds about 60 samples.
* **fps** is the best one-second sample of the measurement window, as in the M1-6 render e2e.
  The stream is 60 Hz, so the peak is the honest reading of "does the client keep up".
* **latency** is the **median** one-second sample, with the worst second kept as a note. The
  dev machine builds other work while the bench runs, and a single contended second would
  otherwise decide the number.
* **RSS per session** is the resident-set growth of the test process from before the session
  to after the measurement (`ps -o rss=`), with the process total kept as a note.

### The gate

`xtask::bench` compares the run with `tests/e2e/bench-baseline.json` and fails on

1. a **missing** metric — a silent gate is worse than a red one;
2. a **budget miss**;
3. a **regression of more than 10 %** that is also larger than the metric's **noise floor**.

The noise floor is the addition the plan does not mention, and it is needed: the measured
run-to-run spread on this pair of machines is 1.3–2.5 ms for decode+present and 0.04–0.27 ms
for input-to-wire, so a pure 10 % rule would fail nightly runs that changed nothing. The floors
(3 fps, 2 ms, 0.5 ms, 10 MB) are those measured spreads.

### Budget waivers

A budget the *reference host* cannot deliver is waived in the baseline with a written reason
(`exceptions` in the JSON). The number is still reported, still printed with its reason, and
the 10 % regression rule still guards it — only the absolute budget stops failing the run. A
waiver is a committed decision, so `--update-baseline` never drops it and a test requires every
waiver to carry an explanation.

## Measured (2026-09-22, homelab + macOS 27 M4 Pro, headless mode, `anim.py` full-screen)

| Metric | Measured | Budget | Verdict |
|---|---|---|---|
| fps at 1280×800 | 60.6–61.0 | ≥ 60 | met |
| fps at 2560×1600 | 29.9–32.0 (peak), 23–24 sustained | ≥ 55 | **waived, host-bound** |
| decode + present p95 | 1.36 ms idle machine, 2.38 ms while building | < 8 ms | met, 3–6× headroom |
| input to wire p99 | 0.043 ms idle, 0.17 ms while building | < 2 ms | met, 12–45× headroom |
| RSS per session | 38–42 MB (48–52 MB process total) | ≤ 300 MB | met, 7× headroom |

### Why 2560×1600 is the host, not Drift

* Drift's **unacknowledged-frame queue is 0** in every one-second sample at both resolutions.
  g-r-d only throttles once it reaches `max(2, min(rtt·60+2, 60))` (plan §1.4), so the client
  is never the thing the server is waiting for.
* **decode + present stays at 1.4–2.4 ms p95** at 2560×1600 — hundreds of frames per second of
  headroom on a stream delivering 24.
* The host's **pixel throughput rises** when the desktop quadruples in area: 1280×800 × 60 fps
  = 61 Mpixel/s, 2560×1600 × 23.5 fps = 96 Mpixel/s. It is already doing 57 % more work.
* On the host during that window, a `gnome-remote-desktop` process peaks at 87 % of a core and
  `anim.py` (the content generator) at 40–60 %, on a 4-core Ryzen 5 3400GE with Vega 11.

Nothing on the client side can raise that ceiling. The budget is kept in `xtask::bench` as the
plan states it, so a faster reference host (or a fix in g-r-d) lights it up again.

## Consequences

* `SessionStats` gained `input_to_wire_p99_ms`, and `frame_latency_p95_ms` now carries a real
  number — the statistics HUD's "ms" field stops reading 0.0.
* `cargo xtask e2e --bench [--update-baseline]` is the only way to refresh the baseline, and it
  prints the full table whether it passes or fails.
* The bench runs one headless session; it says nothing about many tabs at once (M6) or about
  Remote Login's three legs. Those stay in the per-milestone e2e tests.
