import { defineConfig } from 'vite'

export default defineConfig({
  build: {
    // ES2020 语法基线,esbuild 负责降级
    target: 'es2020',
    lib: {
      entry: 'src/index.ts',
      formats: ['es'],
      fileName: () => 'x-notify-service-sdk.js',
    },
    minify: true,
  },
})
