<script setup lang="ts">
import { ref } from 'vue';
const props = defineProps<{ text: string }>();
const open = ref(false);
</script>

<template>
  <span class="relative inline-flex items-center">
    <button type="button" class="ml-2 w-5 h-5 rounded-full text-white flex items-center justify-center"
            :style="{ backgroundColor: '#FF6B35' }"
            @click="open = !open" aria-label="Info">i</button>
    <div v-if="open" class="absolute z-50 mt-8 w-72 p-3 rounded shadow-lg border text-sm"
         style="background-color:#0a0a0a;border-color:#262626;color:#e5e5e5;">
      <div class="flex justify-between items-start">
        <p class="pr-6">{{ props.text }}</p>
        <button class="text-neutral-400 hover:text-white" @click="open=false">✕</button>
      </div>
    </div>
  </span>
</template>

