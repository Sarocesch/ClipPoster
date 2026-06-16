<script setup lang="ts">
import { computed } from 'vue';
import { useRoute } from 'vue-router';

const props = defineProps<{
  to: string;
  label: string;
  icon?: string;
}>();

const route = useRoute();
const isActive = computed(() => route.path === props.to);
</script>

<template>
  <RouterLink
    :to="to"
    class="group flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm transition-colors duration-150"
    :class="isActive
      ? 'bg-orange-600/[0.12] text-orange-300 font-medium'
      : 'text-neutral-500 hover:text-neutral-200 hover:bg-white/[0.04]'"
  >
    <svg
      v-if="icon"
      class="w-[15px] h-[15px] flex-shrink-0 transition-colors duration-150"
      :class="isActive ? 'text-orange-400' : 'text-neutral-700 group-hover:text-neutral-400'"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path :d="icon" />
    </svg>
    <span>{{ label }}</span>
  </RouterLink>
</template>
