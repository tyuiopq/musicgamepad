<script lang="ts">
  // График отклика оси: по горизонтали — отклонение, по вертикали — выход.
  // Повторяет shape_value из engine/src/mapper.rs.
  import type { AxisShape } from "$lib/api";

  let { shape, value = 0 }: { shape: AxisShape; value?: number } = $props();

  const W = 160;
  const H = 72;

  function out(raw: number): number {
    const v = shape.invert ? -raw : raw;
    const dz = Math.min(Math.max(shape.deadzone, 0), 0.95);
    const a = Math.abs(v);
    const s = a < dz ? 0 : Math.min((a - dz) / (1 - dz), 1);
    const c = Math.sign(v) * Math.pow(s, Math.max(shape.curve, 0.05));
    switch (shape.mode) {
      case "bipolar":
        return c;
      case "absolute":
        return Math.abs(c);
      case "positive":
        return Math.max(c, 0);
      case "negative":
        return Math.max(-c, 0);
    }
  }

  const bipolar = $derived(shape.mode === "bipolar");
  const y = (o: number) => (bipolar ? H / 2 - (o * H) / 2 : H - o * H);
  const x = (raw: number) => ((raw + 1) / 2) * W;
  const path = $derived(
    Array.from({ length: 81 }, (_, i) => {
      const raw = -1 + i / 40;
      return `${i ? "L" : "M"}${x(raw).toFixed(1)},${y(out(raw)).toFixed(1)}`;
    }).join(" "),
  );
</script>

<svg viewBox="0 0 {W} {H}" width={W} height={H} role="img" aria-label="Кривая отклика оси">
  <rect x="0" y="0" width={W} height={H} rx="6" class="bg" />
  <line x1={W / 2} y1="0" x2={W / 2} y2={H} class="axis" />
  <line x1="0" y1={y(0)} x2={W} y2={y(0)} class="axis" />
  <path d={path} class="curve" />
  <circle cx={x(value)} cy={y(out(value))} r="4" class="dot" />
</svg>

<style>
  .bg {
    fill: var(--bg);
    stroke: var(--line-strong);
  }
  .axis {
    stroke: var(--line-strong);
    stroke-width: 1;
  }
  .curve {
    fill: none;
    stroke: var(--mod);
    stroke-width: 2;
  }
  .dot {
    fill: var(--accent);
  }
</style>
