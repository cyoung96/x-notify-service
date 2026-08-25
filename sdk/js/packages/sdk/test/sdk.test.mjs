// SDK 语义测试(node --test,直接测 dist 产物;使用独立端口区间,不干扰本机服务)
import assert from 'node:assert/strict'
import http from 'node:http'
import { test, afterEach } from 'node:test'

import { createNoticeBridge } from '../dist/x-notify-service-sdk.js'

const servers = []

// 每个用例独立端口:避免 undici 连接池复用上一用例已关闭服务的陈旧 socket
function mockServer({ app = 'x-notify-service', notifyStatus = 200, port = 24520 } = {}) {
  const calls = { notify: [], close: 0 }
  const server = http.createServer((req, res) => {
    if (req.url === '/health') {
      res.end(JSON.stringify({ app, version: '0.1.0-test', port }))
    } else if (req.url === '/notify') {
      let body = ''
      req.on('data', (c) => (body += c))
      req.on('end', () => {
        calls.notify.push(JSON.parse(body))
        res.statusCode = notifyStatus
        res.end(JSON.stringify({ ok: notifyStatus === 200, via: 'system' }))
      })
    } else if (req.url === '/close') {
      calls.close += 1
      res.end('{"ok":true}')
    } else {
      res.statusCode = 404
      res.end()
    }
  })
  return new Promise((resolve) => server.listen(port, '127.0.0.1', () => resolve({ server, calls, port })))
}

afterEach(() => {
  while (servers.length > 0) {
    servers.pop().close()
  }
})

test('服务未运行:notify 静默失败返回 ok:false,不抛错', async () => {
  const bridge = createNoticeBridge({ basePort: 24590, portRange: 3 })
  const r = await bridge.notify({ title: 't', body: 'b' })
  assert.equal(r.ok, false)
  assert.equal(r.via, undefined)
})

test('伪服务(应用身份不符):discover 拒绝,notify 静默失败', async () => {
  const { server, port } = await mockServer({ app: 'other-service', port: 24530 })
  servers.push(server)
  const bridge = createNoticeBridge({ basePort: port, portRange: 3 })
  assert.equal(await bridge.discover(true), null)
  const r = await bridge.notify({ title: 't' })
  assert.equal(r.ok, false)
})

test('真服务:discover 命中并缓存,notify 送达 snake_case 字段,close 幂等', async () => {
  const { server, calls, port } = await mockServer({ port: 24540 })
  servers.push(server)
  const bridge = createNoticeBridge({ basePort: port, portRange: 3 })

  const base = await bridge.discover(true)
  assert.equal(base, `http://127.0.0.1:${port}`)

  const r = await bridge.notify({ title: '工单', body: '<b>紧急</b>' })
  assert.equal(r.ok, true)
  assert.equal(calls.notify.length, 1)
  assert.deepEqual(calls.notify[0], { title: '工单', body: '<b>紧急</b>' }, '字段应为 snake_case 且透传 HTML')

  await bridge.close()
  await bridge.close()
  assert.equal(calls.close, 2, 'close 每次都请求(幂等由服务端保证)')
})

test('空标题:抛出参数错误(编程错误应显式暴露)', async () => {
  const bridge = createNoticeBridge({ basePort: 24590, portRange: 3 })
  await assert.rejects(() => bridge.notify({ title: ' ' }), /title/)
})
