<script lang="ts">
  export let name:
    | "home"
    | "catalog"
    | "projects"
    | "connections"
    | "tools"
    | "settings"
    | "search"
    | "chevron"
    | "check"
    | "warning"
    | "cloud"
    | "terminal"
    | "refresh"
    | "folder"
    | "github"
    | "close"
    | "copy"
    | "play"
    | "plus"
    | "stop"
    | "maximize"
    | "external" = "home";
  export let size = 20;

  const paths: Record<typeof name, string[]> = {
    home: ["M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8", "M3 10a2 2 0 0 1 .71-1.53l7-6a2 2 0 0 1 2.58 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"],
    catalog: ["M3 3h7v7H3Z", "M14 3h7v7h-7Z", "M3 14h7v7H3Z", "M14 14h7v7h-7Z"],
    projects: ["M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"],
    connections: ["M12 22v-5", "M9 8V2", "M15 8V2", "M18 8v5a6 6 0 0 1-12 0V8Z"],
    tools: ["M14.7 6.3a4 4 0 0 0-5-5l2.1 2.1-2.8 2.8-2.1-2.1a4 4 0 0 0 5 5l-7.5 7.5a2.1 2.1 0 0 0 3 3L12.3 11a4 4 0 0 0 5-5Z"],
    settings: ["M21 4h-7", "M10 4H3", "M21 12h-9", "M8 12H3", "M21 20h-5", "M12 20H3", "M14 2v4", "M8 10v4", "M16 18v4"],
    search: ["m21 21-4.35-4.35", "M19 11a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z"],
    chevron: ["m9 18 6-6-6-6"],
    check: ["m5 12 4 4L19 6"],
    warning: ["M12 3 2.5 20h19L12 3Z", "M12 9v4", "M12 17h.01"],
    cloud: ["M17.5 19H6a4 4 0 0 1-.6-7.95A7 7 0 0 1 19 9.5a4.75 4.75 0 0 1-1.5 9.5Z", "m9 14 3-3 3 3", "M12 11v6"],
    terminal: ["m4 6 5 5-5 5", "M11 18h9"],
    refresh: ["M20 6v5h-5", "M4 18v-5h5", "M18.5 9A7 7 0 0 0 6 6.5L4 11", "M5.5 15A7 7 0 0 0 18 17.5l2-4.5"],
    folder: ["M3 6h7l2 2h9v11H3V6Z"],
    github: ["M12 2a10 10 0 0 0-3.16 19.49c.5.09.68-.22.68-.48v-1.69c-2.78.61-3.37-1.18-3.37-1.18-.45-1.17-1.11-1.48-1.11-1.48-.91-.63.07-.62.07-.62 1 .08 1.53 1.04 1.53 1.04.9 1.55 2.35 1.1 2.92.84.09-.66.35-1.1.64-1.35-2.22-.25-4.56-1.12-4.56-5A3.9 3.9 0 0 1 6.68 8c-.1-.26-.45-1.3.1-2.7 0 0 .84-.27 2.75 1.03a9.3 9.3 0 0 1 5 0c1.9-1.3 2.74-1.03 2.74-1.03.55 1.4.2 2.44.1 2.7a3.9 3.9 0 0 1 1.04 2.72c0 3.88-2.35 4.74-4.58 4.99.36.31.68.92.68 1.86v2.76c0 .27.18.58.69.48A10 10 0 0 0 12 2Z"],
    close: ["m6 6 12 12", "m18 6-12 12"],
    copy: ["M8 8h12v12H8V8Z", "M4 16V4h12"],
    play: ["m8 5 11 7-11 7V5Z"],
    plus: ["M12 5v14", "M5 12h14"],
    stop: ["M6 6h12v12H6z"],
    maximize: ["M8 3H3v5", "M16 3h5v5", "M8 21H3v-5", "M16 21h5v-5"],
    external: ["M14 4h6v6", "m20 4-9 9", "M18 13v7H4V6h7"]
  };
</script>

<svg
  class="icon"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.7"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#each paths[name] as path}
    <path d={path}></path>
  {/each}
</svg>
