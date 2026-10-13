<script lang="ts">
  // Панель настроек выбранного контрола: все его входы (слоты) в редактируемом слое.
  import type { Binding, Layer, PadFamily, Profile } from "$lib/api";
  import { slotsOf, type ControlId } from "$lib/controls";
  import { keyLabel } from "$lib/labels";
  import { findBinding } from "$lib/profileEdit";
  import BindingEditor from "./BindingEditor.svelte";

  let {
    control,
    layer,
    profile,
    family,
    axes,
    onchange,
    onnewlayer,
  }: {
    control: ControlId;
    layer: Layer;
    profile: Profile;
    family: PadFamily;
    axes: Partial<Record<string, number>>;
    onchange: (slotInput: Binding["input"], b: Binding | null) => void;
    onnewlayer: () => string;
  } = $props();

  const base = $derived(profile.layers[0]);
  const slots = $derived(slotsOf(control));

  const title = $derived.by(() => {
    switch (control.kind) {
      case "button":
        return `Кнопка ${keyLabel({ kind: "button", button: control.button }, family)}`;
      case "trigger":
        return `Курок ${keyLabel({ kind: "axis_switch", axis: control.axis, direction: "positive" }, family)}`;
      case "stick":
        return control.side === "left" ? "Левый стик" : "Правый стик";
    }
  });
</script>

<div class="inspector">
  <header>
    <h2>{title}</h2>
    <span class="muted">слой «{layer.name}»</span>
  </header>
  {#each slots as slot (JSON.stringify(slot.input))}
    {@const axis = slot.input.kind === "button" ? null : slot.input.axis}
    <BindingEditor
      {slot}
      binding={findBinding(layer, slot.input)}
      inherited={layer.id !== "base" ? findBinding(base, slot.input) : undefined}
      inheritedLayer={base.name}
      {profile}
      axisValue={axis ? (axes[axis] ?? 0) : 0}
      onchange={(b) => onchange(slot.input, b)}
      {onnewlayer}
    />
  {/each}
</div>

<style>
  .inspector {
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 12px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  header span {
    font-size: 13px;
  }
</style>
