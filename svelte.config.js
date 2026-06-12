import adapter from "@sveltejs/adapter-static";

const config = {
  kit: {
    // Tauri serves a static SPA — prerender everything, no SSR.
    adapter: adapter({
      fallback: "index.html",
    }),
  },
};

export default config;
