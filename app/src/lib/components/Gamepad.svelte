<script lang="ts">
  // Схема геймпада: на каждой кнопке — что она делает прямо сейчас.
  import type { ControlView, Input, PadFamily, Profile, Status, SwitchKey } from "$lib/api";
  import { roleLabel, sameInput, sameKey } from "$lib/describe";
  import { keyLabel } from "$lib/labels";

  let { status, family, profile }: { status: Status; family: PadFamily; profile: Profile | null } = $props();

  const W = 640;
  const H = 460;
  let width = $state(W);
  const scale = $derived(Math.min(1, width / W));

  const btn = (button: string): Input => ({ kind: "button", button });

  function view(input: Input): ControlView | undefined {
    return status.controls.find((v) => sameInput(v.input, input));
  }

  function keyOf(input: Input): SwitchKey | null {
    if (input.kind === "button") return { kind: "button", button: input.button };
    if (input.kind === "axis_switch") return { kind: "axis_switch", axis: input.axis, direction: input.direction };
    return null;
  }

  function isPressed(input: Input): boolean {
    const k = keyOf(input);
    if (!k) return false;
    return status.pressed.some((p) => sameKey(p, k));
  }

  /** Состояние кнопки для стиля: звучит / модификатор включён / нажата / покой. */
  function stateOf(input: Input): "note" | "mod" | "pressed" | "idle" {
    const v = view(input);
    const pressed = isPressed(input);
    if (v?.role === "modifier" && (v.active || pressed)) return "mod";
    if (pressed) return v?.role === "note" ? "note" : "pressed";
    return "idle";
  }

  function label(input: Input): string {
    const v = view(input);
    return v ? roleLabel(v, profile) : "";
  }

  // ---------- курки ----------
  function triggerInfo(axis: "left_trigger" | "right_trigger") {
    const sw: Input = { kind: "axis_switch", axis, direction: "positive", threshold: 0 };
    const ax: Input = { kind: "axis", axis };
    const main = view(sw) ? sw : ax;
    return {
      glyph: keyLabel({ kind: "axis_switch", axis, direction: "positive" }, family),
      label: label(main),
      state: stateOf(sw),
      value: status.axes[axis] ?? 0,
    };
  }
  const lt = $derived(triggerInfo("left_trigger"));
  const rt = $derived(triggerInfo("right_trigger"));

  // ---------- стики ----------
  function stickInfo(side: "left" | "right") {
    const x = `${side}_x`;
    const y = `${side}_y`;
    const parts: string[] = [];
    const lx = label({ kind: "axis", axis: x });
    const ly = label({ kind: "axis", axis: y });
    if (lx) parts.push(`↔ ${lx}`);
    if (ly) parts.push(`↕ ${ly}`);
    for (const [axis, dir, arrow] of [
      [y, "positive", "↑"],
      [y, "negative", "↓"],
      [x, "negative", "←"],
      [x, "positive", "→"],
    ] as const) {
      const l = label({ kind: "axis_switch", axis, direction: dir, threshold: 0 });
      if (l) parts.push(`${arrow} ${l}`);
    }
    const click = btn(`${side}_stick`);
    return {
      glyph: keyLabel({ kind: "button", button: `${side}_stick` }, family),
      move: parts.join(" · "),
      click: label(click),
      clickState: stateOf(click),
      x: status.axes[x] ?? 0,
      y: status.axes[y] ?? 0,
    };
  }
  const ls = $derived(stickInfo("left"));
  const rs = $derived(stickInfo("right"));

  // ---------- крестовина и кнопки ----------
  const DPAD = [
    { button: "dpad_up", x: 56, y: 0 },
    { button: "dpad_left", x: 0, y: 56 },
    { button: "dpad_right", x: 112, y: 56 },
    { button: "dpad_down", x: 56, y: 112 },
  ];
  const FACE = [
    { button: "north", x: 62, y: 0 },
    { button: "west", x: 0, y: 62 },
    { button: "east", x: 124, y: 62 },
    { button: "south", x: 62, y: 124 },
  ];

  const FACE_COLORS: Record<PadFamily, Record<string, string>> = {
    xbox: { north: "var(--face-y)", west: "var(--face-x)", east: "var(--face-b)", south: "var(--face-a)" },
    generic: { north: "var(--face-y)", west: "var(--face-x)", east: "var(--face-b)", south: "var(--face-a)" },
    play_station: { north: "#5fd3a6", west: "#f08ac0", east: "#ff7b7b", south: "#7aa8ff" },
    nintendo: { north: "var(--muted)", west: "var(--muted)", east: "var(--muted)", south: "var(--muted)" },
  };

  // Назначенные входы, которых нет на схеме, — списком под ней
  const DRAWN = new Set([
    ...DPAD.map((d) => d.button),
    ...FACE.map((f) => f.button),
    "left_shoulder",
    "right_shoulder",
    "back",
    "start",
    "left_stick",
    "right_stick",
  ]);
  const extras = $derived(
    status.controls.filter((v) => v.input.kind === "button" && !DRAWN.has(v.input.button)),
  );
</script>

{#snippet face(input: Input, glyph: string, x: number, y: number, round: boolean, glyphColor?: string)}
  {@const st = stateOf(input)}
  {@const v = view(input)}
  <div
    class="cell {st}"
    class:round
    class:unbound={!v}
    style="left: {x}px; top: {y}px"
    title={v ? `${glyph}: ${label(input)}` : `${glyph}: не назначено`}
  >
    <small style={glyphColor && st === "idle" ? `color: ${glyphColor}` : ""}>{glyph}</small>
    <b class:long={label(input).length > 4}>{label(input) || "·"}</b>
  </div>
{/snippet}

{#snippet bumper(button: string, side: "left" | "right")}
  {@const input = btn(button)}
  <div class="bumper {stateOf(input)} {side}">
    <b>{keyLabel({ kind: "button", button }, family)}</b><span>{label(input) || "—"}</span>
  </div>
{/snippet}

{#snippet trigger(t: ReturnType<typeof triggerInfo>, side: "left" | "right")}
  <div class="trigger {t.state} {side}">
    <div class="fill" style="width: {Math.round(t.value * 100)}%"></div>
    <b>{t.glyph}</b><span>{t.label || "—"}</span>
  </div>
{/snippet}

{#snippet stick(s: ReturnType<typeof stickInfo>, left: number)}
  <div class="stick {s.clickState}" style="left: {left}px">
    <span class="glyph">{s.glyph}</span>
    <div class="dot" style="left: {42 + s.x * 28 - 8}px; top: {42 - s.y * 28 - 8}px"></div>
  </div>
  <div class="stick-label" style="left: {left + 42 - 110}px">
    {#if s.move}<div>{s.move}</div>{/if}
    {#if s.click}<div class="click">нажатие — {s.click}</div>{/if}
  </div>
{/snippet}

<div class="wrap" bind:clientWidth={width} style="height: {H * scale}px">
  <div class="pad" style="width: {W}px; height: {H}px; transform: scale({scale})">
    {@render trigger(lt, "left")}
    {@render trigger(rt, "right")}
    {@render bumper("left_shoulder", "left")}
    {@render bumper("right_shoulder", "right")}
    <div class="body"></div>

    <div class="group" style="left: 92px; top: 128px">
      {#each DPAD as d (d.button)}
        {@render face(btn(d.button), keyLabel({ kind: "button", button: d.button }, family), d.x, d.y, false)}
      {/each}
    </div>

    <div class="group" style="left: 374px; top: 116px">
      {#each FACE as f (f.button)}
        {@render face(btn(f.button), keyLabel({ kind: "button", button: f.button }, family), f.x, f.y, true, FACE_COLORS[family][f.button])}
      {/each}
    </div>

    <div class="center">
      {#each ["back", "start"] as b (b)}
        {@const input = btn(b)}
        <div class="small {stateOf(input)}" title={keyLabel({ kind: "button", button: b }, family)}>
          <div class="pill"></div>
          <small>{label(input) || keyLabel({ kind: "button", button: b }, family)}</small>
        </div>
      {/each}
    </div>

    {@render stick(ls, 196)}
    {@render stick(rs, 360)}
  </div>
</div>

{#if extras.length}
  <div class="extras">
    {#each extras as v (JSON.stringify(v.input))}
      {#if v.input.kind === "button"}
        <span class="chip">
          <b>{keyLabel({ kind: "button", button: v.input.button }, family)}</b>
          {roleLabel(v, profile)}
        </span>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .wrap {
    width: 100%;
    display: flex;
    justify-content: center;
  }
  .pad {
    position: relative;
    flex-shrink: 0;
    transform-origin: top center;
  }
  .body {
    position: absolute;
    left: 20px;
    top: 90px;
    width: 600px;
    height: 350px;
    border-radius: 150px 150px 190px 190px / 120px 120px 180px 180px;
    background: var(--raised);
    border: 1px solid var(--line-strong);
  }
  .group {
    position: absolute;
    width: 180px;
    height: 180px;
  }

  .cell {
    position: absolute;
    width: 56px;
    height: 56px;
    border-radius: 10px;
    background: var(--key);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    transition:
      background 60ms,
      box-shadow 60ms;
  }
  .cell.round {
    border-radius: 50%;
  }
  .cell small {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
  }
  .cell b {
    font-size: 17px;
  }
  .cell b.long {
    font-size: 10px;
    font-weight: 600;
    text-align: center;
    line-height: 1.1;
    padding: 0 3px;
  }
  .cell.unbound b {
    color: var(--muted);
  }

  .note {
    background: var(--accent);
    color: var(--bg);
    box-shadow: 0 0 0 6px color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .cell.note small {
    color: var(--bg);
  }
  .mod {
    background: var(--mod-soft);
    border-color: var(--mod) !important;
    box-shadow: inset 0 0 0 1px var(--mod);
  }
  .pressed {
    background: var(--line-strong);
  }

  .bumper,
  .trigger {
    position: absolute;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    border: 1px solid var(--line-strong);
    background: #1d1d26;
    font-size: 14px;
    white-space: nowrap;
  }
  .bumper {
    top: 46px;
    width: 144px;
    height: 34px;
    border-radius: 10px;
  }
  .bumper.left {
    left: 80px;
  }
  .bumper.right {
    right: 80px;
  }
  .bumper b,
  .trigger b {
    font-size: 13px;
    color: var(--muted);
  }
  .bumper.mod b,
  .trigger.mod b {
    color: var(--mod);
  }
  .trigger {
    top: 0;
    width: 124px;
    height: 38px;
    border-radius: 14px 14px 6px 6px;
    overflow: hidden;
  }
  .trigger.left {
    left: 90px;
  }
  .trigger.right {
    right: 90px;
  }
  .trigger .fill {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: var(--accent-soft);
  }
  .trigger b,
  .trigger span {
    position: relative;
  }

  .center {
    position: absolute;
    left: 236px;
    top: 150px;
    width: 168px;
    display: flex;
    justify-content: space-between;
  }
  .small {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 84px;
    background: none !important;
    box-shadow: none !important;
  }
  .small .pill {
    width: 30px;
    height: 18px;
    border-radius: 9px;
    background: var(--key);
  }
  .small.pressed .pill,
  .small.note .pill {
    background: var(--accent);
  }
  .small small {
    white-space: nowrap;
    font-size: 11px;
    color: var(--muted);
    text-align: center;
  }

  .stick {
    position: absolute;
    top: 312px;
    width: 84px;
    height: 84px;
    border-radius: 50%;
    background: var(--key);
    border: 1px solid var(--line-strong);
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
  }
  .stick .dot {
    position: absolute;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0.9;
  }
  .stick .glyph {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    opacity: 0.6;
  }
  .stick.mod {
    border-color: var(--mod);
    background: var(--mod-soft);
  }
  .stick-label {
    position: absolute;
    top: 402px;
    width: 220px;
    text-align: center;
    font-size: 12px;
    line-height: 1.4;
    color: var(--muted);
  }

  .extras {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
    margin-top: 8px;
    font-size: 13px;
  }
  .extras b {
    color: var(--muted);
  }
</style>
