import { createRouter, createWebHashHistory } from 'vue-router';

const ClipsView = () => import('./views/ClipsView.vue');
const Settings = () => import('./views/Settings.vue');

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'ClipsView', component: ClipsView },
    { path: '/settings', name: 'Settings', component: Settings },
  ],
});

export default router;
