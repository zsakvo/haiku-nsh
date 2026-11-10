import { definePlugin } from 'haiku';

// Explicitly imported by settings acceptance tests; never installed by the app.
export default definePlugin({
  id: 'settings.fixture',
  name: 'Settings fixture',
  version: '1.0.0',
  track: {
    async streams() {
      return [];
    },
    async get(id) {
      return { id, title: 'Settings fixture track', artists: [] };
    },
  },
});
