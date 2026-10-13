<script lang="ts">
  import "@fontsource/unbounded/latin-700.css";
  import "@fontsource/unbounded/cyrillic-700.css";
  import "@fontsource/golos-text/latin-400.css";
  import "@fontsource/golos-text/cyrillic-400.css";
  import "@fontsource/golos-text/latin-600.css";
  import "@fontsource/golos-text/cyrillic-600.css";
  import "@fontsource/jetbrains-mono/latin-700.css";
  import "../app.css";

  let { children } = $props();
</script>

{@render children()}
