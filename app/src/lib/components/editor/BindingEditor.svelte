<script lang="ts">
  // Настройка одного входа: роль (нота, аккорд, модификатор, команда, контроллер) и её параметры.
  import { api, type Action, type AxisMode, type AxisShape, type Binding, type ChordKind, type Command, type ControlTarget, type Latch, type ModEffect, type PresetInfo, type Profile } from "$lib/api";
  import type { Slot } from "$lib/controls";
  import { DEFAULT_THRESHOLD } from "$lib/controls";
  import { effectLabel } from "$lib/describe";
  import { noteName, solfege } from "$lib/labels";
  import { CHORD_DEGREES, degreeNote, scaleSteps } from "$lib/music";
  import { defaultAction, roleOf, type Role } from "$lib/profileEdit.svelte";
  import { drumName, isDrums, presetName } from "$lib/sound";
  import { chordNotes as chordNotesOf, learnMessages as learnOf } from "$lib/learn";
  import ShapeCurve from "./ShapeCurve.svelte";

  let {
    slot,
    binding,
    inherited,
    inheritedLayer,
    profile,
    axisValue = 0,
    presets = [],
    midiPort = null,
    onchange,
    onnewlayer,
  }: {
    slot: Slot;
    binding: Binding | undefined;
    inherited: Binding | undefined;
    inheritedLayer: string | null;
    profile: Profile;
    axisValue?: number;
    presets?: PresetInfo[];
    /** Куда сейчас уходит MIDI — для подсказки у кнопки MIDI Learn. */
    midiPort?: string | null;
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
        ["cc", "CC"],
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

  // ---------- кривые осей ----------
  type CurvePreset = "linear" | "exp" | "log" | "s" | "custom";
  const CURVES: [CurvePreset, string][] = [
    ["linear", "Линейная"],
    ["exp", "Экспонента"],
    ["log", "Логарифм"],
    ["s", "S-кривая"],
    ["custom", "Своя"],
  ];
  function curvePreset(s: AxisShape): CurvePreset {
    if (s.curve_kind === "s") return "s";
    if (Math.abs(s.curve - 1) < 0.001) return "linear";
    if (Math.abs(s.curve - 2) < 0.001) return "exp";
    if (Math.abs(s.curve - 0.5) < 0.001) return "log";
    return "custom";
  }
  function applyCurve(s: AxisShape, c: CurvePreset) {
    s.curve_kind = c === "s" ? "s" : "power";
    if (c === "linear") s.curve = 1;
    else if (c === "exp") s.curve = 2;
    else if (c === "log") s.curve = 0.5;
    else if (c === "s") s.curve = 2.5;
    else if ([1, 2, 0.5].includes(s.curve)) s.curve = 1.5;
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
    ["ninth", "Нонаккорд"],
    ["sixth", "Секстаккорд"],
    ["sus2", "Sus2"],
    ["sus4", "Sus4"],
    ["power", "Квинта (power)"],
    ["custom", "Свои интервалы"],
  ];

  const chordNotes = (note: Extract<Action, { kind: "note" }>["note"]) => chordNotesOf(profile, note);

  function parseIntervals(text: string): number[] {
    return text
      .split(/[\s,;]+/)
      .map(Number)
      .filter((n) => Number.isInteger(n) && n >= -24 && n <= 36);
  }

  const LATCH: [Latch, string][] = [
    ["hold", "Пока зажата"],
    ["toggle", "Защёлка"],
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
    ["set_program", "Выбрать инструмент (Program Change)"],
    ["loop_record", "Лупер: записать / наложить слой"],
    ["loop_undo", "Лупер: отменить слой"],
    ["loop_clear", "Лупер: очистить"],
  ];

  function setCommand(type: Command["type"]) {
    edit((a) => {
      if (a.kind !== "command") return;
      if (type === "next_instrument" || type === "prev_instrument") a.command = { type, part: 0 };
      else if (type === "set_program") a.command = { type, part: 0, program: profile.parts[0].program, bank: profile.parts[0].bank };
      else a.command = { type } as Command;
    });
  }

  // ---------- контроллеры ----------
  type TargetKey = "pitch_bend" | "cc1" | "cc11" | "cc74" | "cc64" | "cc" | "velocity" | "pressure";
  const TARGETS: [TargetKey, string][] = [
    ["pitch_bend", "Подтяжка (Pitch Bend)"],
    ["cc1", "Вибрато / Mod Wheel (CC1)"],
    ["cc11", "Экспрессия (CC11)"],
    ["cc74", "Яркость (CC74)"],
    ["cc64", "Педаль (CC64)"],
    ["cc", "Другой CC…"],
    ["pressure", "Давление (Aftertouch)"],
    ["velocity", "Сила следующих нот"],
  ];
  const PRESET_CC = [1, 11, 74, 64];

  function targetKey(t: ControlTarget): TargetKey {
    if (t.type === "pitch_bend") return "pitch_bend";
    if (t.type === "velocity") return "velocity";
    if (t.type === "pressure") return "pressure";
    return PRESET_CC.includes(t.cc) ? (`cc${t.cc}` as TargetKey) : "cc";
  }

  function setTarget(k: TargetKey) {
    edit((a) => {
      if (a.kind !== "control") return;
      if (k === "pitch_bend") a.target = { type: "pitch_bend" };
      else if (k === "velocity") a.target = { type: "velocity", min: 30, max: 127 };
      else if (k === "pressure") a.target = { type: "pressure", min: 0, max: 127 };
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

  const actionPart = $derived(action && (action.kind === "note" || action.kind === "control" || action.kind === "cc") ? action.part : 0);
  const bendRange = $derived(profile.parts[actionPart]?.bend_range ?? 2);
  const drumPart = $derived(isDrums(profile.parts[actionPart]));

  /** Сменить партию; для ударных нота становится конкретной (бочка), т.к. лад им не подходит. */
  function setPart(i: number) {
    edit((a) => {
      if (a.kind !== "note" && a.kind !== "control" && a.kind !== "cc") return;
      a.part = i;
      if (a.kind === "note" && isDrums(profile.parts[i]) && a.note.type !== "absolute") {
        a.note = { type: "absolute", note: 36 };
      }
    });
  }

  // ---------- MIDI Learn ----------
  const learnMessages = () => learnOf(action, profile);
  const canLearn = $derived(learnMessages().length > 0);

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
  {:else if binding && inherited}
    <div class="inherited">
      <span>Перекрывает назначение из слоя «{inheritedLayer}»</span>
      <button type="button" class="link" onclick={() => onchange(null)}>Вернуть как там</button>
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
    {@render seg(LATCH, action.latch ?? "hold", (l) => edit((a) => a.kind === "note" && (a.latch = l)))}
  {:else if action?.kind === "note" && action.note.type === "chord"}
    {@const note = action.note}
    {@render stepper(
      note.degree,
      -21,
      28,
      (v) => edit((a) => a.kind === "note" && a.note.type === "chord" && (a.note.degree = v)),
      `от ${degreeText(note.degree)}`,
    )}
    <select
      aria-label="Вид аккорда"
      value={note.chord}
      onchange={(e) => {
        const c = e.currentTarget.value as ChordKind;
        edit((a) => {
          if (a.kind === "note" && a.note.type === "chord") {
            a.note.chord = c;
            if (c === "custom" && !a.note.intervals?.length) a.note.intervals = [0, 4, 7, 11];
          }
        });
      }}
    >
      {#each CHORDS as [k, text] (k)}<option value={k}>{text}</option>{/each}
    </select>
    {#if note.chord === "custom"}
      <label class="part">
        <span>Интервалы</span>
        <input
          class="text"
          value={(note.intervals ?? []).join(" ")}
          placeholder="0 4 7 11"
          title="Полутоны от основного тона через пробел: 0 4 7 — мажор, 0 3 7 — минор, 0 4 7 11 — maj7"
          onchange={(e) => {
            const iv = parseIntervals(e.currentTarget.value);
            edit((a) => a.kind === "note" && a.note.type === "chord" && (a.note.intervals = iv.length ? iv : [0, 4, 7]));
          }}
        />
      </label>
    {/if}
    {@render stepper(
      note.inversion ?? 0,
      0,
      3,
      (v) => edit((a) => a.kind === "note" && a.note.type === "chord" && (a.note.inversion = v)),
      (note.inversion ?? 0) === 0 ? "Основной вид" : `${note.inversion}-е обращение`,
    )}
    <div class="checks">
      <label class="check">
        <input type="checkbox" checked={note.open ?? false} onchange={(e) => edit((a) => a.kind === "note" && a.note.type === "chord" && (a.note.open = e.currentTarget.checked))} />
        Широкое расположение
      </label>
      <label class="check">
        <input type="checkbox" checked={note.bass ?? false} onchange={(e) => edit((a) => a.kind === "note" && a.note.type === "chord" && (a.note.bass = e.currentTarget.checked))} />
        Бас октавой ниже
      </label>
    </div>
    <p class="result">{chordNotes(note).map(noteName).join(" · ")}</p>
    {@render range("Сила", action.velocity, 1, 127, 1, (v) => edit((a) => a.kind === "note" && (a.velocity = v)), String(action.velocity))}
    {@render seg(LATCH, action.latch ?? "hold", (l) => edit((a) => a.kind === "note" && (a.latch = l)))}
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
    {:else if command.type === "set_program"}
      <select
        aria-label="Инструмент"
        value={`${command.bank}:${command.program}`}
        onchange={(e) => {
          const [bank, program] = e.currentTarget.value.split(":").map(Number);
          edit((a) => {
            if (a.kind === "command" && a.command.type === "set_program") {
              a.command.bank = bank;
              a.command.program = program;
            }
          });
        }}
      >
        {#each presets as p (`${p.bank}:${p.program}`)}
          <option value={`${p.bank}:${p.program}`}>{p.program + 1}. {p.name}{p.bank ? ` (банк ${p.bank})` : ""}</option>
        {:else}
          {#each Array.from({ length: 128 }, (_, i) => i) as i (i)}<option value={`0:${i}`}>Программа {i + 1}</option>{/each}
        {/each}
      </select>
      {#if profile.parts.length > 1}
        <label class="part">
          <span>Партия</span>
          <select
            value={command.part}
            onchange={(e) => {
              const i = Number(e.currentTarget.value);
              edit((a) => a.kind === "command" && a.command.type === "set_program" && (a.command.part = i));
            }}
          >
            {#each profile.parts as p, i (i)}<option value={i}>{p.name}</option>{/each}
          </select>
        </label>
      {/if}
      <p class="hint">Шлёт Program Change: и встроенному звуку, и синту в DAW. Сейчас: {presetName(presets, command.bank, command.program)}.</p>
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
        {@render seg(CURVES, curvePreset(shape), (c) => setShape((s) => applyCurve(s, c)))}
        {#if curvePreset(shape) === "custom" || shape.curve_kind === "s"}
          {@render range(shape.curve_kind === "s" ? "Сила S" : "Кривая", shape.curve, 0.3, 4, 0.05, (v) => setShape((s) => (s.curve = v)), shape.curve.toFixed(2))}
        {/if}
        {@render range("Сглаживание", shape.smoothing ?? 0, 0, 200, 5, (v) => setShape((s) => (s.smoothing = v)), shape.smoothing ? `${shape.smoothing} мс` : "нет")}
        <label class="check">
          <input type="checkbox" checked={shape.invert} onchange={(e) => setShape((s) => (s.invert = e.currentTarget.checked))} />
          Наоборот
        </label>
      </div>
    </div>
    {#if target.type !== "velocity"}
      <label class="check" title="После смены слоя значение не прыгает к положению стика: стик сначала должен «догнать» прежнее значение">
        <input type="checkbox" checked={action.takeover} onchange={(e) => edit((a) => a.kind === "control" && (a.takeover = e.currentTarget.checked))} />
        Подхват (soft takeover)
      </label>
    {/if}
    {#if target.type === "cc" || target.type === "pressure"}
      <label class="check" title="Отклонение задаёт скорость изменения: отпустил стик — значение осталось">
        <input type="checkbox" checked={action.relative} onchange={(e) => edit((a) => a.kind === "control" && (a.relative = e.currentTarget.checked))} />
        Относительный режим: стик крутит значение, а не задаёт его
      </label>
    {/if}
  {:else if action?.kind === "cc"}
    {@const cc = action}
    {@render stepper(cc.cc, 0, 127, (v) => edit((a) => a.kind === "cc" && (a.cc = v)), `CC ${cc.cc}`)}
    {@render seg(
      [
        ["hold", "Пока зажата"],
        ["toggle", "Переключатель"],
        ["step", "Шаг +/−"],
      ],
      cc.step ? "step" : cc.latch,
      (m) =>
        edit((a) => {
          if (a.kind !== "cc") return;
          if (m === "step") a.step = a.step || 8;
          else {
            a.step = 0;
            a.latch = m as Latch;
          }
        }),
    )}
    {#if cc.step}
      {@render stepper(cc.step, -64, 64, (v) => edit((a) => a.kind === "cc" && (a.step = v === 0 ? (cc.step > 0 ? -1 : 1) : v)), `${cc.step > 0 ? "+" : ""}${cc.step} за нажатие`)}
      <p class="hint">Каждое нажатие прибавляет шаг к текущему значению — удобно для «больше/меньше».</p>
    {:else}
      {@render range("Нажата", cc.on, 0, 127, 1, (v) => edit((a) => a.kind === "cc" && (a.on = v)), String(cc.on))}
      {@render range("Отпущена", cc.off, 0, 127, 1, (v) => edit((a) => a.kind === "cc" && (a.off = v)), String(cc.off))}
      <p class="hint">В Ableton: Ctrl+M → кликни Play, Record или любую кнопку → нажми эту кнопку геймпада.</p>
    {/if}
  {/if}

  {#if canLearn}
    <div class="learn">
      <button type="button" class="ghost" onclick={() => api.sendTest(learnMessages())}>Отправить для MIDI Learn</button>
      <span class="hint">
        {midiPort ? `В DAW включи режим обучения, выбери параметр и нажми — уйдёт ровно это сообщение в «${midiPort}».` : "Сначала выбери MIDI-выход в «Настройках»."}
      </span>
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
  .checks {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
  }
  .text {
    font: inherit;
    color: var(--text);
    min-height: 34px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--raised);
    min-width: 0;
  }
  .learn {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-top: 6px;
    border-top: 1px dashed var(--line);
  }
  .ghost {
    align-self: flex-start;
    min-height: 34px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--text);
    font: inherit;
  }
  .ghost:hover {
    border-color: var(--accent);
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
