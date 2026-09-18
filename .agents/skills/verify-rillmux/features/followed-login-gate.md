# followed-login-gate

- **Intent:** A signed-out user opening Followed understands they must log in.
- **Entry:** `/` (Followed), nav label Followed.
- **Source:** `src/pages/BrowsePages.tsx`, `src/locales/en/routes.json` (`followedLoginRequired`).
- **Lane:** web-drivable (signed-out copy). Native shows the same gate without Helix. Credential-dependent live list is not this feature.
- **Drive:** `node .agents/skills/verify-rillmux/scripts/control-rillmux.mjs drive followed-login-gate`
- **Success:** Body contains `Log in to see streams from channels you follow.`
- **Other states:** Logged-in Helix list is desktop/auth-only (blocked on Vite). Empty live list after login is a different copy (`followedEmpty`).
