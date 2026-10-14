<script lang="ts">
  // Простые контурные иконки (24×24, цвет — currentColor).
  let { name, size = 18 }: { name: "play" | "profile" | "records" | "settings" | "library" | "refresh" | "stop"; size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  {#if name === "play"}
    <rect x="3" y="7" width="18" height="11" rx="5.5" />
    <path d="M8 10.5v4M6 12.5h4" />
    <circle cx="15.5" cy="11.5" r="0.6" fill="currentColor" />
    <circle cx="17.5" cy="13.5" r="0.6" fill="currentColor" />
  {:else if name === "profile"}
    <path d="M5 6h9M18 6h1M5 12h3M12 12h7M5 18h11M20 18h-1" />
    <circle cx="16" cy="6" r="2" />
    <circle cx="10" cy="12" r="2" />
    <circle cx="18" cy="18" r="2" />
  {:else if name === "records"}
    <circle cx="12" cy="12" r="8" />
    <circle cx="12" cy="12" r="3" />
  {:else if name === "settings"}
    <circle cx="12" cy="12" r="3" />
    <path d="M12 2.5v3M12 18.5v3M21.5 12h-3M5.5 12h-3M18.7 5.3l-2.1 2.1M7.4 16.6l-2.1 2.1M18.7 18.7l-2.1-2.1M7.4 7.4 5.3 5.3" />
  {:else if name === "library"}
    <path d="M4 5h4v14H4zM10 5h4v14h-4zM16.5 5.5l3.5 1-3 12.5-3.5-1z" />
  {:else if name === "refresh"}
    <path d="M20 11a8 8 0 1 0-2.3 5.7M20 5v6h-6" />
  {:else if name === "stop"}
    <path d="M5 9v6M9 7v10M13 9v6M17 11v2" />
  {/if}
</svg>
