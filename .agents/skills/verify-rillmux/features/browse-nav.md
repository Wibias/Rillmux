# browse-nav

- **Intent:** Reach Top streams and About from the shell nav without typing URLs.
- **Entry:** Sidebar Browse/System nav (`src/components/AppShell.tsx`). Labels from `src/locales/en/nav.json`.
- **Lane:** web-drivable. Native-drivable for chrome only; Helix cards need credentials.
- **Drive:** `node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs drive browse-nav`
- **Success:** After clicking Top streams, pathname `/streams`; after About, pathname `/about` and h1 About.
- **Other states:** Top streams Helix grid stays empty without Twitch credentials (do not treat empty cards as a chrome failure).
