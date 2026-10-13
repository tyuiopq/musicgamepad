<script lang="ts">
  // Лента последних нот: клавиатура слева, ноты — полосы на высоте своей клавиши.
  import { onMount } from "svelte";

  let { notes, tonic }: { notes: number[]; tonic: number } = $props();

  const WINDOW_MS = 10_000;
  const KEYS_W = 44;
  const BLACK = new Set([1, 3, 6, 8, 10]);

  interface Played {
    note: number;
    start: number;
    end: number | null;
  }

  let history: Played[] = [];
  let canvas: HTMLCanvasElement;
  let box: HTMLDivElement;
  let raf = 0;

  // Сравниваем звучащие ноты с прошлым состоянием: новые начинаются, пропавшие заканчиваются
  $effect(() => {
    const now = performance.now();
    const current = new Set(notes);
    for (const p of history) {
      if (p.end === null && !current.has(p.note)) p.end = now;
    }
    for (const n of current) {
      if (!history.some((p) => p.note === n && p.end === null)) history.push({ note: n, start: now, end: null });
    }
    wake();
  });

  function wake() {
    if (!raf) raf = requestAnimationFrame(draw);
  }

  function css(name: string): string {
    return getComputedStyle(box).getPropertyValue(name).trim();
  }

  function draw() {
    raf = 0;
    const now = performance.now();
    history = history.filter((p) => p.end === null || now - p.end < WINDOW_MS);

    const dpr = window.devicePixelRatio || 1;
    const w = box.clientWidth;
    const h = box.clientHeight;
    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    }
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    // Диапазон: две октавы — от октавы ниже тоники до тоники; расширяется под сыгранное
    let lo = tonic - 12;
    let hi = tonic + 11;
    for (const p of history) {
      lo = Math.min(lo, p.note);
      hi = Math.max(hi, p.note);
    }
    const rows = hi - lo + 1;
    const rowH = h / rows;
    const y = (n: number) => (hi - n) * rowH;
    const sounding = new Set(history.filter((p) => p.end === null).map((p) => p.note));

    const bg = css("--bg");
    const accent = css("--accent");
    const muted = css("--muted");

    // дорожки
    for (let n = lo; n <= hi; n++) {
      if (BLACK.has(((n % 12) + 12) % 12)) {
        ctx.fillStyle = "#0e0e13";
        ctx.fillRect(KEYS_W, y(n), w - KEYS_W, rowH);
      }
      if (n % 12 === 0) {
        ctx.fillStyle = "#2a2a35";
        ctx.fillRect(KEYS_W, y(n) + rowH - 1, w - KEYS_W, 1);
      }
    }

    // клавиши
    for (let n = lo; n <= hi; n++) {
      const black = BLACK.has(((n % 12) + 12) % 12);
      ctx.fillStyle = sounding.has(n) ? accent : black ? "#23232d" : "#d9d9e2";
      ctx.fillRect(0, y(n), KEYS_W, rowH - 0.5);
      if (n % 12 === 0 && rowH >= 6) {
        ctx.fillStyle = sounding.has(n) ? bg : "#101015";
        ctx.font = `600 ${Math.min(10, rowH)}px "Golos Text", sans-serif`;
        ctx.textBaseline = "middle";
        ctx.textAlign = "right";
        ctx.fillText(`C${Math.floor(n / 12) - 1}`, KEYS_W - 4, y(n) + rowH / 2);
      }
    }

    // ноты: правый край — «сейчас»
    const pxPerMs = (w - KEYS_W) / WINDOW_MS;
    const r = Math.min(3, rowH / 2);
    for (const p of history) {
      const x0 = Math.max(KEYS_W, w - (now - p.start) * pxPerMs);
      const x1 = p.end === null ? w : w - (now - p.end) * pxPerMs;
      if (x1 <= KEYS_W) continue;
      ctx.fillStyle = p.end === null ? accent : muted;
      ctx.globalAlpha = p.end === null ? 1 : 0.55;
      ctx.beginPath();
      ctx.roundRect(x0, y(p.note) + 1, Math.max(2, x1 - x0), Math.max(1, rowH - 2), r);
      ctx.fill();
      ctx.globalAlpha = 1;
    }

    ctx.fillStyle = css("--text");
    ctx.fillRect(w - 2, 0, 2, h);

    // Перерисовываем, пока в окне есть ноты; иначе спим до следующей
    if (history.length) wake();
  }

  onMount(() => {
    const ro = new ResizeObserver(() => wake());
    ro.observe(box);
    wake();
    return () => {
      ro.disconnect();
      cancelAnimationFrame(raf);
    };
  });
</script>

<div class="roll" bind:this={box}>
  <canvas bind:this={canvas} aria-label="Последние сыгранные ноты"></canvas>
</div>

<style>
  .roll {
    position: relative;
    height: 100%;
    min-height: 140px;
    border-radius: 8px;
    overflow: hidden;
    border: 1px solid var(--line);
    background: #121218;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
</style>
