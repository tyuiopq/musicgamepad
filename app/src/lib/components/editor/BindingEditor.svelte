<script lang="ts">
  // Настройка одного входа: роль (нота, аккорд, модификатор, команда, контроллер) и её параметры.
  import type { Action, AxisMode, AxisShape, Binding, ChordKind, Command, ControlTarget, ModEffect, Profile } from "$lib/api";
  import type { Slot } from "$lib/controls";
  import { DEFAULT_THRESHOLD } from "$lib/controls";
  import { effectLabel } from "$lib/describe";
  import { noteName, solfege } from "$lib/labels";
  import { CHORD_DEGREES, degreeNote, scaleSteps } from "$lib/music";
  import { defaultAction, roleOf, type Role } from "$lib/profileEdit";
  import { drumName, isDrums } from "$lib/sound";
  import ShapeCurve from "./ShapeCurve.svelte";

  let {
    slot,
    binding,
    inherited,
    inheritedLayer,
    profile,
    axisValue = 0,
    onchange,
    onnewlayer,
  }: {
    slot: Slot;
    binding: Binding | undefined;
    inherited: Binding | undefined;
    inheritedLayer: string | null;
    profile: Profile;
    axisValue?: number;
    onchange: (b: Binding | null) => void;
    onnewlayer: () => string;
  } = $props();

  const action = $derived(binding?.action ?? null);
  const role = $derived(roleOf(action));
  const threshold = $derived(binding?.input.kind === "axis_switch" ? binding.input.threshold : DEFAULT_THRESHOLD);

  const ROLES = $derived<[Role, string][]>(
    slot.continuous
    ? [
        ["none", "Ничего"],
        ["control", "Контроллер"],
      ]
    : [
        ["none", "Ничего"],
        ["note", "Нота"],
        ["chord", "Аккорд"],
        ["modifier", "Модификатор"],
        ["command", "Команда"],
      ],
  );

  function emit(next: Action | null, thr = threshold) {
    if (!next) return onchange(null);
    const input = slot.input.kind === "axis_switch" ? { ...slot.input, threshold: thr } : slot.input;
    onchange({ input, action: next });
  }

  /** Копия текущего действия для правки. */
  function edit(fn: (a: Action) => void) {
    if (!action) return;
    const a = $state.snapshot(action) as Action;
    fn(a);
    emit(a);
  }

  function setRole(r: Role) {
    if (r === role) return;
    emit(defaultAction(r, slot.input));
  }

  function overrideInherited() {
    if (inherited) emit($state.snapshot(inherited.action) as Action);
  }

  // ---------- ноты ----------
  function degreeText(d: number): string {
    const n = scaleSteps(profile.music).length;
    const step = (((d % n) + n) % n) + 1;
    const oct = Math.floor(d / n);
    const shift = oct === 0 ? "" : oct > 0 ? `, +${oct} окт.` : `, ${oct} окт.`;
    return `${step}-я ступень${shift}`;
  }
  const notePreview = (n: number) => `${noteName(n)} · ${solfege(n)}`;

  const CHORDS: [ChordKind, string][] = [
    ["triad", "Трезвучие"],
    ["seventh", "Септаккорд"],
    ["power", "Квинта"],
  ];

  // ---------- модификаторы ----------
  const EFFECTS: [ModEffect["type"], string][] = [
    ["semitones", "Полутоны"],
    ["octaves", "Октава"],
    ["degrees", "Ступени"],
    ["layer", "Слой"],
    ["velocity", "Сила"],
    ["sustain", "Педаль"],
  ];
  const otherLayers = $derived(profile.layers.filter((l) => l.id !== "base"));

  function setEffect(type: ModEffect["type"]) {
    edit((a) => {
      if (a.kind !== "modifier") return;
      a.effect =
        type === "semitones"
          ? { type, semitones: 1 }
          : type === "octaves"
            ? { type, octaves: 1 }
            : type === "degrees"
              ? { type, degrees: 1 }
              : type === "layer"
                ? { type, layer: otherLayers[0]?.id ?? onnewlayer() }
                : type === "velocity"
                  ? { type, velocity: 120 }
                  : { type: "sustain" };
    });
  }

  // ---------- команды ----------
  const COMMANDS: [Command["type"], string][] = [
    ["next_instrument", "Следующий инструмент"],
    ["prev_instrument", "Предыдущий инструмент"],
    ["toggle_record", "Запись: старт/стоп"],
    ["panic", "Тишина (заглушить всё)"],
    ["toggle_metronome", "Метроном: вкл/выкл"],
    ["loop_record", "Лупер: записать / наложить слой"],
    ["loop_undo", "Лупер: отменить слой"],
    ["loop_clear", "Лупер: очистить"],
  ];

  function setCommand(type: Command["type"]) {
    edit((a) => {
      if (a.kind !== "command") return;
      a.command = type === "next_instrument" || type === "prev_instrument" ? { type, part: 0 } : ({ type } as Command);
    });
  }

  // ---------- контроллеры ----------
  type TargetKey = "pitch_bend" | "cc1" | "cc11" | "cc74" | "cc64" | "cc" | "velocity";
  const TARGETS: [TargetKey, string][] = [
    ["pitch_bend", "Подтяжка"],
    ["cc1", "Вибрато"],
    ["cc11", "Громкость"],
    ["cc74", "Яркость"],
    ["cc64", "Педаль"],
    ["cc", "Другой CC"],
    ["velocity", "Сила нажатия"],
  ];
  const PRESET_CC = [1, 11, 74, 64];

  function targetKey(t: ControlTarget): TargetKey {
    if (t.type === "pitch_bend") return "pitch_bend";
    if (t.type === "velocity") return "velocity";
    return PRESET_CC.includes(t.cc) ? (`cc${t.cc}` as TargetKey) : "cc";
  }

  function setTarget(k: TargetKey) {
    edit((a) => {
      if (a.kind !== "control") return;
      if (k === "pitch_bend") a.target = { type: "pitch_bend" };
      else if (k === "velocity") a.target = { type: "velocity", min: 30, max: 127 };
      else if (k === "cc") a.target = { type: "cc", cc: 2, min: 0, max: 127 };
      else a.target = { type: "cc", cc: Number(k.slice(2)), min: k === "cc11" ? 40 : 0, max: 127 };
    });
  }

  function setShape(fn: (s: AxisShape) => void) {
    edit((a) => {
      if (a.kind === "control") fn(a.shape);
    });
  }

  const isStickAxis = $derived(slot.input.kind === "axis" && !slot.input.axis.endsWith("trigger"));
  const isVertical = $derived(slot.input.kind === "axis" && slot.input.axis.endsWith("_y"));
  const MODES = $derived<[AxisMode, string][]>(
    isStickAxis
      ? [
          ["bipolar", "В обе стороны"],
          ["absolute", "От центра"],
          ["positive", isVertical ? "Только вверх" : "Только вправо"],
          ["negative", isVertical ? "Только вниз" : "Только влево"],
        ]
      : [
          ["positive", "Нажатие"],
          ["bipolar", "От середины"],
        ],
  );

  const actionPart = $derived(action && (action.kind === "note" || action.kind === "control") ? action.part : 0);
  const bendRange = $derived(profile.parts[actionPart]?.bend_range ?? 2);
  const drumPart = $derived(isDrums(profile.parts[actionPart]));

  /** Сменить партию; для ударных нота становится конкретной (бочка), т.к. лад им не подходит. */
  function setPart(i: number) {
    edit((a) => {
      if (a.kind !== "note" && a.kind !== "control") return;
      a.part = i;
      if (a.kind === "note" && isDrums(profile.parts[i]) && a.note.type !== "absolute") {
        a.note = { type: "absolute", note: 36 };
      }
    });
  }

  function noteText(n: number): string {
    const drum = drumPart ? drumName(n) : null;
    return drum ? `${drum} (${n})` : notePreview(n);
  }
</script>

{#snippet stepper(value: number, min: number, max: number, set: (v: number) => void, text: string)}
  <div class="stepper">
    <button type="button" aria-label="Меньше" disabled={value <= min} onclick={() => set(value - 1)}>−</button>
    <span>{text}</span>
    <button type="button" aria-label="Больше" disabled={value >= max} onclick={() => set(value + 1)}>+</button>
  </div>
{/snippet}

{#snippet seg<T extends string>(options: [T, string][], current: T, set: (v: T) => void)}
  <div class="seg">
    {#each options as [v, text] (v)}
      <button type="button" class:on={v === current} aria-pressed={v === current} onclick={() => set(v)}>{text}</button>
    {/each}
  </div>
{/snippet}

{#snippet range(label: string, value: number, min: number, max: number, step: number, set: (v: number) => void, text: string)}
  <label class="range">
    <span>{label}</span>
    <input type="range" {min} {max} {step} {value} oninput={(e) => set(Number(e.currentTarget.value))} />
    <b>{text}</b>
  </label>
{/snippet}

<section class="slot">
  <h3>{slot.title}</h3>

  {#if !binding && inherited}
    <div class="inherited">
      <span>Из слоя «{inheritedLayer}»</span>
      <button type="button" class="link" onclick={overrideInherited}>Изменить в этом слое</button>
    </div>
  {/if}

  {@render seg(ROLES, role, setRole)}

  {#if profile.parts.length > 1 && (action?.kind === "note" || action?.kind === "control")}
    <label class="part">
      <span>Партия</span>
      <select value={actionPart} onchange={(e) => setPart(Number(e.currentTarget.value))}>
        {#each profile.parts as p, i (i)}<option value={i}>{p.name}</option>{/each}
      </select>
    </label>
  {/if}

  {#if action?.kind === "note" && action.note.type !== "chord"}
    {@const note = action.note}
    {@render seg(
      [
        ["degree", "Ступень лада"],
        ["absolute", "Конкретная нота"],
      ],
      note.type,
      (t) =>
        edit((a) => {
          if (a.kind === "note")
            a.note = t === "degree" ? { type: "degree", degree: 0 } : { type: "absolute", note: degreeNote(profile.music, 0) };
        }),
    )}
    {#if note.type === "degree"}
      {@render stepper(
        note.degree,
        -21,
        28,
        (v) => edit((a) => a.kind === "note" && a.note.type === "degree" && (a.note.degree = v)),
        degreeText(note.degree),
      )}
      <p class="result">{notePreview(degreeNote(profile.music, note.degree))}</p>
    {:else if note.type === "absolute"}
      {@render stepper(
        note.note,
        0,
        127,
        (v) => edit((a) => a.kind === "note" && a.note.type === "absolute" && (a.note.note = v)),
        noteText(note.note),
      )}
      <p class="hint">{drumPart ? "Звук ударной установки." : "Не зависит от лада — удобно для ударных."}</p>
    {/if}
    {@render range("Сила", action.velocity, 1, 127, 1, (v) => edit((a) => a.kind === "note" && (a.velocity = v)), String(action.velocity))}
  {:else if action?.kind === "note" && action.note.type === "chord"}
    {@const note = action.note}
    {@render stepper(
      note.degree,
      -21,
      28,
      (v) => edit((a) => a.kind === "note" && a.note.type === "chord" && (a.note.degree = v)),
      `от ${degreeText(note.degree)}`,
    )}
    {@render seg(CHORDS, note.chord, (c) => edit((a) => a.kind === "note" && a.note.type === "chord" && (a.note.chord = c)))}
    <p class="result">
      {CHORD_DEGREES[note.chord].map((d) => noteName(degreeNote(profile.music, note.degree + d))).join(" · ")}
    </p>
    {@render range("Сила", action.velocity, 1, 127, 1, (v) => edit((a) => a.kind === "note" && (a.velocity = v)), String(action.velocity))}
  {:else if action?.kind === "modifier"}
    {@const effect = action.effect}
    <div class="grid">
      {#each EFFECTS as [type, text] (type)}
        <button type="button" class:on={effect.type === type} aria-pressed={effect.type === type} onclick={() => setEffect(type)}>
          {text}
        </button>
      {/each}
    </div>
    {#if effect.type === "semitones"}
      {@render stepper(
        effect.semitones,
        -12,
        12,
        (v) => edit((a) => a.kind === "modifier" && a.effect.type === "semitones" && (a.effect.semitones = v)),
        effectLabel(effect, profile),
      )}
    {:else if effect.type === "octaves"}
      {@render stepper(
        effect.octaves,
        -4,
        4,
        (v) => edit((a) => a.kind === "modifier" && a.effect.type === "octaves" && (a.effect.octaves = v)),
        effectLabel(effect, profile),
      )}
    {:else if effect.type === "degrees"}
      {@render stepper(
        effect.degrees,
        -14,
        14,
        (v) => edit((a) => a.kind === "modifier" && a.effect.type === "degrees" && (a.effect.degrees = v)),
        effectLabel(effect, profile),
      )}
    {:else if effect.type === "layer"}
      <select
        value={effect.layer}
        onchange={(e) => {
          const id = e.currentTarget.value === "__new" ? onnewlayer() : e.currentTarget.value;
          edit((a) => a.kind === "modifier" && a.effect.type === "layer" && (a.effect.layer = id));
        }}
      >
        {#each otherLayers as l (l.id)}<option value={l.id}>{l.name}</option>{/each}
        <option value="__new">+ Новый слой</option>
      </select>
    {:else if effect.type === "velocity"}
      {@render range(
        "Сила",
        effect.velocity,
        1,
        127,
        1,
        (v) => edit((a) => a.kind === "modifier" && a.effect.type === "velocity" && (a.effect.velocity = v)),
        String(effect.velocity),
      )}
    {:else}
      <p class="hint">Ноты продолжают звучать после отпускания кнопок.</p>
    {/if}
    {@render seg(
      [
        ["hold", "Пока зажата"],
        ["toggle", "Переключатель"],
      ],
      action.latch,
      (l) => edit((a) => a.kind === "modifier" && (a.latch = l)),
    )}
  {:else if action?.kind === "command"}
    {@const command = action.command}
    <select value={command.type} onchange={(e) => setCommand(e.currentTarget.value as Command["type"])}>
      {#each COMMANDS as [type, text] (type)}<option value={type}>{text}</option>{/each}
    </select>
    {#if (command.type === "next_instrument" || command.type === "prev_instrument")}
      {#if profile.parts.length > 1}
        <label class="part">
          <span>Партия</span>
          <select
            value={command.part}
            onchange={(e) => {
              const i = Number(e.currentTarget.value);
              edit((a) => {
                if (a.kind === "command" && (a.command.type === "next_instrument" || a.command.type === "prev_instrument")) a.command.part = i;
              });
            }}
          >
            {#each profile.parts as p, i (i)}<option value={i}>{p.name}</option>{/each}
          </select>
        </label>
      {/if}
      {@const list = profile.parts[command.part]?.instruments ?? []}
      <p class="hint">
        {list.length
          ? `По быстрому списку партии: ${list.length} инстр.`
          : "Быстрый список партии пуст — отметь инструменты звёздочкой во вкладке «Звук»."}
      </p>
    {/if}
  {:else if action?.kind === "control"}
    {@const target = action.target}
    {@const shape = action.shape}
    <select value={targetKey(target)} onchange={(e) => setTarget(e.currentTarget.value as TargetKey)}>
      {#each TARGETS as [k, text] (k)}<option value={k}>{text}</option>{/each}
    </select>
    {#if target.type === "cc" && targetKey(target) === "cc"}
      {@render stepper(target.cc, 0, 127, (v) => edit((a) => a.kind === "control" && a.target.type === "cc" && (a.target.cc = v)), `CC ${target.cc}`)}
    {/if}
    {#if target.type === "pitch_bend"}
      <p class="hint">Диапазон подтяжки: ±{bendRange} полутона (меняется в настройках звука).</p>
    {:else}
      {@render range("От", target.min, 0, 127, 1, (v) => edit((a) => a.kind === "control" && a.target.type !== "pitch_bend" && (a.target.min = v)), String(target.min))}
      {@render range("До", target.max, 0, 127, 1, (v) => edit((a) => a.kind === "control" && a.target.type !== "pitch_bend" && (a.target.max = v)), String(target.max))}
    {/if}
    {@render seg(MODES, shape.mode, (m) => setShape((s) => (s.mode = m)))}
    <div class="curve">
      <ShapeCurve {shape} value={axisValue} />
      <div class="curve-controls">
        {@render range("Мёртвая зона", shape.deadzone, 0, 0.5, 0.01, (v) => setShape((s) => (s.deadzone = v)), `${Math.round(shape.deadzone * 100)}%`)}
        {@render range("Кривая", shape.curve, 0.3, 3, 0.05, (v) => setShape((s) => (s.curve = v)), shape.curve.toFixed(2))}
        <label class="check">
          <input type="checkbox" checked={shape.invert} onchange={(e) => setShape((s) => (s.invert = e.currentTarget.checked))} />
          Наоборот
        </label>
      </div>
    </div>
  {/if}

  {#if slot.input.kind === "axis_switch" && action}
    {@render range(
      "Порог",
      threshold,
      0.1,
      0.95,
      0.05,
      (v) => emit($state.snapshot(action) as Action, v),
      `${Math.round(threshold * 100)}%`,
    )}
  {/if}
</section>

<style>
  .slot {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 0;
    border-top: 1px solid var(--line);
  }
  .slot:first-child {
    border-top: none;
    padding-top: 0;
  }
  h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .part {
    display: grid;
    grid-template-columns: 96px 1fr;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--muted);
  }
  .inherited {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 13px;
    color: var(--muted);
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    color: var(--mod);
    font-size: 13px;
  }
  .seg,
  .grid {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
  .seg button,
  .grid button {
    flex: 1 1 auto;
    min-height: 32px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--muted);
    font-size: 13px;
  }
  .seg button.on,
  .grid button.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }
  .stepper {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .stepper button {
    width: 36px;
    height: 36px;
    flex-shrink: 0;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
    font-size: 18px;
  }
  .stepper button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .stepper span {
    flex: 1;
    text-align: center;
  }
  .result {
    margin: 0;
    font-family: var(--font-display);
    font-size: 18px;
    color: var(--accent);
    text-align: center;
  }
  .hint {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
  }
  .range {
    display: grid;
    grid-template-columns: 96px 1fr 44px;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--muted);
  }
  .range b {
    color: var(--text);
    font-weight: 600;
    text-align: right;
  }
  .range input {
    accent-color: var(--accent);
    min-width: 0;
  }
  select {
    min-height: 36px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
  }
  .curve {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
  }
  .curve-controls {
    flex: 1 1 180px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 13px;
    color: var(--muted);
  }
</style>
