import { sveltekit } from "@sveltejs/kit/vite";

export default {
  plugins: [sveltekit()],
  // Tauri expects a fixed port and no clearing of the screen.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
};
