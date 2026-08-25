import { defineConfig } from 'vite'

export default defineConfig({
  build: {
    // 信创浏览器基线:Chrome 87+(ES2020 语法,esbuild 负责降级)
    target: 'chrome87',
    lib: {
      entry: 'src/index.ts',
      formats: ['es'],
      fileName: () => 'x-notify-service-sdk.js',
    },
    minify: true,
  },
})
