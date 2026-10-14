<script lang="ts">
  // MIDI-монитор: что пришло с геймпада и что программа отправила — синтезатору и в MIDI-выход.
  import { onMount } from "svelte";
  import { api, type MonitorEntry, type MonitorInput, type PadFamily } from "$lib/api";
  import { keyLabel, noteName } from "$lib/labels";

  let { family = "xbox" }: { family?: PadFamily } = $props();

  type Filter = "all" | "in" | "out";
  let filter = $state<Filter>("all");
  const FILTERS: [Filter, string][] = [
    ["all", "Всё"],
    ["in", "Входы"],
    ["out", "Выходы"],
  ];
  const AXIS_NAMES: Record<string, string> = {
    left_x: "Левый стик ↔",
    left_y: "Левый стик ↕",
    right_x: "Правый стик ↔",
    right_y: "Правый стик ↕",
  };

  function describeInput(i: MonitorInput): { ch: string; kind: string; data: string } {
    if (i.kind === "button") {
      return { ch: "", kind: i.down ? "▼ Кнопка" : "▲ Кнопка", data: keyLabel({ kind: "button", button: i.button }, family) };
    }
    const name = i.axis.endsWith("trigger")
      ? keyLabel({ kind: "axis_switch", axis: i.axis, direction: "positive" }, family)
      : (AXIS_NAMES[i.axis] ?? i.axis);
    const v = i.value;
    return { ch: "", kind: "Ось", data: `${name} · ${v > 0 ? "+" : ""}${v.toFixed(2)}` };
  }


  const MAX = 300;
  const CC_NAMES: Record<number, string> = {
    0: "Банк",
    1: "Mod Wheel",
    7: "Громкость",
    10: "Панорама",
    11: "Экспрессия",
    32: "Банк (LSB)",
    64: "Педаль",
    71: "Резонанс",
    72: "Затухание",
    73: "Атака",
    74: "Яркость",
    91: "Реверб",
    93: "Хорус",
    101: "RPN (MSB)",
    100: "RPN (LSB)",
    6: "Data Entry",
    38: "Data Entry (LSB)",
    120: "All Sound Off",
    123: "All Notes Off",
  };

  let rows = $state<MonitorEntry[]>([]);
  const shown = $derived(filter === "all" ? rows : rows.filter((r) => (filter === "in") === !!r.input));
  let paused = $state(false);
  let since = 0;
  let box: HTMLDivElement;

  function describe(b: number[]): { ch: string; kind: string; data: string } {
    const s = b[0];
    const ch = s < 0xf0 ? `CH${(s & 0x0f) + 1}` : "";
    switch (s & 0xf0) {
      case 0x90:
        if (b[2] > 0) return { ch, kind: "Note On", data: `${noteName(b[1])} (${b[1]}) · vel ${b[2]}` };
        return { ch, kind: "Note Off", data: `${noteName(b[1])} (${b[1]})` };
      case 0x80:
        return { ch, kind: "Note Off", data: `${noteName(b[1])} (${b[1]})` };
      case 0xb0:
        return { ch, kind: `CC${b[1]}`, data: `${CC_NAMES[b[1]] ? CC_NAMES[b[1]] + " · " : ""}${b[2]}` };
      case 0xc0:
        return { ch, kind: "Program", data: `${b[1] + 1}` };
      case 0xd0:
        return { ch, kind: "Pressure", data: `${b[1]}` };
      case 0xe0: {
        const v = (b[2] << 7) + b[1] - 8192;
        return { ch, kind: "Pitch Bend", data: v > 0 ? `+${v}` : `${v}` };
      }
    }
    if (s === 0xfa) return { ch, kind: "Start", data: "" };
    if (s === 0xfb) return { ch, kind: "Continue", data: "" };
    if (s === 0xfc) return { ch, kind: "Stop", data: "" };
    if (s === 0xf0 && b[1] === 0x7f && b[3] === 0x06) return { ch, kind: "MMC", data: b[4] === 6 ? "Record" : b[4] === 1 ? "Stop" : `команда ${b[4]}` };
    return { ch, kind: "Системное", data: b.map((x) => x.toString(16).padStart(2, "0")).join(" ") };
  }

  async function poll() {
    const fresh = await api.midiMonitor(since);
    if (!fresh.length) return;
    since = fresh[fresh.length - 1].seq;
    if (paused) return;
    rows = [...rows, ...fresh].slice(-MAX);
    queueMicrotask(() => box && (box.scrollTop = box.scrollHeight));
  }

  onMount(() => {
    const t = setInterval(poll, 100);
    poll();
    return () => clearInterval(t);
  });
</script>

<div class="monitor">
  <div class="tools">
    <button type="button" class="ghost" aria-pressed={paused} onclick={() => (paused = !paused)}>{paused ? "▶ Продолжить" : "❚❚ Пауза"}</button>
    <button type="button" class="ghost" onclick={() => (rows = [])}>Очистить</button>
    <div class="seg filter">
      {#each FILTERS as [f, label] (f)}
        <button type="button" class:on={filter === f} aria-pressed={filter === f} onclick={() => (filter = f)}>{label}</button>
      {/each}
    </div>
    <span class="muted small">Входы геймпада и то, что уходит синтезатору и в MIDI-выход. MIDI Clock не показывается.</span>
  </div>
  <div class="log" bind:this={box} role="log" aria-live="off">
    {#each shown as r (r.seq)}
      {@const d = r.input ? describeInput(r.input) : describe(r.bytes)}
      <div class="row" class:note={d.kind.startsWith("Note On")} class:from-pad={!!r.input}>
        <span class="t">{(r.ms / 1000).toFixed(3)}</span>
        <span class="ch">{d.ch}</span>
        <span class="kind">{d.kind}</span>
        <span class="data">{d.data}</span>
      </div>
    {:else}
      <p class="muted empty">Пока тихо — нажми что-нибудь на геймпаде.</p>
    {/each}
  </div>
</div>

<style>
  .monitor {
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 100%;
    min-height: 140px;
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .ghost {
    min-height: 30px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 13px;
  }
  .log {
    flex: 1;
    min-height: 120px;
    max-height: 260px;
    overflow-y: auto;
    background: #121218;
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 6px 10px;
    font-family: var(--font-mono);
    font-size: 12px;
    user-select: text;
  }
  .row {
    display: grid;
    grid-template-columns: 70px 44px 96px 1fr;
    gap: 8px;
    padding: 1px 0;
    color: var(--muted);
  }
  .row .kind,
  .row .data {
    color: var(--text);
  }
  .row.note .kind {
    color: var(--accent);
  }
  .row.from-pad .kind,
  .row.from-pad .data {
    color: var(--mod);
  }
  .filter > button {
    flex: 0 0 auto;
  }
  .small {
    font-size: 12px;
  }
  .empty {
    font-family: var(--font);
  }
</style>
