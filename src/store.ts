import { ref, watch } from 'vue';
import { Store } from '@tauri-apps/plugin-store';

let storePromise: Promise<Store> | null = null;
async function getStore(): Promise<Store> {
  if (!storePromise) storePromise = Store.load('settings.bin');
  return storePromise;
}

export function useStoreField<T = string>(key: string, defaultValue: T) {
  const v = ref<T>(defaultValue as T);
  // Load immediately so values restore on first render and after navigation/restart
  (async () => {
    try {
      const store = await getStore();
      const saved = await store.get<T>(key);
      if (saved !== null && saved !== undefined) v.value = saved as T;
    } catch (e) {
      console.error('Store load error', e);
    }
  })();
  watch(v, async (nv) => {
    try {
      const store = await getStore();
      await store.set(key, nv as any);
      await store.save();
    } catch (e) {
      console.error('Store save error', e);
    }
  }, { deep: true });
  return v;
}

export async function getFromStore<T = string>(key: string, fallback: T): Promise<T> {
  const store = await getStore();
  const saved = await store.get<T>(key);
  return (saved ?? fallback) as T;
}

export async function setInStore<T = string>(key: string, value: T) {
  const store = await getStore();
  await store.set(key, value as any);
  await store.save();
}

export { getStore };

