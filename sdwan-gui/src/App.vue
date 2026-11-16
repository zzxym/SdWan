<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import pkg from '~/../package.json'

onBeforeMount(async () => {
  try {
    await getCurrentWindow().setTitle(`Sdwan GUI: v${pkg.version}`)
  } catch (e) {
    console.error('Failed to set window title', e)
  }
})
</script>

<template>
  <Toast position="bottom-right" />
  <RouterView />
</template>
